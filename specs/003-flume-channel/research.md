# Research: 003-flume-channel

**Date**: 2026-03-09

## R1: flume vs tokio::sync::mpsc for non-blocking send

**Decision**: Use `flume::bounded()` with `Sender::try_send()` for the Layer → BackgroundTask channel.

**Rationale**:
- The current code already uses `tokio::sync::mpsc::Sender::try_send()` (non-blocking, drops on full) — see `src/lib.rs:523`.
- flume's `try_send()` has identical semantics: returns `Err(TrySendError::Full(msg))` when full.
- flume `Sender` is `Send + Sync + Clone` — same as tokio's mpsc sender.
- flume does not require a tokio runtime for the send path, making the Layer more runtime-agnostic.
- flume provides `Receiver::try_recv()` and `Receiver::recv_async()` for the background task.

**Alternatives considered**:
- Keep tokio mpsc: rejected because flume removes the tokio dependency from the synchronous Layer path and provides `drain()`/`try_iter()` for batch draining.
- crossbeam-channel: rejected because it lacks async receive support needed by the background task.

## R2: Background task receive strategy

**Decision**: Use `recv_async().await` in the main loop with `try_recv()` drain before each send cycle.

**Rationale**:
- The background task loop needs to: (1) wait for events, (2) drain all available events, (3) send batch to Loki, (4) sleep for backoff interval.
- `recv_async().await` is the async equivalent of `poll_recv()` — yields to the runtime when no events are available.
- After waking, `try_recv()` in a loop drains all buffered events before sending (FR-009).
- On shutdown signal (`None`), the task flushes pending queues and exits (FR-010, constitution III).

**Alternatives considered**:
- `drain()` iterator: could miss events arriving during iteration; `try_recv` loop is safer.
- Polling loop with `tokio::time::sleep`: wastes CPU when idle; `recv_async` is event-driven.

## R3: Drop counter implementation

**Decision**: Use `Arc<AtomicU64>` shared between the Layer and returned to the caller via a public accessor.

**Rationale**:
- `AtomicU64` with `Ordering::Relaxed` is the lowest-overhead atomic operation — no memory fences needed since the counter is monotonically increasing and only used for monitoring.
- `Arc` is needed because the Layer is cloned when used with tracing-subscriber's `with()`.
- Expose via `Layer::dropped_count(&self) -> u64` method.
- Increment in `on_event` only when `try_send` returns `TrySendError::Full`.

**Alternatives considered**:
- `AtomicUsize`: `u64` is more appropriate for a counter that may accumulate over long-running processes.
- Per-thread counters with thread-local storage: over-engineered for this use case.

## R4: Rate-limited warning log for drops

**Decision**: Log a warning on the first drop, then suppress for a configurable window (default: same as backoff duration, 500ms). Use a timestamp-based approach with `Instant`.

**Rationale**:
- Rate limiting avoids flooding the tracing output when drops are sustained.
- Using `Instant::now()` comparison is cheap (single syscall, no allocation).
- Store `last_drop_warning: Option<Instant>` on the Layer; emit warning only when elapsed > rate limit interval.
- The warning must be emitted WITHOUT going through the loki layer (to avoid recursion). Use a direct `tracing::warn!` which other subscribers will see, but guard against self-recursion by checking if we're already inside `on_event`.

**Alternatives considered**:
- Counter-based rate limiting (every Nth drop): less predictable behavior under varying rates.
- No rate limiting: would flood output during sustained overload.

## R5: tokio dependency after migration

**Decision**: Keep `tokio` as a dependency with `sync` feature removed; add `time` feature if not already present for `tokio::time::sleep` in the background task.

**Rationale**:
- The background task still needs `tokio::time::sleep()` for backoff delays.
- `tokio::sync::mpsc` is replaced by flume, so `sync` feature is no longer needed.
- tokio is already required by reqwest and the async runtime, so this adds no new dependency.

**Alternatives considered**:
- Remove tokio entirely and use `futures_timer`: would add a new dependency for minimal benefit since tokio is already in the dep tree via reqwest.

## R6: BackgroundTask type change (Future → async fn)

**Decision**: Convert `BackgroundTask` from a `pub struct` implementing `Future` to a private struct with an `async fn start(self)` method. Expose the return type as `pub type BackgroundTaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>`.

**Rationale**:
- The manual `Future::poll` implementation is ~100 lines of complex state management with manual pinning.
- An `async fn` loop is ~40 lines of straightforward sequential logic.
- The `BackgroundTaskFuture` type alias preserves API ergonomics: `tokio::spawn(task)` still works.
- This is a breaking change to the public type (BackgroundTask struct no longer public), justified by FR-012.

**Alternatives considered**:
- Keep Future impl, just swap channel: misses the simplification opportunity.
- Return `impl Future`: doesn't work across crate boundaries for public API in all cases; boxed future is safer.
