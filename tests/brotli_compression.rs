mod common;
use common::TestServer;

#[tokio::test]
async fn test_brotli_encoding_br() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| "Hello, World!"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept-Encoding: br").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        response.to_lowercase().contains("content-encoding: br"),
        "Response should have Content-Encoding: br header"
    );
}

#[tokio::test]
async fn test_brotli_encoding_brotli() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| "Hello, World!"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept-Encoding: brotli").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        response.to_lowercase().contains("content-encoding: br"),
        "Response should have Content-Encoding: br header"
    );
}

#[tokio::test]
async fn test_brotli_no_compression_without_header() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| "Hello, World!"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept: text/html").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        !response.to_lowercase().contains("content-encoding: br"),
        "Response should not have Content-Encoding: br header without Accept-Encoding"
    );
    assert!(
        response.contains("Hello, World!"),
        "Response body should be uncompressed"
    );
}

#[tokio::test]
async fn test_brotli_no_compression_with_gzip_only() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| "Hello, World!"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept-Encoding: gzip").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        !response.to_lowercase().contains("content-encoding: br"),
        "Response should not have brotli encoding when only gzip is requested"
    );
}

#[tokio::test]
async fn test_brotli_decompression() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req| "This is a longer message that should compress well with brotli compression!"}"#,
        false,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_compressed("").await;
    assert!(output.status.success());

    let body = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        body.trim(),
        "This is a longer message that should compress well with brotli compression!",
        "curl --compressed should decompress the brotli response"
    );
}

#[tokio::test]
async fn test_brotli_empty_body() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| ""}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept-Encoding: br").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        !response.to_lowercase().contains("content-encoding: br"),
        "Empty bodies should not get the Content-Encoding header"
    );
}

#[tokio::test]
async fn test_brotli_with_custom_headers() {
    let server = TestServer::new(
        "127.0.0.1:0",
        r#"{|req|
            .response {
                headers: {
                    "X-Custom-Header": "test-value"
                }
            }
            "Compressed content with custom headers"
        }"#,
        false,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server.curl_with_header("", "Accept-Encoding: br").await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        response.to_lowercase().contains("content-encoding: br"),
        "Should have Content-Encoding header"
    );
    assert!(
        response
            .to_lowercase()
            .contains("x-custom-header: test-value"),
        "Custom headers should be preserved"
    );
}

#[tokio::test]
async fn test_brotli_multiple_encodings() {
    let server = TestServer::new("127.0.0.1:0", r#"{|req| "Test content"}"#, false).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let output = server
        .curl_with_header("", "Accept-Encoding: gzip, deflate, br")
        .await;
    assert!(output.status.success());

    let response = String::from_utf8_lossy(&output.stdout);
    assert!(
        response.to_lowercase().contains("content-encoding: br"),
        "Should use brotli when multiple encodings are accepted"
    );
}
