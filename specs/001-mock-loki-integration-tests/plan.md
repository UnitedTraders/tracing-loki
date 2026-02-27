# Implementation Plan: Mock Loki Integration Tests

**Branch**: `001-mock-loki-integration-tests` | **Date**: 2026-02-27 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-mock-loki-integration-tests/spec.md`

## Summary

Add integration tests that exercise the full tracing-loki pipeline (Layer ->
channel -> BackgroundTask -> HTTP POST) against a fake Loki HTTP server
running in-process. The fake server accepts `/loki/api/v1/push`, Snappy-
decompresses and protobuf-decodes the body, and captures `PushRequest`
messages for assertion. Tests verify message correctness, label propagation,
extra field inclusion, span field serialization, and wire format compliance.

## Technical Context

**Language/Version**: Rust 2021 edition, stable + nightly (CI matrix)
**Primary Dependencies**: tracing 0.1, tracing-subscriber 0.3, reqwest, prost, snap, loki-api (workspace)
**Storage**: N/A
**Testing**: `cargo test --all` with `tokio::test` async runtime
**Target Platform**: Cross-platform (macOS, Linux)
**Project Type**: Library (test suite addition)
**Performance Goals**: N/A (test code)
**Constraints**: No external services; tests must pass with `cargo test --all`
**Scale/Scope**: ~8-12 test functions, 1 test helper module, ~400-600 lines

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Library-First API Design | PASS | No public API changes |
| II. Correctness & Safety | PASS | Test code; `unwrap()`/`expect()` permitted in tests |
| III. Async Discipline | PASS | Tests use tokio runtime; BackgroundTask spawned normally |
| IV. Protocol Fidelity | PASS | Tests verify protocol correctness (this is the purpose) |
| V. Testing Discipline | PASS | Fake Loki server per constitution; no external services |
| VI. Performance & Resource Bounds | PASS | New dev-dependency (axum) justified for fake server |
| VII. Backward Compatibility | PASS | No public API changes; dev-deps only |
| Rust Standards | PASS | Must pass fmt + clippy |

## Project Structure

### Documentation (this feature)

```text
specs/001-mock-loki-integration-tests/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
tests/
└── integration.rs       # All integration tests + FakeLokiServer helper

Cargo.toml               # Updated dev-dependencies (axum, tokio features)
```

**Structure Decision**: Single integration test file at `tests/integration.rs`.
The fake server helper struct and label parsing utility are defined in the
same file (private to tests). This keeps the test infrastructure co-located
and avoids unnecessary module splitting for ~500 lines of test code.

## Design Details

### Fake Loki Server

- Struct `FakeLokiServer` with:
  - `start() -> Self`: Binds `axum` router to `127.0.0.1:0`, spawns server
    task, returns self with bound address
  - `url() -> url::Url`: Returns `http://127.0.0.1:{port}/`
  - `requests() -> Vec<PushRequest>`: Clones captured requests for assertion
- Handler for `POST /loki/api/v1/push`:
  - Reads raw body bytes
  - `snap::raw::Decoder::decompress()` to get protobuf bytes
  - `PushRequest::decode()` via `prost::Message`
  - Pushes to `Arc<Mutex<Vec<PushRequest>>>`
  - Returns HTTP 200

### Test Pattern

Each test:
1. `let server = FakeLokiServer::start().await;`
2. Build `(Layer, BackgroundTaskController, BackgroundTask)` via
   `tracing_loki::builder()...build_controller_url(server.url())`
3. `tokio::spawn(task)` for BackgroundTask
4. Use `tracing::subscriber::with_default(registry.with(layer), || { ... })`
   for scoped subscriber (test isolation, no global state)
5. `controller.shutdown().await` + `handle.await` to flush pipeline
6. `server.requests()` to get captured data for assertions

### Label Parsing Helper

- `fn parse_labels(s: &str) -> HashMap<String, String>`: Parses
  `{key1="value1",key2="value2"}` format into a map for order-independent
  assertions.

### Test Cases

**US1 — Message Correctness** (P1):
- `test_basic_message`: Single info event, assert line contains message
- `test_all_levels`: Events at all 5 levels, assert each arrives with
  correct level label
- `test_structured_fields`: Event with key-value fields, assert fields
  appear in entry line JSON

**US2 — Label Correctness** (P2):
- `test_custom_labels`: Builder with custom labels, assert StreamAdapter
  labels include them plus "level"
- `test_multiple_labels`: Multiple custom labels, assert all present
- `test_no_custom_labels`: No custom labels, assert only "level" present

**US3 — Extra Fields** (P3):
- `test_extra_field`: Single extra field, assert present in entry line JSON
- `test_multiple_extra_fields`: Multiple extra fields, assert all present
- `test_extra_fields_with_event_fields`: Both extra and event fields,
  assert coexistence without overwriting

**US4 — Wire Format** (P4):
- `test_protobuf_snappy_decode`: Verify fake server successfully decodes
  (this is implicitly tested by all other tests, but one explicit test
  confirms the PushRequest structure has streams and entries)
- `test_timestamp_precision`: Verify EntryAdapter timestamp is nanosecond
  precision and within reasonable tolerance

**Edge Cases**:
- `test_span_fields`: Event inside a span, assert span fields in entry line

## Complexity Tracking

No constitution violations. No complexity justification needed.
