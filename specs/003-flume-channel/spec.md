# Feature Specification: Flume Channel with Overflow Drop Policy and Configurable Backoff

**Feature Branch**: `003-flume-channel`
**Created**: 2026-03-09
**Status**: Draft
**Input**: User description: "Change mpsc channel to flume channel with bounds on the channel and drop policy on overflow, add backoff used by background process. Consult similar changes in https://github.com/hrxi/tracing-loki/compare/master...cullenjewellery:tracing-loki:master"

## Clarifications

### Session 2026-03-09

- Q: Should dropped events be truly silent, or should the library provide some observability signal? → A: Both — emit a rate-limited warning log when events are dropped AND expose an atomic counter on the Layer that consumers can poll to check total dropped event count.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Bounded Channel with Overflow Drop (Priority: P1)

As a library consumer running a high-throughput application, I need the internal event channel to drop log events gracefully when the channel is full, so that logging never blocks or slows down my application's critical path. When drops occur, I need visibility through both a rate-limited warning log and a programmatically accessible drop counter.

**Why this priority**: This is the core value of the feature. The current tokio mpsc channel blocks the sender when full, which can cause backpressure in the application's hot path. Switching to a bounded flume channel with a try-send (drop-on-overflow) policy eliminates this risk entirely.

**Independent Test**: Can be fully tested by emitting events faster than the background task consumes them, verifying that the sender never blocks, excess events are dropped, the drop counter increments, and a warning is logged.

**Acceptance Scenarios**:

1. **Given** a layer configured with a channel capacity of N, **When** more than N events are emitted before the background task processes any, **Then** the excess events are dropped without blocking the caller, and the drop counter reflects the number of dropped events.
2. **Given** a layer with default configuration, **When** events are emitted at a normal rate, **Then** all events are delivered to Loki (no drops under normal load) and the drop counter remains at zero.
3. **Given** a layer with a custom channel capacity, **When** the capacity is set to 1024, **Then** the channel accommodates up to 1024 pending events before dropping.
4. **Given** the channel is full and events are being dropped, **When** the first drop occurs (and periodically thereafter), **Then** a rate-limited warning is emitted via tracing to alert operators.

---

### User Story 2 - Configurable Channel Capacity (Priority: P1)

As a library consumer, I need to configure the internal channel capacity via the builder, so that I can tune memory usage versus drop tolerance for my specific workload.

**Why this priority**: Different applications have different memory budgets and logging volumes. A fixed channel size forces a one-size-fits-all tradeoff. Exposing this as a builder option lets consumers optimize for their use case.

**Independent Test**: Can be fully tested by constructing a layer with a custom capacity via the builder and verifying the channel accepts exactly that many events before overflow behavior kicks in.

**Acceptance Scenarios**:

1. **Given** a builder, **When** `.channel_capacity(2048)` is called, **Then** the internal channel has a capacity of 2048.
2. **Given** a builder with no explicit channel capacity, **When** the layer is built, **Then** the default capacity of 512 is used (backward compatible).

---

### User Story 3 - Configurable Background Task Backoff (Priority: P2)

As a library consumer, I need to configure the base backoff interval used by the background task's send loop, so that I can control how frequently log batches are pushed to Loki.

**Why this priority**: The backoff interval determines how often the background task attempts to push batches. A shorter interval reduces log delivery latency; a longer interval reduces HTTP request volume. Making this configurable lets consumers tune the tradeoff.

**Independent Test**: Can be fully tested by setting a custom backoff duration via the builder and verifying the background task respects it in its send cycle.

**Acceptance Scenarios**:

1. **Given** a builder, **When** `.backoff(Duration::from_millis(100))` is called, **Then** the background task uses 100ms as the base interval between send cycles.
2. **Given** a builder with no explicit backoff, **When** the layer is built, **Then** the default backoff of 500ms is used (backward compatible).

---

### User Story 4 - Simplified Background Task as Async Function (Priority: P2)

As a library consumer, I need the background task to be a straightforward async function rather than a manual Future implementation, so that the library is easier to understand, maintain, and debug.

**Why this priority**: The current hand-rolled `Future` implementation with manual polling is complex. Converting to an `async fn` loop simplifies the code and makes the send/receive/backoff cycle more readable, while producing the same observable behavior.

**Independent Test**: Can be fully tested by running existing integration tests -- all current tests should pass unchanged since this is an internal refactor with no API change to the task's external behavior.

