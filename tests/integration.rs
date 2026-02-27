use std::collections::HashMap;
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::body::Bytes;
use axum::routing::post;
use axum::Router;
use loki_api::logproto::PushRequest;
use loki_api::prost::Message;
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use url::Url;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// FakeLokiServer
// ---------------------------------------------------------------------------

struct FakeLokiServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<PushRequest>>>,
}

impl FakeLokiServer {
    async fn start() -> Self {
        let requests: Arc<Mutex<Vec<PushRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let shared = requests.clone();

        let app = Router::new().route(
            "/loki/api/v1/push",
            post(move |body: Bytes| {
                let shared = shared.clone();
                async move {
                    let mut decoder = snap::raw::Decoder::new();
                    let decompressed = decoder
                        .decompress_vec(&body)
                        .expect("snappy decompression failed");
                    let push_request =
                        PushRequest::decode(&decompressed[..]).expect("protobuf decode failed");
                    shared.lock().unwrap().push(push_request);
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app).into_future());

        FakeLokiServer { addr, requests }
    }

    fn url(&self) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}/", self.addr.port())).unwrap()
    }

    fn requests(&self) -> Vec<PushRequest> {
        self.requests.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// parse_labels helper
// ---------------------------------------------------------------------------

fn parse_labels(s: &str) -> HashMap<String, String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(s);

    if inner.is_empty() {
        return HashMap::new();
    }

    let mut result = HashMap::new();
    let mut chars = inner.chars().peekable();

    while chars.peek().is_some() {
        // Parse key (until '=')
        let key: String = chars.by_ref().take_while(|&c| c != '=').collect();

        // Expect opening quote
        let quote = chars.next();
        assert_eq!(quote, Some('"'), "expected opening quote for key {key}");

        // Parse value (until unescaped closing quote)
        let mut value = String::new();
        loop {
            match chars.next() {
                Some('\\') => {
                    if let Some(escaped) = chars.next() {
                        value.push(escaped);
                    }
                }
                Some('"') => break,
                Some(c) => value.push(c),
                None => panic!("unexpected end of label string"),
            }
        }

        result.insert(key, value);

        // Skip comma separator
        if chars.peek() == Some(&',') {
            chars.next();
        }
    }

    result
}

// ---------------------------------------------------------------------------
// US1: Verify Log Messages Reach Loki Correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_basic_message() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("test message");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    assert_eq!(requests.len(), 1, "expected exactly one PushRequest");

    let req = &requests[0];
    assert_eq!(req.streams.len(), 1, "expected exactly one stream");

    let stream = &req.streams[0];
    let labels = parse_labels(&stream.labels);
    assert_eq!(labels.get("host").map(String::as_str), Some("test"));
    assert_eq!(labels.get("level").map(String::as_str), Some("info"));
    assert_eq!(labels.len(), 2, "expected exactly 2 labels (host, level)");

    assert_eq!(stream.entries.len(), 1, "expected exactly one entry");

    let entry = &stream.entries[0];
    assert!(
        entry.timestamp.is_some(),
        "entry should have a timestamp set"
    );

    let json: serde_json::Value = serde_json::from_str(&entry.line)?;
    assert_eq!(json["message"], "test message");
    assert_eq!(json["_target"], "integration");
    assert_eq!(
        json["_spans"],
        serde_json::json!([]),
        "no spans expected for a top-level event"
    );
    assert!(json["_file"].is_string(), "_file should be present");
    assert!(json["_line"].is_number(), "_line should be present");
    Ok(())
}

#[tokio::test]
async fn test_all_levels() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::trace!("t");
        tracing::debug!("d");
        tracing::info!("i");
        tracing::warn!("w");
        tracing::error!("e");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let mut found_levels: Vec<String> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .filter_map(|s| {
            let labels = parse_labels(&s.labels);
            labels.get("level").cloned()
        })
        .collect();
    found_levels.sort();
    found_levels.dedup();

    assert_eq!(
        found_levels,
        vec!["debug", "error", "info", "trace", "warn"],
        "expected all 5 levels, got: {:?}",
        found_levels
    );
    Ok(())
}

#[tokio::test]
async fn test_structured_fields() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(task = "setup", result = "ok", "operation complete");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let line = &entries[0].line;
    let json: serde_json::Value = serde_json::from_str(line)?;
    assert_eq!(json["task"], "setup");
    assert_eq!(json["result"], "ok");
    assert_eq!(json["message"], "operation complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// US2: Verify Labels Are Correctly Applied
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_custom_labels() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("service", "my_app")?
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("labeled");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let streams: Vec<_> = requests.iter().flat_map(|r| &r.streams).collect();
    assert!(!streams.is_empty());

    let labels = parse_labels(&streams[0].labels);
    assert_eq!(labels.get("service").map(String::as_str), Some("my_app"));
    assert_eq!(labels.get("host").map(String::as_str), Some("test"));
    assert_eq!(labels.get("level").map(String::as_str), Some("info"));
    Ok(())
}

