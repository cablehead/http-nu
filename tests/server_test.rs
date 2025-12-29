use assert_cmd::cargo::cargo_bin;
use std::process;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::time::timeout;

struct TestServer {
    child: Child,
    address: String,
}

impl TestServer {
    async fn new(addr: &str, closure: &str, tls: bool) -> Self {
        Self::new_with_plugins(addr, closure, tls, &[]).await
    }

    async fn new_with_plugins(
        addr: &str,
        closure: &str,
        tls: bool,
        plugins: &[std::path::PathBuf],
    ) -> Self {
        let mut cmd = tokio::process::Command::new(cargo_bin("http-nu"));
        cmd.arg("--log-format").arg("jsonl");

        // Add plugin arguments first
        for plugin in plugins {
            cmd.arg("--plugin").arg(plugin);
        }

        cmd.arg(addr).arg(closure);

        if tls {
            cmd.arg("--tls").arg("tests/combined.pem");
        }

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to start http-nu server");

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

        let address = timeout(std::time::Duration::from_secs(5), addr_rx)
            .await
            .expect("Failed to get address from http-nu server")
            .expect("Channel closed before address received");

        Self { child, address }
    }

    async fn curl(&self, path: &str) -> process::Output {
        let mut cmd = tokio::process::Command::new("curl");
        if self.address.starts_with('/') {
            cmd.arg("--unix-socket").arg(&self.address);
            cmd.arg(format!("http://localhost{path}"));
        } else {
            cmd.arg(format!("{}{path}", self.address));
        }
        cmd.output().await.expect("Failed to execute curl")
    }

    async fn curl_tls(&self, path: &str) -> process::Output {
        // Extract port from address format "https://127.0.0.1:8080"
        let port = self.address.split(':').next_back().unwrap();
        let mut cmd = tokio::process::Command::new("curl");
        cmd.arg("--cacert")
            .arg("tests/cert.pem")
            .arg("--resolve")
            .arg(format!("localhost:{port}:127.0.0.1"))
            .arg(format!("https://localhost:{port}{path}"));

        cmd.output().await.expect("Failed to execute curl")
    }