**Acceptance Scenarios**:

1. **Given** the background task is spawned, **When** events are emitted and shutdown is called, **Then** all pending events are flushed and the task completes (same as current behavior).
2. **Given** the background task encounters an HTTP error, **When** retrying with exponential backoff, **Then** the retry behavior remains consistent with the current implementation.

---

### Edge Cases

- What happens when the channel capacity is set to 0? The builder should reject this with an error.
- What happens when events are emitted after `shutdown()` is called? They should be dropped (same as current behavior).
- What happens when the background task encounters repeated send failures? Exponential backoff applies with the same cap at 600 seconds, and outstanding messages are dropped after threshold.
- What happens when the flume sender is used from multiple threads simultaneously? Flume senders are thread-safe and support concurrent sends without cloning.
- What happens when drops are occurring at very high rates? The warning log is rate-limited to avoid flooding the logging output; the atomic counter always reflects the true total.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The library MUST replace the tokio mpsc channel with a flume bounded channel for internal event transport.
- **FR-002**: The library MUST drop events without blocking when the channel is full, emit a rate-limited warning log, and increment an atomic drop counter on the Layer.
- **FR-003**: The builder MUST provide a `.channel_capacity(n)` method to configure the channel's bounded capacity.
- **FR-004**: The default channel capacity MUST be 512 (preserving backward compatibility).
- **FR-005**: The builder MUST provide a `.backoff(duration)` method to configure the base backoff interval of the background task.
- **FR-006**: The default backoff MUST be 500ms (preserving backward compatibility).
- **FR-007**: The background task MUST use an async loop pattern instead of manual Future polling.
- **FR-008**: The background task MUST retain exponential backoff on send failure, capped at 600 seconds.
- **FR-009**: The background task MUST drain all pending events from the channel on each iteration before sending.
- **FR-010**: The background task MUST exit cleanly when it receives the shutdown signal (None value).
- **FR-011**: The builder MUST reject a channel capacity of 0 with an error.
- **FR-012**: All existing public API signatures and behavior MUST remain backward compatible, except that `BackgroundTask` is no longer exposed as a named Future type but as a type-aliased boxed future.
- **FR-013**: The Layer MUST expose a method to read the current dropped event count for programmatic monitoring.

### Key Entities

- **Event Channel**: The bounded communication pipe between the synchronous Layer and the async BackgroundTask, with configurable capacity and drop-on-overflow semantics.
- **Backoff Duration**: The base interval between background task send cycles, configurable via the builder and used as the foundation for exponential backoff on failures.
- **Drop Counter**: An atomic counter on the Layer tracking the total number of events dropped due to channel overflow, readable by consumers for monitoring purposes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Under sustained load exceeding channel capacity, log emission never blocks the caller for more than the time to attempt a single channel send (sub-microsecond).
- **SC-002**: All existing integration tests pass without modification (backward compatibility).
- **SC-003**: New integration tests verify overflow drop behavior by emitting events faster than the background task can consume, and confirm the drop counter reflects the correct number of dropped events.
- **SC-004**: Memory usage under sustained high-throughput logging remains proportional to the configured channel capacity, not to the total number of emitted events.
- **SC-005**: Custom backoff and channel capacity values set via the builder are reflected in the runtime behavior of the background task.
- **SC-006**: A rate-limited warning is logged when events are dropped, observable by subscribers attached to the tracing system.

## Assumptions

- The `flume` crate is a well-maintained, production-quality alternative to tokio mpsc that provides `try_send` for non-blocking overflow semantics.
- The current default channel capacity of 512 and backoff of 500ms are reasonable defaults that should be preserved for backward compatibility.
- The `BackgroundTask` type currently implements `Future` directly. After this change, the builder will return a `Pin<Box<dyn Future<Output = ()> + Send>>` (type-aliased as `BackgroundTaskFuture`) instead. This is a minor API change but maintains ergonomic equivalence for the `tokio::spawn(task)` pattern.
- The reference implementation at `cullenjewellery:tracing-loki:master` serves as directional guidance, not an exact blueprint. Our implementation will adapt the approach to fit the current codebase (which includes the plain text format and field-to-label features from branch 002).
- The rate-limited warning log uses a reasonable default rate limit (e.g., at most once per backoff interval) to avoid flooding the logging system during sustained overload.
