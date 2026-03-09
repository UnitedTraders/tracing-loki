# Public API Contract: 003-flume-channel

**Date**: 2026-03-09

## New Public Types

### `BackgroundTaskFuture`

```rust
/// Wrapper around the future running the background log shipping task.
pub type BackgroundTaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
```

Replaces the previously public `BackgroundTask` struct in all return positions.

## New Builder Methods

### `Builder::backoff`

```rust
/// Set the base backoff interval for the background task send loop.
///
/// The background task sleeps for this duration between send cycles.
/// On send failure, exponential backoff is applied starting from this base.
///
/// Default: 500ms.
pub fn backoff(self, backoff: Duration) -> Builder
```

### `Builder::channel_capacity`

```rust
/// Set the capacity of the internal event channel.
///
/// When the channel is full, new events are dropped without blocking.
/// Higher values use more memory but reduce drop probability under load.
///
/// Default: 512. Must be > 0.
///
/// Returns `Err` if `capacity` is 0.
pub fn channel_capacity(self, capacity: usize) -> Result<Builder, Error>
```

## New Layer Methods

### `Layer::dropped_count`

```rust
/// Returns the total number of events dropped due to channel overflow.
///
/// This counter is monotonically increasing and uses relaxed atomic ordering.
/// Useful for monitoring and alerting on log delivery capacity.
pub fn dropped_count(&self) -> u64
```

## Modified Return Types

### `builder().build_url()`

```rust
// Before:
pub fn build_url(self, loki_url: Url) -> Result<(Layer, BackgroundTask), Error>

// After:
pub fn build_url(self, loki_url: Url) -> Result<(Layer, BackgroundTaskFuture), Error>
```

### `builder().build_controller_url()`

```rust
// Before:
pub fn build_controller_url(self, loki_url: Url)
    -> Result<(Layer, BackgroundTaskController, BackgroundTask), Error>

// After:
pub fn build_controller_url(self, loki_url: Url)
    -> Result<(Layer, BackgroundTaskController, BackgroundTaskFuture), Error>
```

### `layer()` convenience function

```rust
// Before:
pub fn layer(...) -> Result<(Layer, BackgroundTask), Error>

// After:
pub fn layer(...) -> Result<(Layer, BackgroundTaskFuture), Error>
```

## Modified Shutdown

### `BackgroundTaskController::shutdown`

```rust
// Before (tokio mpsc):
pub async fn shutdown(&self) {
    let _ = self.sender.send(None).await;
}

// After (flume):
pub async fn shutdown(&self) {
    let _ = self.sender.send_async(None).await;
}
```

## New Error Variant

```rust
// Added to ErrorInner enum:
ZeroChannelCapacity
// Display: "channel capacity must be greater than 0"
```

## Removed Public Types

- `BackgroundTask` struct — no longer publicly exposed. Replaced by `BackgroundTaskFuture` type alias.
