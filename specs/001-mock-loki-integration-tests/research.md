# Research: Mock Loki Integration Tests

**Date**: 2026-02-27
**Feature**: 001-mock-loki-integration-tests

## R1: Fake HTTP Server Approach

**Decision**: Use `axum` as the in-process fake Loki HTTP server bound to
`127.0.0.1:0` (OS-assigned port).

**Rationale**: `axum` is a lightweight, async-native HTTP framework built
on `hyper` and `tokio`. It shares the same async runtime the tests already
require (`tokio`). It adds minimal dependency surface for `dev-dependencies`
and provides ergonomic request body extraction.

**Alternatives considered**:
- `wiremock` / `mockito`: Higher-level mocking libraries, but they abstract
  away the raw body — we need to manually Snappy-decompress and protobuf-
  decode, so a custom handler is simpler.
- Raw `hyper::Server`: Works but requires more boilerplate for routing and
  body extraction. `axum` wraps `hyper` with zero overhead.
- `warp`: Similar to axum but less ecosystem momentum; axum is the tokio
  team's recommendation.

## R2: Snappy Decompression in Tests

**Decision**: Use the existing `snap` crate (already a production
dependency) with `snap::raw::Decoder` to decompress request bodies in
the fake server.

**Rationale**: The library uses `snap::raw::Encoder` (block format, not
stream format). The test must use the matching `snap::raw::Decoder`.
Re-using the same crate avoids version mismatches.

**Alternatives considered**:
- `snap::read::FrameDecoder` (stream format): Wrong format — Loki expects
  raw/block Snappy, not framed.

## R3: Protobuf Decoding in Tests

**Decision**: Use `prost::Message::decode()` on the `PushRequest` type
from the `loki-api` crate (workspace member, already available).

**Rationale**: The `loki-api` crate provides the exact protobuf types
(`PushRequest`, `StreamAdapter`, `EntryAdapter`) used by the production
code. Decoding in tests with the same types gives a direct round-trip
verification.

## R4: Test Synchronization Strategy

**Decision**: The fake server stores received `PushRequest` messages in
an `Arc<Mutex<Vec<PushRequest>>>`. Tests emit events, then use
`BackgroundTaskController::shutdown()` to flush the pipeline. After
shutdown completes, the captured requests are inspected.

**Rationale**: Graceful shutdown via `BackgroundTaskController` ensures
the `BackgroundTask` drains its channel and sends all pending batches
before the Future completes. This eliminates timing-dependent waits and
makes tests deterministic.

**Alternatives considered**:
- Polling with timeout: Fragile, can flake under load.
- Channel-based notification from fake server: More complex, shutdown
  already guarantees delivery.

## R5: Test Isolation

**Decision**: Each test function creates its own fake server, Layer,
and BackgroundTask. No shared state between tests.

**Rationale**: `tracing_subscriber::set_global_default` can only be
called once per process. Instead, tests use `tracing::subscriber::with_default`
(scoped subscriber) or `Registry::with(layer)` with a local dispatch.
This allows tests to run in parallel without interfering.

**Alternatives considered**:
- Global subscriber with `#[serial]`: Prevents parallelism; fragile.

## R6: Label Parsing in Assertions

**Decision**: Parse the `StreamAdapter.labels` string (Prometheus format
like `{level="info",host="mine"}`) into a `HashMap<String, String>` for
assertion convenience.

**Rationale**: String comparison is brittle (label ordering may vary).
Parsing into a map allows order-independent key-value assertions.

## R7: Dev Dependency Additions

**Decision**: Add `axum` and `tokio` (with `net` feature) as
`dev-dependencies`.

**Rationale**:
- `axum`: Fake HTTP server (minimal, async-native, built on hyper/tokio).
- `tokio` already in dev-deps; add `net` feature for `TcpListener::bind`.
- `snap` and `prost` are already production dependencies, usable in tests.
- `loki-api` is a workspace member, usable in tests.
- No other new dependencies needed.
