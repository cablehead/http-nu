use tokio::time::Duration;

mod common;
use common::TestServer;

#[tokio::test]
async fn test_server_missing_host_header() {
    let server = TestServer::new(
        "127.0.0.1:0",
        "{|req| let host = $req.headers.host; $\"Host: ($host)\" }",
        false,
    )
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let addr = server.address.clone();
    let mut parts = addr.split(':');
    let host = parts.next().unwrap();
    let port = parts.next().unwrap();
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "printf 'GET / HTTP/1.0\\r\\n\\r\\n' | nc {} {}",
            host, port
        ))
        .output()
        .await
        .expect("run nc");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("500"), "expected 500 status, got: {text}");
}