#[tokio::test]
async fn test_multiple_labels() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "alpha")?
        .label("env", "staging")?
        .label("region", "us_east")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!("multi");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let streams: Vec<_> = requests.iter().flat_map(|r| &r.streams).collect();
    assert!(!streams.is_empty());

    let labels = parse_labels(&streams[0].labels);
    assert_eq!(labels.get("host").map(String::as_str), Some("alpha"));
    assert_eq!(labels.get("env").map(String::as_str), Some("staging"));
    assert_eq!(labels.get("region").map(String::as_str), Some("us_east"));
    assert_eq!(labels.get("level").map(String::as_str), Some("warn"));
    assert_eq!(labels.len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_no_custom_labels() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder().build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("bare");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let streams: Vec<_> = requests.iter().flat_map(|r| &r.streams).collect();
    assert!(!streams.is_empty());

    let labels = parse_labels(&streams[0].labels);
    assert_eq!(labels.len(), 1);
    assert_eq!(labels.get("level").map(String::as_str), Some("info"));
    Ok(())
}

// ---------------------------------------------------------------------------
// US3: Verify Extra Fields Appear in Log Entries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_extra_field() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .extra_field("pid", "1234")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("with extra");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let json: serde_json::Value = serde_json::from_str(&entries[0].line)?;
    assert_eq!(json["pid"], "1234");
    Ok(())
}

#[tokio::test]
async fn test_multiple_extra_fields() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .extra_field("pid", "1234")?
        .extra_field("version", "0.2.6")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("multi extra");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let json: serde_json::Value = serde_json::from_str(&entries[0].line)?;
    assert_eq!(json["pid"], "1234");
    assert_eq!(json["version"], "0.2.6");
    Ok(())
}

#[tokio::test]
async fn test_extra_fields_with_event_fields() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .extra_field("pid", "1234")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(task = "init", "starting");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let json: serde_json::Value = serde_json::from_str(&entries[0].line)?;
    assert_eq!(json["pid"], "1234", "extra field should be present");
    assert_eq!(json["task"], "init", "event field should be present");
    Ok(())
}

// ---------------------------------------------------------------------------
// US4: Verify Protobuf and Snappy Encoding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_protobuf_structure() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("structure check");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    assert!(!requests.is_empty(), "expected at least one PushRequest");

    let req = &requests[0];
    assert!(!req.streams.is_empty(), "PushRequest should have streams");

    let stream = &req.streams[0];
    assert!(
        !stream.entries.is_empty(),
        "StreamAdapter should have entries"
    );

    let entry = &stream.entries[0];
    assert!(
        !entry.line.is_empty(),
        "EntryAdapter line should not be empty"
    );
    Ok(())
}

#[tokio::test]
async fn test_timestamp_precision() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let before = SystemTime::now();

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("timed");
    });

    let after = SystemTime::now();

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let ts = entries[0]
        .timestamp
        .as_ref()
        .expect("timestamp should be present");

    let before_unix = before.duration_since(SystemTime::UNIX_EPOCH)?;
    let after_unix = after.duration_since(SystemTime::UNIX_EPOCH)?;

    let ts_secs = ts.seconds as u64;
    assert!(
        ts_secs >= before_unix.as_secs() && ts_secs <= after_unix.as_secs() + 1,
        "timestamp seconds ({}) should be between {} and {}",
        ts_secs,
        before_unix.as_secs(),
        after_unix.as_secs() + 1
    );

    assert!(ts.nanos >= 0, "nanos should be non-negative");
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge Cases: Span Fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_span_fields() -> TestResult {
    let server = FakeLokiServer::start().await;
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")?
        .build_controller_url(server.url())?;

    let handle = tokio::spawn(task);

    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("my_span", span_field = "span_value");
        let _guard = span.enter();
        tracing::info!(event_field = "event_value", "inside span");
    });

    controller.shutdown().await;
    handle.await?;

    let requests = server.requests();
    let entries: Vec<_> = requests
        .iter()
        .flat_map(|r| &r.streams)
        .flat_map(|s| &s.entries)
        .collect();
    assert!(!entries.is_empty());

    let json: serde_json::Value = serde_json::from_str(&entries[0].line)?;

    assert_eq!(
        json["span_field"], "span_value",
        "span field should be present in entry"
    );
    assert_eq!(
        json["event_field"], "event_value",
        "event field should be present in entry"
    );

    let spans = json["_spans"]
        .as_array()
        .expect("_spans should be an array");
    assert!(
        spans.iter().any(|s| s == "my_span"),
        "_spans should contain 'my_span', got: {:?}",
        spans
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// parse_labels unit test
// ---------------------------------------------------------------------------

#[test]
fn test_parse_labels() {
    let labels = parse_labels(r#"{level="info",host="mine"}"#);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels.get("level").map(String::as_str), Some("info"));
    assert_eq!(labels.get("host").map(String::as_str), Some("mine"));

    let empty = parse_labels("{}");
    assert!(empty.is_empty());

    let single = parse_labels(r#"{level="error"}"#);
    assert_eq!(single.len(), 1);
    assert_eq!(single.get("level").map(String::as_str), Some("error"));
}
