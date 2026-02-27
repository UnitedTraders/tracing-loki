# Tasks: Mock Loki Integration Tests

**Input**: Design documents from `/specs/001-mock-loki-integration-tests/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, quickstart.md

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup

**Purpose**: Add dev-dependencies and create the test file skeleton.

- [x] T001 Add `axum` and update `tokio` features in dev-dependencies in `Cargo.toml`. Add `axum` (latest compatible version) and `prost` to `[dev-dependencies]`. Update existing tokio dev-dependency to include `net` feature: `features = ["macros", "rt-multi-thread", "net"]`. Add `loki-api` as dev-dependency: `loki-api = { path = "loki-api" }`. Add `snap` to dev-dependencies (same version as production dep `1.0.5`). Run `cargo check --tests` to verify compilation.
- [x] T002 Create test file `tests/integration.rs` with module-level imports and empty test placeholder. Imports needed: `axum`, `loki_api::logproto::PushRequest`, `prost::Message`, `snap`, `std::collections::HashMap`, `std::net::SocketAddr`, `std::sync::{Arc, Mutex}`, `tokio::net::TcpListener`, `tracing`, `tracing_subscriber::layer::SubscriberExt`, `url::Url`. Add a single `#[tokio::test] async fn smoke() {}` placeholder to verify compilation. Run `cargo test --test integration` to confirm the test file compiles and the smoke test passes.

**Checkpoint**: `cargo test --test integration` compiles and passes with empty smoke test.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the `FakeLokiServer` helper and `parse_labels` utility that ALL user story tests depend on.

**CRITICAL**: No user story work can begin until this phase is complete.

- [x] T003 Implement `FakeLokiServer` struct in `tests/integration.rs`. The struct holds `addr: SocketAddr` and `requests: Arc<Mutex<Vec<PushRequest>>>`. Implement `async fn start() -> Self`: create `Arc<Mutex<Vec<PushRequest>>>`, build an axum `Router` with a `POST /loki/api/v1/push` handler, bind a `TcpListener` to `127.0.0.1:0`, capture the bound address, spawn the server via `tokio::spawn(axum::serve(...).into_future())`. The handler reads raw body bytes (`axum::body::Bytes`), Snappy-decompresses using `snap::raw::Decoder`, protobuf-decodes via `PushRequest::decode()`, pushes to the shared vec, returns HTTP 200. Implement `fn url(&self) -> Url` returning `http://127.0.0.1:{port}/`. Implement `fn requests(&self) -> Vec<PushRequest>` cloning the captured vec.
- [x] T004 Implement `fn parse_labels(s: &str) -> HashMap<String, String>` helper in `tests/integration.rs`. Parses Prometheus-format label strings like `{key1="value1",key2="value2"}` into a `HashMap`. Must handle: stripping outer `{` and `}`, splitting by `,`, splitting each pair by `=`, stripping double-quotes from values, handling escaped characters in quoted values. Add a simple inline test verifying `parse_labels(r#"{level="info",host="mine"}"#)` returns the expected map.
- [x] T005 Remove the `smoke` placeholder test from T002 and verify `cargo test --test integration` still compiles cleanly with no test functions yet (or keep one minimal test that exercises `FakeLokiServer::start()` and asserts `server.requests().is_empty()`).

**Checkpoint**: Foundation ready. `FakeLokiServer` and `parse_labels` are available. `cargo test --test integration` passes.

---

## Phase 3: User Story 1 — Verify Log Messages Reach Loki Correctly (Priority: P1)

**Goal**: Verify that emitted tracing events arrive at the fake Loki server with correct message text, correct level labels, and correct structured fields.

**Independent Test**: Emit events, shut down, assert received PushRequest entries contain expected content.

### Implementation for User Story 1

