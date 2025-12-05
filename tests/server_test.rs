mod common;
use common::TestServer;

use assert_cmd::cargo::cargo_bin;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::time::timeout;

/// Test server with stdin support for dynamic script reloading
struct TestServerWithStdin {
    child: Child,
    address: String,
    stdin: Option<ChildStdin>,
}

impl TestServerWithStdin {
    /// Spawn the server process and return handles, but don't send any script yet.
    /// The server will wait for a valid script before emitting the "start" message.
    fn spawn(addr: &str, tls: bool) -> (Child, ChildStdin, tokio::sync::oneshot::Receiver<String>) {
        let mut cmd = tokio::process::Command::new(cargo_bin("http-nu"));
        cmd.arg(addr).arg("-");

        if tls {
            cmd.arg("--tls").arg("tests/combined.pem");
        }

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to start http-nu server");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        // Spawn tasks to read output
        let mut addr_tx = Some(addr_tx);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[HTTP-NU STDOUT] {line}");
                if addr_tx.is_some() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(addr_str) = json.get("address").and_then(|a| a.as_str()) {
                            if let Some(tx) = addr_tx.take() {
                                let _ = tx.send(addr_str.trim().to_string());
                            }
                        }
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[HTTP-NU STDERR] {line}");
            }
        });

        (child, stdin, addr_rx)
    }

    async fn write_script(&mut self, script: &str) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin
            .write_all(script.as_bytes())
            .await
            .expect("Failed to write script to stdin");
        stdin
            .write_all(b"\0")
            .await
            .expect("Failed to write null terminator to stdin");
        stdin.flush().await.expect("Failed to flush stdin");
    }

    async fn close_stdin(&mut self) {
        self.stdin.take();
    }

    async fn curl_get(&self) -> String {
        let mut cmd = tokio::process::Command::new("curl");
        cmd.arg("-s").arg(format!("http://{}/", self.address));
        let output = cmd.output().await.expect("Failed to execute curl");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

impl Drop for TestServerWithStdin {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

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
async fn test_server_static_files_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let index_path = tmp.path().join("index.html");
    std::fs::write(&index_path, "fallback page").unwrap();

    let closure = format!(
        "{{|req| .static '{}' $req.path --fallback 'index.html' }}",
        tmp.path().to_str().unwrap()
    );
    let server = TestServer::new("127.0.0.1:0", &closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let output = server.curl("/missing/route").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "fallback page");
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

#[tokio::test]
async fn test_server_reverse_proxy_custom_query() {
    // Start a backend server that echoes the query parameters it receives.
    let backend = TestServer::new("127.0.0.1:0", r#"{|req| $req.query | to json}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start a proxy server that modifies query parameters.
    let proxy_closure = format!(
        r#"{{|req| .reverse-proxy "http://{}" {{ query: ($req.query | upsert "context-id" "smidgeons" | reject "debug") }} }}"#,
        backend.address
    );
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test the query parameter modification.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s").arg(format!(
        "http://{}/test?page=1&debug=true&limit=10",
        proxy.address
    ));

    let output = cmd.output().await.unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Verify the query was modified: context-id added, debug removed, others preserved
    assert_eq!(json["context-id"], "smidgeons");
    assert_eq!(json["page"], "1");
    assert_eq!(json["limit"], "10");
    assert!(json.get("debug").is_none()); // debug should be removed
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

#[tokio::test]
async fn test_parse_error_ansi_formatting() {
    use assert_cmd::cargo::cargo_bin;

    let output = tokio::process::Command::new(cargo_bin("http-nu"))
        .arg("127.0.0.1:0")
        .arg("{|req| use nonexistent oauth}")
        .output()
        .await
        .expect("Failed to execute http-nu");

    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT contain escaped ANSI sequences
    assert!(
        !stderr.contains(r"\u{1b}"),
        "stderr contains escaped ANSI codes: {stderr}"
    );

    // Should contain the error text
    assert!(
        stderr.contains("Parse error") || stderr.contains("ExportNotFound"),
        "stderr missing expected error text: {stderr}"
    );
}

#[tokio::test]
async fn test_sse_brotli_compression_streams_immediately() {
    // Test that SSE responses with brotli compression stream events immediately,
    // not buffered until the stream ends.
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            .response {status: 200, headers: {"Content-Type": "text/event-stream"}}
            1..4 | each {|i|
                sleep 200ms
                $"data: event-($i)\n\n"
            }
        }"#,
        false,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Start curl with brotli compression, reading raw compressed bytes
    let start = std::time::Instant::now();
    let mut child = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("-N") // --no-buffer, stream data as it arrives
        .arg("-H")
        .arg("Accept-Encoding: br")
        .arg(format!("http://{}", server.address))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start curl");

    let stdout = child.stdout.take().unwrap();
    use tokio::io::AsyncReadExt;
    let mut reader = stdout;

    // Read first chunk - should arrive after ~200ms (first event), not ~800ms (all events)
    let mut first_chunk = vec![0u8; 64];
    let n = reader.read(&mut first_chunk).await.unwrap();
    let first_chunk_time = start.elapsed();

    assert!(n > 0, "Expected to receive data");

    // First chunk should arrive well before all events would complete (~800ms)
    // Give some margin for startup overhead, but it should be < 500ms
    assert!(
        first_chunk_time < std::time::Duration::from_millis(500),
        "First SSE chunk took {:?}, expected < 500ms. SSE compression may be buffering instead of streaming.",
        first_chunk_time
    );

    // Wait for the rest and verify total time is ~600ms (3 more events * 200ms)
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).await.unwrap();
    let total_time = start.elapsed();

    child.wait().await.unwrap();

    // Total time should be ~800ms (4 events * 200ms delay)
    assert!(
        total_time >= std::time::Duration::from_millis(700),
        "Total time {:?} too short, expected ~800ms for streaming",
        total_time
    );

    // Decompress and verify we got all events
    let all_compressed: Vec<u8> = first_chunk[..n]
        .iter()
        .chain(remaining.iter())
        .copied()
        .collect();
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut &all_compressed[..], &mut decompressed)
        .expect("Failed to decompress brotli SSE data");

    let text = String::from_utf8(decompressed).expect("Invalid UTF-8");
    assert!(text.contains("data: event-1"), "Missing event-1");
    assert!(text.contains("data: event-2"), "Missing event-2");
    assert!(text.contains("data: event-3"), "Missing event-3");

    println!(
        "SSE brotli streaming verified: first chunk at {:?}, total {:?}",
        first_chunk_time, total_time
    );
}

