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

    // Use a raw TCP connection so the test doesn't depend on `nc`
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(&server.address)
        .await
        .expect("connect to server");
    stream
        .write_all(b"GET / HTTP/1.0\r\n\r\n")
        .await
        .expect("send request");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("500"), "expected 500 status, got: {text}");
}