    fn send_ctrl_c(&mut self) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id().expect("child id") as i32);
            kill(pid, Signal::SIGINT).expect("failed to send SIGINT");
        }
        #[cfg(not(unix))]
        {
            // On Windows, use forceful termination since console Ctrl+C handling
            // requires special setup that our server doesn't have
            let _ = self.child.start_kill();
        }
    }

    fn send_sigterm(&mut self) {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(self.child.id().expect("child id") as i32);
            kill(pid, Signal::SIGTERM).expect("failed to send SIGTERM");
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.start_kill();
        }
    }

    async fn wait_for_exit(&mut self) -> std::process::ExitStatus {
        use tokio::time::{timeout, Duration};
        timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("server did not exit in time")
            .expect("failed waiting for child")
    }

    fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if !self.has_exited() {
            let _ = self.child.start_kill();
        }
    }
}

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
        cmd.arg("--log-format").arg("jsonl");
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
        cmd.arg("-s").arg(format!("{}/", self.address));
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
        r#"{{|req| .reverse-proxy "{}" {{ headers: {{ "x-custom-header": "proxy-added" }} }} }}"#,
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
        r#"{{|req| .reverse-proxy "{}" {{ strip_prefix: "/api" }} }}"#,
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
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "{}" }}"#, backend.address);
    let proxy_forward = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the original request body is forwarded.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-d")
        .arg("forwarded")
        .arg(&proxy_forward.address);
    let output = cmd.output().await.expect("Failed to execute curl");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "forwarded");

    // Start a proxy server that overrides the request body.
    let proxy_closure = format!(
        r#"{{|req| "override" | .reverse-proxy "{}" }}"#,
        backend.address
    );
    let proxy_override = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the request body is overridden.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-d")
        .arg("original")
        .arg(&proxy_override.address);
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
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "{}" }}"#, backend.address);
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test that the Host header is forwarded correctly.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg("-H")
        .arg("Host: example.com")
        .arg(&proxy.address);
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
    let proxy_closure = format!(r#"{{|req| .reverse-proxy "{}" }}"#, backend.address);
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // First test: verify backend server streams properly on its own
    println!("Testing backend directly...");
    let backend_start = std::time::Instant::now();
    let mut backend_child = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--raw")
        .arg("-N") // --no-buffer
        .arg(&backend.address)
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
        .arg(&proxy.address)
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
        r#"{{|req| .reverse-proxy "{}" {{ query: ($req.query | upsert "context-id" "smidgeons" | reject "debug") }} }}"#,
        backend.address
    );
    let proxy = TestServer::new("127.0.0.1:0", &proxy_closure, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Test the query parameter modification.
    let mut cmd = tokio::process::Command::new("curl");
    cmd.arg("-s")
        .arg(format!("{}/test?page=1&debug=true&limit=10", proxy.address));

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

/// Tests that inflight requests complete during graceful shutdown.
/// Uses SIGTERM (not SIGINT) to avoid killing nushell jobs immediately.
#[cfg(unix)]
#[tokio::test]
async fn test_graceful_shutdown_waits_for_inflight_requests() {
    // Server with a 500ms delay in the response
    let mut server =
        TestServer::new("127.0.0.1:0", r#"{|req| sleep 500ms; "completed"}"#, false).await;

    // Start a request (will take 500ms to complete)
    // Use --retry and --retry-connrefused to handle slow server startup on CI
    let url = format!("{}/", server.address);
    let request_handle = tokio::spawn(async move {
        tokio::process::Command::new("curl")
            .arg("-s")
            .arg("--retry")
            .arg("3")
            .arg("--retry-delay")
            .arg("1")
            .arg("--retry-connrefused")
            .arg(&url)
            .output()
            .await
            .expect("curl failed")
    });

    // Give the request time to connect and start processing
    // Increased for CI environments (especially macOS) where timing can vary
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Send SIGTERM to trigger graceful shutdown (doesn't kill nushell jobs like SIGINT)
    server.send_sigterm();

    // The request should complete successfully despite shutdown being triggered
    let output = request_handle.await.expect("request task panicked");
    assert!(output.status.success(), "curl failed: {output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(body, "completed");

    // Server should exit cleanly
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

/// Tests that the server supports HTTP/1.1 connections
#[tokio::test]
async fn test_http1_support() {
    let mut server = TestServer::new("127.0.0.1:0", r#"{|req| $req.proto}"#, false).await;

    let output = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--http1.1")
        .arg(format!("{}/", server.address))
        .output()
        .await
        .expect("curl failed");

    assert!(output.status.success(), "curl failed: {output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(body, "HTTP/1.1");

    server.send_sigterm();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

/// Tests that the server supports HTTP/2 connections (h2c - cleartext)
#[tokio::test]
async fn test_http2_support() {
    let mut server = TestServer::new("127.0.0.1:0", r#"{|req| $req.proto}"#, false).await;

    // Use --http2-prior-knowledge for h2c (HTTP/2 without TLS)
    let output = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--http2-prior-knowledge")
        .arg(format!("{}/", server.address))
        .output()
        .await
        .expect("curl failed");

    assert!(output.status.success(), "curl failed: {output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(body, "HTTP/2.0");

    server.send_sigterm();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

/// Tests that HTTP/2 works over TLS (h2 via ALPN)
#[tokio::test]
async fn test_http2_tls_support() {
    let mut server = TestServer::new("127.0.0.1:0", r#"{|req| $req.proto}"#, true).await;

    // Extract port from address format "https://127.0.0.1:8080"
    let port = server.address.split(':').next_back().unwrap();

    // Use --http2 to prefer HTTP/2 via ALPN negotiation
    let output = tokio::process::Command::new("curl")
        .arg("-s")
        .arg("--http2")
        .arg("--cacert")
        .arg("tests/cert.pem")
        .arg("--resolve")
        .arg(format!("localhost:{port}:127.0.0.1"))
        .arg(format!("https://localhost:{port}/"))
        .output()
        .await
        .expect("curl failed");

    assert!(output.status.success(), "curl failed: {output:?}");
    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(body, "HTTP/2.0");

    server.send_sigterm();
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
        .arg(&server.address)
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
        "First SSE chunk took {first_chunk_time:?}, expected < 500ms. SSE compression may be buffering instead of streaming.",
    );

    // Wait for the rest and verify total time is ~600ms (3 more events * 200ms)
    let mut remaining = Vec::new();
    reader.read_to_end(&mut remaining).await.unwrap();
    let total_time = start.elapsed();

    child.wait().await.unwrap();

    // Total time should be ~800ms (4 events * 200ms delay)
    assert!(
        total_time >= std::time::Duration::from_millis(700),
        "Total time {total_time:?} too short, expected ~800ms for streaming",
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
        "SSE brotli streaming verified: first chunk at {first_chunk_time:?}, total {total_time:?}"
    );
}

#[tokio::test]
async fn test_to_sse_command() {
    // Test that `to sse` properly formats records with id, event, data, retry fields
    // and auto-sets the correct headers
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            [
                {id: "1", event: "greeting", data: "hello"}
                {id: "2", event: "update", data: "world", retry: 5000}
                {data: {count: 42}}
            ] | to sse
        }"#,
        false,
    )
    .await;

    // Use curl with -i to get headers
    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg("-i")
        .arg(&server.address)
        .output()
        .expect("curl failed");

    assert!(output.status.success());
    let response = String::from_utf8_lossy(&output.stdout);

    // Check headers
    assert!(
        response.contains("content-type: text/event-stream"),
        "Missing content-type header"
    );
    assert!(
        response.contains("cache-control: no-cache"),
        "Missing cache-control header"
    );
    assert!(
        response.contains("connection: keep-alive"),
        "Missing connection header"
    );

    // Check SSE event formatting
    assert!(response.contains("id: 1"), "Missing id: 1");
    assert!(
        response.contains("event: greeting"),
        "Missing event: greeting"
    );
    assert!(response.contains("data: hello"), "Missing data: hello");

    assert!(response.contains("id: 2"), "Missing id: 2");
    assert!(response.contains("event: update"), "Missing event: update");
    assert!(response.contains("data: world"), "Missing data: world");
    assert!(response.contains("retry: 5000"), "Missing retry: 5000");

    // Check JSON serialization of record data
    assert!(
        response.contains(r#"data: {"count":42}"#),
        "Missing JSON data"
    );
}

#[tokio::test]
async fn test_to_sse_ignores_null_fields() {
    // Test that `to sse` ignores null values for optional fields
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            [
                {event: "test", data: "hello", id: null, retry: null}
                {event: "with-id", data: "world", id: "123", retry: null}
                {event: "with-retry", data: "foo", id: null, retry: 5000}
            ] | to sse
        }"#,
        false,
    )
    .await;

    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg(&server.address)
        .output()
        .expect("curl failed");

    assert!(output.status.success());
    let response = String::from_utf8_lossy(&output.stdout);

    // First event: no id or retry lines
    assert!(response.contains("event: test"), "Missing event: test");
    assert!(response.contains("data: hello"), "Missing data: hello");

    // Second event: has id, no retry
    assert!(response.contains("id: 123"), "Missing id: 123");
    assert!(
        response.contains("event: with-id"),
        "Missing event: with-id"
    );

    // Third event: has retry, no id
    assert!(response.contains("retry: 5000"), "Missing retry: 5000");
    assert!(
        response.contains("event: with-retry"),
        "Missing event: with-retry"
    );

    // Should not contain empty id/retry lines or "null"
    assert!(!response.contains("id: \n"), "Should not contain empty id");
    assert!(
        !response.contains("retry: \n"),
        "Should not contain empty retry"
    );
    assert!(
        !response.contains("id: null"),
        "Should not contain id: null"
    );
    assert!(
        !response.contains("retry: null"),
        "Should not contain retry: null"
    );
}

#[tokio::test]
async fn test_to_sse_data_list() {
    // Test that `to sse` handles data as a list of items
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            [
                {event: "test", data: ["line1", "line2", "line3"]}
                {event: "embedded", data: ["first", "has\nnewline", "last"]}
                {event: "mixed", data: ["string", {num: 42}, "another"]}
            ] | to sse
        }"#,
        false,
    )
    .await;

    let output = std::process::Command::new("curl")
        .arg("-s")
        .arg(&server.address)
        .output()
        .expect("curl failed");

    assert!(output.status.success());
    let response = String::from_utf8_lossy(&output.stdout);

    // List items become separate data lines
    assert!(response.contains("data: line1"), "Missing data: line1");
    assert!(response.contains("data: line2"), "Missing data: line2");
    assert!(response.contains("data: line3"), "Missing data: line3");

    // Embedded newlines get split into separate data lines
    assert!(response.contains("data: first"), "Missing data: first");
    assert!(response.contains("data: has"), "Missing data: has");
    assert!(response.contains("data: newline"), "Missing data: newline");
    assert!(response.contains("data: last"), "Missing data: last");

    // Non-string items get JSON serialized
    assert!(response.contains("data: string"), "Missing data: string");
    assert!(
        response.contains(r#"data: {"num":42}"#),
        "Missing JSON data in list"
    );
    assert!(response.contains("data: another"), "Missing data: another");
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

    // 8. Send script "3" without null terminator, then close stdin
    // The script should be processed when stdin closes (EOF acts as terminator)
    {
        let stdin = server.stdin.as_mut().unwrap();
        stdin.write_all(br#"{|req| "3"}"#).await.unwrap();
        stdin.flush().await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    server.close_stdin().await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 9. Curl should return "3" (script was processed on stdin close)
    assert_eq!(server.curl_get().await, "3");
}

/// Tests that missing Host header returns 500 error
#[tokio::test]
async fn test_server_missing_host_header() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut server = TestServer::new(
        "127.0.0.1:0",
        "{|req| let host = $req.headers.host; $\"Host: ($host)\" }",
        false,
    )
    .await;

    // Use a raw TCP connection so the test doesn't depend on `nc`
    // Strip the http:// prefix from address for raw TCP connection
    let addr = server.address.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(addr).await.expect("connect to server");
    stream
        .write_all(b"GET / HTTP/1.0\r\n\r\n")
        .await
        .expect("send request");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("500"), "expected 500 status, got: {text}");

    server.send_sigterm();
    let status = server.wait_for_exit().await;
    assert!(status.success());
}

/// Tests basic router exact path matching
#[tokio::test]
async fn test_router_exact_path() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {path: "/health"} {|req ctx| "OK"})
                (route {path: "/status"} {|req ctx| "RUNNING"})
                (route true {|req ctx| "NOT FOUND"})
            ]
        }"#,
        false,
    )
    .await;

    let output = server.curl("/health").await;
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "OK");

    let output = server.curl("/status").await;
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "RUNNING");

    let output = server.curl("/unknown").await;
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "NOT FOUND");
}

