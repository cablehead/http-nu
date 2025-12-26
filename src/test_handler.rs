use std::sync::Arc;
use std::time::Instant;

use arc_swap::ArcSwap;
use http_body_util::{BodyExt, Empty, Full};
use hyper::{body::Bytes, Request};
use tokio::time::Duration;

use crate::commands::{MjCommand, ResponseStartCommand, StaticCommand, ToSse};
use crate::handler::handle;

#[tokio::test]
async fn test_handle() {
    let engine = test_engine(r#"{|req| "hello world" }"#);

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(Arc::new(ArcSwap::from_pointee(engine)), None, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("hello world"));
}

#[tokio::test]
async fn test_handle_with_response_start() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
        match $req {
            {uri: "/resource" method: "POST"} => {
                .response {
                    status: 201
                    headers: {
                    "Content-Type": "text/plain"
                    "X-Custom": "test"
                    }
                }
                "created resource"
            }
        }
    }"#,
    )));

    // Test successful POST to /resource
    let req = Request::builder()
        .method("POST")
        .uri("/resource")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine.clone(), None, req).await.unwrap();

    // Verify response metadata
    assert_eq!(resp.status(), 201);
    assert_eq!(resp.headers()["content-type"], "text/plain");
    assert_eq!(resp.headers()["x-custom"], "test");

    // Verify body
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        "created resource"
    );
}

#[tokio::test]
async fn test_handle_post() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(r#"{|req| $in }"#)));

    // Create POST request with a body
    let body = "Hello from the request body!";
    let req = Request::builder()
        .method("POST")
        .uri("/echo")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();

    // Verify response status
    assert_eq!(resp.status(), 200);

    // Verify body is echoed back
    let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(String::from_utf8(resp_body.to_vec()).unwrap(), body);
}

#[tokio::test]
async fn test_handle_streaming() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            1..3 | each { |n| sleep 0.1sec; $n }
        }"#,
    )));

    let req = Request::builder()
        .method("GET")
        .uri("/stream")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let start_time = Instant::now();
    let mut collected = Vec::new();

    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                if let Some(data) = frame.data_ref() {
                    let chunk_str = String::from_utf8(data.to_vec()).unwrap();
                    let elapsed = start_time.elapsed();
                    collected.push((chunk_str.trim().to_string(), elapsed));
                }
            }
            Some(Err(e)) => panic!("Error reading frame: {e}"),
            None => break,
        }
    }

    // Should have 3 chunks
    assert_eq!(collected.len(), 3);
    assert_timing_sequence(&collected);
}

fn assert_timing_sequence(timings: &[(String, Duration)]) {
    // Check values arrive in sequence
    for (i, (value, _)) in timings.iter().enumerate() {
        assert_eq!(
            value,
            &(i + 1).to_string(),
            "Values should arrive in sequence"
        );
    }

    // Check each gap is roughly 100ms
    for i in 1..timings.len() {
        let gap = timings[i].1 - timings[i - 1].1;
        assert!(
            gap >= Duration::from_millis(50) && gap <= Duration::from_millis(300),
            "Gap between chunk {} and {} was {:?}, expected ~100ms",
            i,
            i + 1,
            gap
        );
    }
}

#[tokio::test]
async fn test_content_type_precedence() {
    // 1. Explicit header should take precedence
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
           .response {headers: {"Content-Type": "text/plain"}}
           {foo: "bar"}
       }"#,
    )));
    let req1 = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let resp1 = handle(engine.clone(), None, req1).await.unwrap();
    assert_eq!(resp1.headers()["content-type"], "text/plain");

    // 2. Pipeline metadata
    let req2 = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req| ls | to yaml }"#,
    )));
    let resp2 = handle(engine.clone(), None, req2).await.unwrap();
    assert_eq!(resp2.headers()["content-type"], "application/yaml");

    // 3. Record defaults to JSON
    let req3 = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req| {foo: "bar"} }"#,
    )));
    let resp3 = handle(engine.clone(), None, req3).await.unwrap();
    assert_eq!(resp3.headers()["content-type"], "application/json");

    // 4. Plain text defaults to text/html
    let req4 = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req| "Hello World"}"#,
    )));
    let resp4 = handle(engine.clone(), None, req4).await.unwrap();
    assert_eq!(resp4.headers()["content-type"], "text/html; charset=utf-8");
}

#[tokio::test]
async fn test_handle_bytestream() {
    // `to csv` returns a ByteStream with content-type text/csv
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req| ls | to csv }"#,
    )));

    let req = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();

    // Verify CSV content type
    assert_eq!(resp.headers()["content-type"], "text/csv");

    // Collect and verify body
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let content = String::from_utf8(body.to_vec()).unwrap();

    // Basic CSV format validation
    assert!(content.contains("name"));
    assert!(content.contains("type"));
    assert!(content.contains(","));
}

#[tokio::test]
async fn test_handle_preserve_preamble() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"
        def do-foo [more: string] {
          "foo" + $more
        }

        {|req|
          do-foo $req.path
        }
        "#,
    )));

    let req = Request::builder()
        .uri("/bar")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();

    // Collect and verify body
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let content = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(content, "foo/bar");
}

