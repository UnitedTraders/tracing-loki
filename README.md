tracing-loki
============

A [tracing](https://github.com/tokio-rs/tracing) layer for [Grafana
Loki](https://grafana.com/oss/loki/).

[![Build status](https://github.com/hrxi/tracing-loki/actions/workflows/build.yaml/badge.svg)](https://github.com/hrxi/tracing-loki/actions/workflows/build.yaml)

Documentation
-------------

https://docs.rs/tracing-loki

Usage
-----

Add this to your `Cargo.toml`:
```toml
[dependencies]
tracing-loki = "0.2"
```

Example
-------

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use std::process;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), tracing_loki::Error> {
    let (layer, task) = tracing_loki::builder()
        .label("host", "mine")?
        .extra_field("pid", format!("{}", process::id()))?
        .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

    // We need to register our layer with `tracing`.
    tracing_subscriber::registry()
        .with(layer)
        // One could add more layers here, for example logging to stdout:
        // .with(tracing_subscriber::fmt::Layer::new())
        .init();

    // The background task needs to be spawned so the logs actually get
    // delivered.
    tokio::spawn(task);

    tracing::info!(
        task = "tracing_setup",
        result = "success",
        "tracing successfully set up",
    );

    Ok(())
}
```

Plain text mode
---------------

By default, log entries are serialized as JSON. You can switch to plain text
mode and optionally promote event fields to Loki stream labels:

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), tracing_loki::Error> {
    let (layer, task) = tracing_loki::builder()
        .label("host", "mine")?
        .plain_text()
        .field_to_label("service", "service")?
        .exclude_unmapped_fields()
        .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

    tracing_subscriber::registry()
        .with(layer)
        .init();

    tokio::spawn(task);

    tracing::info!(
        service = "auth",
        request_id = "abc-123",
        "user logged in",
    );
    // Entry line: "user logged in"
    // Stream labels: {host="mine", level="info", service="auth"}
    // request_id is excluded because exclude_unmapped_fields() is set.

    Ok(())
}
```

OpenTelemetry integration
-------------------------

Enable the `opentelemetry` feature to automatically include `trace_id`, `span_id`,
and `span_name` in log entries when used alongside
[`tracing-opentelemetry`](https://crates.io/crates/tracing-opentelemetry):

```toml
[dependencies]
tracing-loki = { version = "0.2", features = ["opentelemetry"] }
tracing-opentelemetry = "0.32"
opentelemetry = "0.31"
opentelemetry_sdk = "0.31"
```

```rust,no_run
use opentelemetry::trace::TracerProvider as _;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), tracing_loki::Error> {
    let (layer, task) = tracing_loki::builder()
        .label("host", "mine")?
        .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
    let tracer = provider.tracer("my-service");

    tracing_subscriber::registry()
        .with(layer)
        .with(OpenTelemetryLayer::new(tracer))
        .init();

    tokio::spawn(task);

    let span = tracing::info_span!("handle_request");
    let _guard = span.enter();
    tracing::info!("processing request");
    // JSON entry includes trace_id, span_id, and span_name automatically.

    Ok(())
}
```

OTel fields can be promoted to Loki stream labels with `field_to_label`,
enabling LogQL queries like `{trace_id="abc123"}`:

```rust,no_run
# use tracing_loki::builder;
# use url::Url;
let (layer, task) = builder()
    .label("host", "mine")?
    .field_to_label("trace_id", "trace_id")?
    .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;
# Ok::<(), tracing_loki::Error>(())
```

When the `opentelemetry` feature is enabled but no `OpenTelemetryLayer` is
registered, `span_id` falls back to the tracing span ID (16-char hex) and
`trace_id` is omitted.