#[tokio::test]
async fn test_dynamic_script_reload() {
    // Spawn server process - it will wait for a valid script
    let (child, mut stdin, addr_rx) = TestServerWithStdin::spawn("127.0.0.1:0", false);

    // Helper to write a script to stdin
    async fn write_script(stdin: &mut ChildStdin, script: &str) {
        stdin.write_all(script.as_bytes()).await.unwrap();
        stdin.write_all(b"\0").await.unwrap();
        stdin.flush().await.unwrap();
        // Give tokio a chance to actually send the data to the child process
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // 1. Send bad script (actual parse error) - server should reject it and keep waiting
    write_script(&mut stdin, "{|req| { unclosed").await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 2. Send good script "1" - server should start
    write_script(&mut stdin, r#"{|req| "1"}"#).await;

    // Wait for server to start
    let address = timeout(std::time::Duration::from_secs(5), addr_rx)
        .await
        .expect("Server didn't start")
        .expect("Channel closed");

    let mut server = TestServerWithStdin {
        child,
        address,
        stdin: Some(stdin),
    };

    // 3. Curl should return "1"
    assert_eq!(server.curl_get().await, "1");

    // 4. Send bad script (different parse error) - server should reject and keep "1"
    server.write_script("{|req| ] unbalanced").await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 5. Curl should still return "1"
    assert_eq!(server.curl_get().await, "1");

    // 6. Send good script "2"
    server.write_script(r#"{|req| "2"}"#).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 7. Curl should return "2"
    assert_eq!(server.curl_get().await, "2");

    // 8. Close stdin
    server.close_stdin().await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 9. Curl should still return "2" (server keeps running after stdin EOF)
    assert_eq!(server.curl_get().await, "2");
}
