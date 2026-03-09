# Quickstart: 003-flume-channel

**Date**: 2026-03-09

## Prerequisites

- Rust 2021 edition (stable or nightly)
- Working `cargo build --all` on current master

## Key Files to Modify

1. **`Cargo.toml`** — Add `flume = "0.11"`, remove `sync` from tokio features
2. **`src/lib.rs`** — Replace mpsc with flume, refactor BackgroundTask to async fn, add drop counter + warning, add BackgroundTaskFuture type alias
3. **`src/builder.rs`** — Add `backoff` and `channel_capacity` fields/methods, update build return types
4. **`tests/integration.rs`** — Add overflow drop tests, update any BackgroundTask type references

## Build & Test

```bash
cargo build --all                    # Verify compilation
cargo test --all                     # All tests (unit + integration + doc)
cargo clippy --all-targets           # Zero warnings
cargo fmt -- --check                 # Formatting check
```

## Verification Checklist

- [ ] `flume` added to `[dependencies]`
- [ ] `tokio` sync feature removed (time feature kept)
- [ ] `event_channel()` uses `flume::bounded(cap)`
- [ ] `Layer.sender` is `flume::Sender<Option<LokiEvent>>`
- [ ] `Layer.on_event` uses `try_send`, increments drop counter on `TrySendError::Full`
- [ ] Rate-limited warning emitted on first drop
- [ ] `Layer::dropped_count()` returns `u64`
- [ ] `BackgroundTask` is private, has `async fn start(self)`
- [ ] `BackgroundTaskFuture` type alias is public
- [ ] `Builder::backoff()` and `Builder::channel_capacity()` methods work
- [ ] `build_url()` and `build_controller_url()` return `BackgroundTaskFuture`
- [ ] Shutdown flushes pending events before exiting
- [ ] Exponential backoff preserved (500ms base, 600s cap)
- [ ] All 32 existing integration tests pass
- [ ] New overflow/drop tests pass
- [ ] `cargo clippy` zero warnings
- [ ] `cargo fmt -- --check` passes
