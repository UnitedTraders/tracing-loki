# Quickstart: Plain Text Log Format with Field-to-Label Mapping

**Date**: 2026-02-27
**Feature**: 002-plain-text-format

## Usage Examples

### Plain Text Format

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), tracing_loki::Error> {
    let (layer, task) = tracing_loki::builder()
        .label("host", "mine")?
        .plain_text()  // Switch from JSON to plain text
        .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

    tracing_subscriber::registry().with(layer).init();
    tokio::spawn(task);

    // Entry line in Loki: "hello world"
    tracing::info!("hello world");

    // Entry line: "starting up task=setup"
    tracing::info!(task = "setup", "starting up");

    Ok(())
}
```

### Field-to-Label Mapping

```rust
let (layer, task) = tracing_loki::builder()
    .label("host", "mine")?
    .field_to_label("service", "service")?   // Same name
    .field_to_label("task", "task_name")?     // Rename field→label
    .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

// Stream labels: {host="mine",level="info",service="auth"}
// Entry line (JSON): {"message":"request received","request_id":"abc",...}
// Note: "service" field is removed from entry line (promoted to label)
tracing::info!(service = "auth", request_id = "abc", "request received");
```

### Exclude Unmapped Fields

```rust
let (layer, task) = tracing_loki::builder()
    .label("host", "mine")?
    .plain_text()
    .field_to_label("service", "service")?
    .exclude_unmapped_fields()
    .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

// Stream labels: {host="mine",level="info",service="auth"}
// Entry line: "request received"
// Note: request_id is excluded (unmapped) and service is a label
tracing::info!(service = "auth", request_id = "abc", "request received");
```

### Map Metadata Fields to Labels

```rust
let (layer, task) = tracing_loki::builder()
    .label("host", "mine")?
    .plain_text()
    .field_to_label("_target", "target")?  // Metadata → label
    .exclude_unmapped_fields()
    .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

// Stream labels: {host="mine",level="info",target="my_app::handlers"}
// Entry line: "processing request"
tracing::info!("processing request");
```

## Running Tests

```bash
# Run all tests (including integration tests)
cargo test --all

# Run only integration tests
cargo test --test integration

# Run a specific test
cargo test --test integration test_plain_text_basic

# Run with output visible
cargo test --test integration -- --nocapture
```

## Builder API Summary

| Method                                      | Description                                    | Default     |
|---------------------------------------------|------------------------------------------------|-------------|
| `.plain_text()`                             | Switch entry line format to plain text         | JSON        |
| `.field_to_label("source", "target")?`      | Map tracing field to Loki label                | No mappings |
| `.exclude_unmapped_fields()`                | Drop unmapped fields from entry line           | Include all |
