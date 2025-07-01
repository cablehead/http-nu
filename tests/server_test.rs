mod common;
use common::TestServer;

#[tokio::test]
async fn test_server_startup_and_shutdown() {
    let _server = TestServer::new("127.0.0.1:0", "{|req| $req.method}", false).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_server_unix_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let socket_path = tmp.path().join("test.sock");
    let socket_path_str = socket_path.to_str().unwrap();
    let server = TestServer::new(socket_path_str, "{|req| $req.method}", false).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let output = server.curl("").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "GET");
}

#[tokio::test]
async fn test_server_tcp_socket() {
    let server = TestServer::new("127.0.0.1:0", "{|req| $req.method}", false).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let output = server.curl("").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "GET");
}

#[tokio::test]
async fn test_server_tls_socket() {
    let server = TestServer::new("127.0.0.1:0", "{|req| $req.method}", true).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let output = server.curl_tls("").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "GET");
}

#[tokio::test]
async fn test_server_static_files() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "Hello from static file").unwrap();

    let closure = format!(
        "{{|req| .static '{}' $req.path }}",
        tmp.path().to_str().unwrap()
    );
    let server = TestServer::new("127.0.0.1:0", &closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let output = server.curl("/test.txt").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "Hello from static file");
}

#[tokio::test]
async fn test_server_reverse_proxy() {
    // Start a backend server that echoes the method, path, query, and a custom header.
    let backend = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            let method = $req.method
            let path = $req.path
            let query = ($req.query | get foo | default 'none')
            let header = ($req.headers | get "x-custom-header" | default "not-found")
            $"Backend: ($method) ($path) ($query) ($header)"
        }"#,
        false,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server that forwards to the backend with a custom header.
    let proxy_closure = format!(
        r#"{{|req| .reverse-proxy "http://{}" {{ headers: {{ "x-custom-header": "proxy-added" }} }} }}"#,
        backend.address
    );
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test basic proxying with a query parameter.
    let output = proxy.curl("/test?foo=bar").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "Backend: GET /test bar proxy-added");
}

#[tokio::test]
async fn test_server_reverse_proxy_strip_prefix() {
    // Start a backend server that returns the request path.
    let backend = TestServer::new("127.0.0.1:0", r#"{|req| $"Path: ($req.path)"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server with prefix stripping.
    let proxy_closure = format!(
        r#"{{|req| .reverse-proxy "http://{}" {{ strip_prefix: "/api" }} }}"#,
        backend.address
    );
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the /api prefix is stripped from the request path.
    let output = proxy.curl("/api/users").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "Path: /users");
}

#[tokio::test]
async fn test_server_reverse_proxy_body_handling() {
    // Start a backend server that echoes the request body.
    let backend = TestServer::new("127.0.0.1:0", r#"{|req| $in}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server that forwards the original request body.
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "http://{}" }}"#, backend.address);
    let proxy_forward = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the original request body is forwarded.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-d")
        .arg("forwarded")
        .arg(format!("http://{}", proxy_forward.address));
    let output = cmd.output().await.expect("Failed to execute curl");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "forwarded");

    // Start a proxy server that overrides the request body.
    let proxy_closure = format!(
        r#"{{|req| "override" | .reverse-proxy "http://{}" }}"#,
        backend.address
    );
    let proxy_override = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the request body is overridden.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-d")
        .arg("original")
        .arg(format!("http://{}", proxy_override.address));
    let output = cmd.output().await.expect("Failed to execute curl");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "override");
}

#[tokio::test]
async fn test_server_reverse_proxy_host_header() {
    // Start a backend server that echoes the Host header.
    let backend =
        TestServer::new("127.0.0.1:0", r#"{|req| $req.headers | get "host"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server.
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "http://{}" }}"#, backend.address);
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the Host header is forwarded correctly.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-H")
        .arg("Host: example.com")
        .arg(format!("http://{}", proxy.address));
    let output = cmd.output().await.expect("Failed to execute curl");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "example.com");
}

#[tokio::test]
async fn test_reverse_proxy_streaming() {
    // Start a backend server that streams data with delays
    let backend = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            .response {status: 200}
            1..3 | each {|i|
                sleep 100ms
                $"chunk-($i)\n"
            }
        }"#,
        false,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "http://{}" }}"#, backend.address);
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // First test: verify backend server streams properly on its own
    println!("Testing backend directly...");
    let backend_start = std::time::Instant::now();
    let mut backend_child = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--raw")
        .arg("-N") // --no-buffer
        .arg(format!("http://{}", backend.address))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start curl for backend");

    let backend_stdout = backend_child.stdout.take().unwrap();
    use tokio::io::AsyncReadExt;
    let mut backend_reader = backend_stdout;
    let mut backend_first_byte = [0u8; 1];

    backend_reader
        .read_exact(&mut backend_first_byte)
        .await
        .unwrap();
    let backend_first_byte_time = backend_start.elapsed();

    let mut backend_remaining = Vec::new();
    backend_reader
        .read_to_end(&mut backend_remaining)
        .await
        .unwrap();
    let backend_total_time = backend_start.elapsed();

    backend_child.wait().await.unwrap();

    println!(
        "Backend - First byte: {:?}, Total: {:?}, Diff: {:?}",
        backend_first_byte_time,
        backend_total_time,
        backend_total_time.saturating_sub(backend_first_byte_time)
    );

    // Let's see what data we actually got
    let all_backend_data = [&backend_first_byte[..], &backend_remaining[..]].concat();
    println!(
        "Backend data: {:?}",
        String::from_utf8_lossy(&all_backend_data)
    );

    // Test to prove reverse proxy streams correctly
    // We'll measure when first byte arrives vs when request completes
    let start = std::time::Instant::now();
    let mut child = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--raw") // Don't parse chunked encoding
        .arg("-N") // --no-buffer
        .arg(format!("http://{}", proxy.address))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start curl");

    // Read output as it arrives
    let stdout = child.stdout.take().unwrap();
    let mut reader = stdout;
    let mut first_byte = [0u8; 1];

    // Measure when first byte arrives
    reader.read_exact(&mut first_byte).await.unwrap();
    let first_byte_time = start.elapsed();

    // Read remaining output
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).await.unwrap();
    let total_time = start.elapsed();

    child.wait().await.unwrap();

    println!("First byte at: {first_byte_time:?}, Total time: {total_time:?}");

    // If proxy were streaming: first byte ~100ms, total ~300ms
    let time_difference = total_time.saturating_sub(first_byte_time);

    // Total time should be at least the backend processing time
    assert!(total_time >= std::time::Duration::from_millis(280));

    // For true streaming, there should be at least 150ms between first byte and completion
    assert!(
        time_difference >= std::time::Duration::from_millis(150),
        "Expected at least 150ms between first byte and completion for streaming. Got: {time_difference:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_server_tcp_graceful_shutdown() {
    let mut server = TestServer::new("127.0.0.1:0", "{|req| $req.method}", false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    server.send_ctrl_c();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

#[cfg(unix)]
#[tokio::test]
async fn test_server_tls_graceful_shutdown() {
    let mut server = TestServer::new("127.0.0.1:0", "{|req| $req.method}", true).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    server.send_ctrl_c();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

#[cfg(unix)]
#[tokio::test]
async fn test_server_unix_graceful_shutdown() {
    let tmp = tempfile::tempdir().unwrap();
    let socket_path = tmp.path().join("test_sigint.sock");
    let socket_path_str = socket_path.to_str().unwrap();
    let mut server = TestServer::new(socket_path_str, "{|req| $req.method}", false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    server.send_ctrl_c();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}