#[tokio::test]
async fn test_handle_static() {
    let tmp = tempfile::tempdir().unwrap();
    let static_dir = tmp.path().join("static");
    std::fs::create_dir(&static_dir).unwrap();

    let css = "body { background: blue; }";
    std::fs::write(static_dir.join("styles.css"), css).unwrap();

    let engine = Arc::new(ArcSwap::from_pointee(test_engine(&format!(
        r#"{{|req| .static '{}' $req.path }}"#,
        static_dir.to_str().unwrap()
    ))));

    let req = Request::builder()
        .uri("/styles.css")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "text/css");

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), css);
}

fn test_engine(script: &str) -> crate::Engine {
    let mut engine = crate::Engine::new().unwrap();
    engine
        .add_commands(vec![
            Box::new(ResponseStartCommand::new()),
            Box::new(StaticCommand::new()),
            Box::new(ToSse {}),
            Box::new(MjCommand::new()),
        ])
        .unwrap();
    engine.parse_closure(script).unwrap();
    engine
}

#[tokio::test]
async fn test_handle_binary_value() {
    // Test data: simple binary content (PNG-like header and some bytes)
    let expected_binary = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0xFF, 0xAA, 0xBB, 0xCC, 0xDD, // Additional bytes
    ];

    // Create engine that returns a binary value directly
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
        .response {
            headers: {"Content-Type": "application/octet-stream"}
        }
        0x[89 50 4E 47 0D 0A 1A 0A FF AA BB CC DD]  # Creates a Binary value type
    }"#,
    )));

    let req = Request::builder()
        .uri("/binary-value")
        .body(Empty::<Bytes>::new())
        .unwrap();

    // Currently this will panic, but after fixing it should return a response
    let resp = handle(engine, None, req).await.unwrap();

    // Assert proper content type
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "application/octet-stream");

    // Assert binary content matches exactly
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.to_vec(), expected_binary);
}

#[tokio::test]
async fn test_handle_missing_header_error() {
    // Test script that tries to access missing header - reproduces the exact error from logs
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            let host = $req.headers.host
            $"Host: ($host)"
        }"#,
    )));

    // Create request without host header
    let req = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    // This should fail currently - the Nu script tries to access missing column 'host'
    let resp = handle(engine, None, req).await.unwrap();

    // After fixing, this should return 500 with error message instead of hanging
    assert_eq!(resp.status(), 500);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Script error"));
}

#[tokio::test]
async fn test_handle_script_panic() {
    // Test script that panics deliberately
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            error make {msg: "Deliberate panic for testing"}
        }"#,
    )));

    let req = Request::builder()
        .uri("/panic")
        .body(Empty::<Bytes>::new())
        .unwrap();

    // This should return 500 instead of hanging/crashing the thread
    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 500);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Script error") || body_str.contains("Script panic"));
}

#[tokio::test]
async fn test_handle_nu_shell_column_error() {
    // Test script with different Nu shell column access errors
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            let auth = $req.headers.authorization.bearer
            $"Auth: ($auth)"
        }"#,
    )));

    let req = Request::builder()
        .uri("/auth")
        .body(Empty::<Bytes>::new())
        .unwrap();

    // Should return 500 error instead of thread panic
    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 500);
}

#[tokio::test]
async fn test_handle_script_runtime_error() {
    // Test script with runtime errors (division by zero, etc)
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            let result = (10 / 0)
            $"Result: ($result)"
        }"#,
    )));

    let req = Request::builder()
        .uri("/divide")
        .body(Empty::<Bytes>::new())
        .unwrap();

    // Should gracefully handle runtime errors
    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 500);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Script error"));
}

#[tokio::test]
async fn test_multi_value_set_cookie_headers() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
            .response {
                status: 200
                headers: {
                    "Set-Cookie": ["session=abc123; Path=/; HttpOnly", "token=xyz789; Path=/; Secure"]
                }
            }
            "cookies set"
        }"#,
    )));

    let req = Request::builder()
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify we have two separate Set-Cookie headers
    let set_cookie_headers: Vec<_> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();

    assert_eq!(set_cookie_headers.len(), 2);
    assert!(set_cookie_headers.contains(&"session=abc123; Path=/; HttpOnly"));
    assert!(set_cookie_headers.contains(&"token=xyz789; Path=/; Secure"));
}

#[tokio::test]
async fn test_handle_mj_template() {
    let engine = Arc::new(ArcSwap::from_pointee(test_engine(
        r#"{|req|
        {items: [1, 2, 3], name: "test&foo"} | .mj --inline "
{%- for i in items -%}
  {%- if i == 2 %}{% continue %}{% endif -%}
  {{ i }}
{%- endfor %}
{{ name|urlencode }}
{{ items|tojson }}"
    }"#,
    )));

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, None, req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    // Tests: loop_controls (continue skips 2), urlencode, tojson
    assert!(body.contains("13")); // 1 and 3, skipped 2
    assert!(body.contains("test%26foo")); // urlencode
    assert!(body.contains("[1, 2, 3]") || body.contains("[1,2,3]")); // tojson
}

#[tokio::test]
async fn test_handle_html_record() {
    let engine = test_engine(r#"{|req| {__html: "<div>hello</div>"} }"#);

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(Arc::new(ArcSwap::from_pointee(engine)), None, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body, "<div>hello</div>");
}
