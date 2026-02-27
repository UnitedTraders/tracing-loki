# Quickstart: Mock Loki Integration Tests

**Date**: 2026-02-27
**Feature**: 001-mock-loki-integration-tests

## Running the Tests

```bash
# Run all tests (including new integration tests)
cargo test --all

# Run only integration tests
cargo test --test integration

# Run a specific test
cargo test --test integration test_basic_message

# Run with output visible
cargo test --test integration -- --nocapture
```

## Test Structure Overview

Tests live in `tests/integration.rs` (or `tests/integration/` module).

Each test follows this pattern:

1. **Start fake server** — bind to `127.0.0.1:0`, capture requests
2. **Build Layer** — use `tracing_loki::builder()` with fake server URL
3. **Set up subscriber** — register Layer with a scoped subscriber
4. **Emit events** — use `tracing::info!()`, `tracing::warn!()`, etc.
5. **Shutdown** — call `BackgroundTaskController::shutdown()` to flush
6. **Assert** — inspect captured `PushRequest` messages

## Example: Basic Message Test

```rust
#[tokio::test]
async fn test_basic_message() {
    // 1. Start fake Loki server
    let server = FakeLokiServer::start().await;

    // 2. Build Layer pointing to fake server
    let (layer, controller, task) = tracing_loki::builder()
        .label("host", "test")
        .unwrap()
        .build_controller_url(server.url())
        .unwrap();

    // 3. Spawn background task
    let handle = tokio::spawn(task);

    // 4. Emit event with scoped subscriber
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("hello world");
    });

    // 5. Shutdown and wait
    controller.shutdown().await;
    handle.await.unwrap();

    // 6. Assert
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let entry = &requests[0].streams[0].entries[0];
    assert!(entry.line.contains("hello world"));
}
```

## Adding New Tests

To add a new test case:

1. Write an async test function with `#[tokio::test]`
2. Use the shared `FakeLokiServer` helper
3. Configure the Builder with the labels/fields relevant to the test
4. Use `tracing::subscriber::with_default` for test isolation
5. Always shutdown via controller before asserting
