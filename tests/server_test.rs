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