- [x] T006 [US1] Implement `test_basic_message` in `tests/integration.rs`. Start FakeLokiServer. Build Layer+Controller+Task via `tracing_loki::builder().label("host", "test").unwrap().build_controller_url(server.url()).unwrap()`. Spawn task. Use `tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || { tracing::info!("test message"); })`. Shutdown controller, await task handle. Assert `server.requests()` has exactly 1 request. Assert the first stream's first entry's `line` field contains `"test message"`. Covers FR-003.
- [x] T007 [US1] Implement `test_all_levels` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `label("host", "test")`. Use scoped subscriber to emit one event at each level: `tracing::trace!("t")`, `tracing::debug!("d")`, `tracing::info!("i")`, `tracing::warn!("w")`, `tracing::error!("e")`. Shutdown and collect requests. Flatten all streams across all requests. Use `parse_labels()` on each stream's labels string. Assert that the set of level label values across all streams equals `{"trace", "debug", "info", "warn", "error"}`. Covers FR-004, SC-002.
- [x] T008 [US1] Implement `test_structured_fields` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `label("host", "test")`. Emit `tracing::info!(task = "setup", result = "ok", "operation complete")`. Shutdown and collect. Parse the entry line as JSON (`serde_json::Value`). Assert JSON contains `"task": "setup"` and `"result": "ok"` and the message text `"operation complete"`. Covers FR-007.

**Checkpoint**: User Story 1 complete. Basic message delivery, all 5 levels, and structured fields are verified. `cargo test --test integration` passes.

---

## Phase 4: User Story 2 — Verify Labels Are Correctly Applied (Priority: P2)

**Goal**: Verify that custom labels configured via the Builder appear in the received `StreamAdapter` labels alongside the automatic "level" label.

**Independent Test**: Configure Builder with labels, emit an event, assert received stream labels match expectations.

### Implementation for User Story 2

- [x] T009 [US2] Implement `test_custom_labels` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `builder.label("service", "my_app").unwrap().label("host", "test").unwrap()`. Emit a single `tracing::info!("labeled")`. Shutdown and collect. Parse labels from the first stream. Assert labels contain `service="my_app"`, `host="test"`, and `level="info"`. Covers FR-005.
- [x] T010 [P] [US2] Implement `test_multiple_labels` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with 3 custom labels: `host`, `env`, `region`. Emit `tracing::warn!("multi")`. Shutdown and collect. Parse labels. Assert all 3 custom labels plus `level="warn"` are present (4 total). Covers FR-005, SC-003.
- [x] T011 [P] [US2] Implement `test_no_custom_labels` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `tracing_loki::builder().build_controller_url(server.url()).unwrap()` (no custom labels). Emit `tracing::info!("bare")`. Shutdown and collect. Parse labels. Assert the only label is `level="info"`. Covers FR-005.

**Checkpoint**: User Story 2 complete. Label propagation verified for custom, multiple, and no-labels cases.

---

## Phase 5: User Story 3 — Verify Extra Fields Appear in Log Entries (Priority: P3)

**Goal**: Verify that extra fields configured via the Builder appear in every log entry's serialized JSON body, alongside event-specific fields.

**Independent Test**: Configure extra fields, emit an event, assert entry line JSON contains the extra field key-value pairs.

### Implementation for User Story 3

- [x] T012 [US3] Implement `test_extra_field` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `builder.label("host", "test").unwrap().extra_field("pid", "1234").unwrap()`. Emit `tracing::info!("with extra")`. Shutdown and collect. Parse entry line as JSON. Assert JSON contains `"pid": "1234"`. Covers FR-006.
- [x] T013 [P] [US3] Implement `test_multiple_extra_fields` in `tests/integration.rs`. Build Layer with `extra_field("pid", "1234")` and `extra_field("version", "0.2.6")`. Emit event. Assert both fields present in entry line JSON. Covers FR-006, SC-003.
- [x] T014 [P] [US3] Implement `test_extra_fields_with_event_fields` in `tests/integration.rs`. Build Layer with `extra_field("pid", "1234")`. Emit `tracing::info!(task = "init", "starting")`. Parse entry line JSON. Assert both `"pid": "1234"` (extra field) and `"task": "init"` (event field) are present. Assert neither overwrites the other. Covers FR-006, FR-007, SC-003.

**Checkpoint**: User Story 3 complete. Extra fields verified for single, multiple, and coexistence with event fields.

---

## Phase 6: User Story 4 — Verify Protobuf and Snappy Encoding (Priority: P4)

**Goal**: Verify wire format correctness — the fake server can decode the Snappy-compressed protobuf payload and the resulting PushRequest is well-formed with valid timestamps.

**Independent Test**: Emit an event, verify the decoded PushRequest has at least one stream with at least one entry, and that the timestamp is nanosecond-precision.

### Implementation for User Story 4