/// Tests router path parameter extraction
#[tokio::test]
async fn test_router_path_parameters() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {path-matches: "/users/:id"} {|req ctx| $"User: ($ctx.id)"})
                (route {path-matches: "/posts/:userId/:postId"} {|req ctx| $"Post ($ctx.postId) by user ($ctx.userId)"})
                (route true {|req ctx| "NOT FOUND"})
            ]
        }"#,
        false,
    )
    .await;

    let output = server.curl("/users/alice").await;
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "User: alice"
    );

    let output = server.curl("/posts/bob/123").await;
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Post 123 by user bob"
    );
}

/// Tests router method matching
#[tokio::test]
async fn test_router_method_matching() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {method: "GET", path: "/items"} {|req ctx| "LIST"})
                (route {method: "POST", path: "/items"} {|req ctx| "CREATE"})
                (route true {|req ctx| "NOT FOUND"})
            ]
        }"#,
        false,
    )
    .await;

    let output = server.curl("/items").await;
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "LIST");

    let output = tokio::process::Command::new("curl")
        .arg("-X")
        .arg("POST")
        .arg(format!("{}/items", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "CREATE");
}

/// Tests router header matching
#[tokio::test]
async fn test_router_header_matching() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {has-header: {accept: "application/json"}} {|req ctx| "JSON"})
                (route true {|req ctx| "OTHER"})
            ]
        }"#,
        false,
    )
    .await;

    let output = tokio::process::Command::new("curl")
        .arg("-H")
        .arg("Accept: application/json")
        .arg(format!("{}/", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "JSON");

    let output = tokio::process::Command::new("curl")
        .arg("-H")
        .arg("Accept: text/html")
        .arg(format!("{}/", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "OTHER");
}

/// Tests router combined conditions (method + path + headers)
#[tokio::test]
async fn test_router_combined_conditions() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {
                    method: "POST"
                    path-matches: "/api/:version/data"
                    has-header: {accept: "application/json"}
                } {|req ctx| $"API ($ctx.version) JSON"})
                (route true {|req ctx| "FALLBACK"})
            ]
        }"#,
        false,
    )
    .await;

    let output = tokio::process::Command::new("curl")
        .arg("-X")
        .arg("POST")
        .arg("-H")
        .arg("Accept: application/json")
        .arg(format!("{}/api/v1/data", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "API v1 JSON"
    );

    // Wrong method
    let output = tokio::process::Command::new("curl")
        .arg("-H")
        .arg("Accept: application/json")
        .arg(format!("{}/api/v1/data", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "FALLBACK");
}

/// Tests router 501 response when no routes match
#[tokio::test]
async fn test_router_no_match_501() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            use http-nu/router *
            dispatch $req [
                (route {method: "POST", path: "/users"} {|req ctx| "CREATED"})
            ]
        }"#,
        false,
    )
    .await;

    let output = tokio::process::Command::new("curl")
        .arg("-i")
        .arg(format!("{}/unknown", server.address))
        .output()
        .await
        .expect("curl failed");
    assert!(output.status.success());
    let response = String::from_utf8_lossy(&output.stdout);
    assert!(response.contains("501 Not Implemented"));
    assert!(response.contains("No route configured"));
}

/// Tests that plugins can be loaded and their commands used
#[tokio::test]
async fn test_plugin_loading() {
    let plugin_path = cargo_bin("nu_plugin_test");
    let server = TestServer::new_with_plugins(
        "127.0.0.1:0",
        "{|req| test-plugin-cmd}",
        false,
        &[plugin_path],
    )
    .await;

    let output = server.curl("/").await;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "PLUGIN_WORKS");
}