- [x] T015 [US4] Implement `test_protobuf_structure` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `label("host", "test")`. Emit `tracing::info!("structure check")`. Shutdown and collect. Assert: requests is non-empty, first request has non-empty `streams`, first stream has non-empty `entries`, first entry has a non-empty `line`. This explicitly confirms the Snappy+protobuf round-trip succeeded (FR-008, SC-004).
- [x] T016 [P] [US4] Implement `test_timestamp_precision` in `tests/integration.rs`. Record `std::time::SystemTime::now()` before emitting, emit `tracing::info!("timed")`, record time after. Shutdown and collect. Extract the `timestamp` from the first entry's `EntryAdapter`. Assert timestamp is `Some`. Convert `prost_types::Timestamp` to a comparable form. Assert the timestamp falls between the before and after times (within a reasonable tolerance, e.g., 5 seconds). Assert `nanos` field is populated (not always zero). Covers FR-009, SC-004.

**Checkpoint**: User Story 4 complete. Wire format and timestamp precision verified.

---

## Phase 7: Edge Cases & Span Fields

**Purpose**: Verify span field inclusion — events emitted inside a span contain the span's fields in the serialized entry.

- [x] T017 Implement `test_span_fields` in `tests/integration.rs`. Start FakeLokiServer. Build Layer with `label("host", "test")`. Use scoped subscriber. Create a span with `tracing::info_span!("my_span", span_field = "span_value")`. Enter the span, emit `tracing::info!(event_field = "event_value", "inside span")`. Shutdown and collect. Parse entry line JSON. Assert `"span_field": "span_value"` is present (from span context). Assert `"event_field": "event_value"` is present (from event). Assert `"_spans"` array contains `"my_span"`. Covers FR-011, SC-005.

**Checkpoint**: All user stories and edge cases complete. Full test suite passes.

---

## Phase 8: Polish & Validation

**Purpose**: Final validation and cleanup.

- [x] T018 Run `cargo fmt -- --check` on the entire workspace. Fix any formatting issues in `tests/integration.rs`.
- [x] T019 [P] Run `cargo clippy --all-targets --all-features` and fix any warnings in `tests/integration.rs`.
- [x] T020 [P] Run `cargo test --all` and verify all tests pass (both existing unit tests and new integration tests). Verify no external services are required.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - US1 (Phase 3): No dependency on other stories
  - US2 (Phase 4): No dependency on other stories
  - US3 (Phase 5): No dependency on other stories
  - US4 (Phase 6): No dependency on other stories
  - Edge Cases (Phase 7): No dependency on other stories
- **Polish (Phase 8)**: Depends on all phases being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Phase 2 — No dependencies on other stories
- **User Story 2 (P2)**: Can start after Phase 2 — No dependencies on other stories
- **User Story 3 (P3)**: Can start after Phase 2 — No dependencies on other stories
- **User Story 4 (P4)**: Can start after Phase 2 — No dependencies on other stories
- **Edge Cases**: Can start after Phase 2 — No dependencies on other stories

### Within Each User Story

- All tests within a story are in the same file (`tests/integration.rs`)
- Tasks marked [P] within a story can be written in parallel (independent test functions)
- Non-[P] tasks within a story should be written first (they establish the pattern)

### Parallel Opportunities

- T001 and T002 are sequential (T002 depends on T001 for compilation)
- T003 and T004 can run in parallel (different functions, no dependencies)
- After Phase 2, ALL user story phases (3-7) can proceed in parallel
- Within each story, tasks marked [P] can be written in parallel

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T002)
2. Complete Phase 2: Foundational (T003-T005)
3. Complete Phase 3: User Story 1 (T006-T008)
4. **STOP and VALIDATE**: `cargo test --test integration` passes with 3 message correctness tests
5. This alone provides the core integration test infrastructure and verifies the most critical contract

### Incremental Delivery

1. Setup + Foundational -> Foundation ready
2. Add US1 (T006-T008) -> 3 tests, message correctness verified
3. Add US2 (T009-T011) -> 6 tests, label correctness verified
4. Add US3 (T012-T014) -> 9 tests, extra fields verified
5. Add US4 (T015-T016) -> 11 tests, wire format verified
6. Add Edge Cases (T017) -> 12 tests, span fields verified
7. Polish (T018-T020) -> fmt + clippy + full validation
