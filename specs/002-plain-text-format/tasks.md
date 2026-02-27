# Tasks: Plain Text Log Format with Field-to-Label Mapping

**Input**: Design documents from `/specs/002-plain-text-format/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, quickstart.md

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup

**Purpose**: Add new types, error variants, and Builder API surface needed by all user stories.

- [x] T001 Add `LogLineFormat` enum (`Json`, `PlainText`) and `FieldMapping` struct (`source_field: String`, `target_label: String`) to `src/builder.rs`. Add `log_format`, `field_mappings: Vec<FieldMapping>`, and `exclude_unmapped_fields: bool` fields to the `Builder` struct with defaults (`Json`, empty vec, `false`). Ensure `Builder` still derives `Clone`.
- [x] T002 Add new `ErrorInner` variants to `src/lib.rs`: `DuplicateFieldMapping(String)`, `FieldMappingConflictsWithLabel(String)`, `InvalidFieldMappingLabelCharacter(String, char)`. Add `Display` match arms with descriptive messages for each new variant.
- [x] T003 Implement three new Builder methods in `src/builder.rs`: (1) `plain_text(self) -> Builder` sets `log_format` to `PlainText`; (2) `field_to_label(self, source: S, target: T) -> Result<Builder, Error>` validates target label characters match `[A-Za-z_]`, rejects `"level"`, rejects conflicts with existing static label names (check `self.labels.seen_keys`), rejects duplicate source fields, then pushes `FieldMapping`; (3) `exclude_unmapped_fields(self) -> Builder` sets the flag to `true`. Add rustdoc with examples to all three methods.
- [x] T004 Make `FormattedLabels::seen_keys` accessible for conflict checking in `src/labels.rs`. Add a `pub fn contains(&self, key: &str) -> bool` method that checks if a label name is already registered. Also add a `pub fn finish_with_dynamic(&self, level: Level, dynamic: &HashMap<String, String>) -> String` method that builds the full Prometheus label string including dynamic labels (sorted alphabetically for deterministic ordering). Run `cargo check` to verify compilation.

**Checkpoint**: `cargo check` compiles. Builder API methods exist. No behavior changes yet.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Modify `LokiEvent` and `BackgroundTask` queue routing so that dynamic labels flow through the system. These changes are required by ALL user stories.

**CRITICAL**: No user story work can begin until this phase is complete.

- [x] T005 Add `dynamic_labels: HashMap<String, String>` field to `LokiEvent` struct in `src/lib.rs`. Update all existing `LokiEvent` construction sites in `Layer::on_event` to include `dynamic_labels: HashMap::new()` (no behavior change yet).
- [x] T006 Add `log_format: LogLineFormat`, `field_mappings: Vec<FieldMapping>`, and `exclude_unmapped_fields: bool` fields to the `Layer` struct in `src/lib.rs`. Update `Builder::build_url` and `Builder::build_controller_url` in `src/builder.rs` to pass these from Builder to Layer. Import `LogLineFormat` and `FieldMapping` in `src/lib.rs`.
- [x] T007 Replace `queues: LevelMap<SendQueue>` with `queues: HashMap<String, SendQueue>` in `BackgroundTask` in `src/lib.rs`. Store the `FormattedLabels` (or a clone) in `BackgroundTask` so it can construct label strings at runtime. Update `BackgroundTask::new` to accept and store labels. In the `poll` method, when receiving a `LokiEvent`: compute the full label string via `labels.finish_with_dynamic(event.level, &event.dynamic_labels)`, look up or insert a `SendQueue` for that key, push the event. Update all `self.queues.values()` / `self.queues.values_mut()` iteration to use `HashMap` methods. Run `cargo test --all` to verify all existing tests still pass (SC-001).

**Checkpoint**: Foundation ready. Dynamic labels flow through the pipeline. All existing tests pass. `cargo test --all` green.

---

## Phase 3: User Story 1 — Plain Text Log Format (Priority: P1)

**Goal**: Enable `.plain_text()` to produce message-only entry lines instead of JSON.

**Independent Test**: Configure Builder with `.plain_text()`, emit events, verify entry line is plain text.

### Implementation for User Story 1

- [x] T008 [US1] Implement plain text serialization in `Layer::on_event` in `src/lib.rs`. Add a format branch: when `self.log_format` is `PlainText`, extract the `message` field from the event using a lightweight `Visit` implementation (capture only `field.name() == "message"` via `record_str`/`record_debug`). When `exclude_unmapped_fields` is false, append remaining event fields and span fields as `key=value` pairs after the message (values with spaces quoted). Extra fields are always appended. Metadata fields (`_target`, `_module_path`, `_file`, `_line`, `_spans`) are excluded by default in plain text mode. When `log_format` is `Json`, use the existing `SerializedEvent` path (no change). Covers FR-001, FR-002, FR-003, FR-011, FR-013, FR-014, FR-015.
- [x] T009 [US1] Add integration test `test_plain_text_basic` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test")`. Emit `tracing::info!("hello world")`. Assert entry line equals exactly `"hello world"`. Covers FR-002.
- [x] T010 [P] [US1] Add integration test `test_plain_text_with_fields_included` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test")` (exclude_unmapped_fields defaults to false). Emit `tracing::info!(task = "setup", "starting up")`. Assert entry line starts with `"starting up"` and contains `task=` somewhere. Covers FR-011.
- [x] T011 [P] [US1] Add integration test `test_plain_text_metadata_excluded` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test")`. Emit `tracing::info!("check meta")`. Assert entry line does NOT contain `_target`, `_file`, `_line`, `_spans`, `_module_path`. Covers FR-014.
- [x] T012 [P] [US1] Add integration test `test_default_format_unchanged` in `tests/integration.rs`. Build Layer with default config (no `.plain_text()`). Emit `tracing::info!("json check")`. Parse entry line as JSON. Assert `json["message"] == "json check"` and `json["_target"]` is present. Covers FR-003, FR-015.
- [x] T013 [P] [US1] Add integration test `test_plain_text_extra_fields_included` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test").extra_field("pid", "1234")`. Emit `tracing::info!("with extra")`. Assert entry line contains `pid=1234` or `pid="1234"`. Covers FR-013.

**Checkpoint**: User Story 1 complete. Plain text mode works. Default JSON behavior preserved. 5 new tests pass. `cargo test --test integration` green.

---

## Phase 4: User Story 2 — Map Tracing Fields to Loki Labels (Priority: P2)

**Goal**: Enable `.field_to_label()` to promote event/span fields to stream labels.

**Independent Test**: Configure field mapping, emit event with that field, verify it appears as stream label and is removed from entry line.

### Implementation for User Story 2

- [x] T014 [US2] Implement field extraction in `Layer::on_event` in `src/lib.rs`. After collecting event fields (via existing `SerializeEventFieldMapStrippingLog`) and span fields, iterate `self.field_mappings`. For each mapping, check if `source_field` exists in the collected event fields map, span fields map, or metadata values (`_target`, `_module_path`, `_file`, `_line`). If found, convert value to `String` (for JSON values use display/as_str), add to `dynamic_labels` HashMap with target_label as key, and remove the field from the respective collection so it does not appear in the entry line. Pass the populated `dynamic_labels` into `LokiEvent`. Covers FR-004, FR-005, FR-008, FR-012, FR-014.
- [x] T015 [US2] Add integration test `test_field_to_label_basic` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("service", "service")`. Emit `tracing::info!(service = "auth", "request")`. Assert stream labels contain `service="auth"`, `host="test"`, `level="info"`. Assert entry line does NOT contain `"service"` or `"auth"`. Covers FR-004, FR-008.
- [x] T016 [P] [US2] Add integration test `test_field_to_label_rename` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("task", "task_name")`. Emit `tracing::info!(task = "cleanup", "done")`. Assert stream labels contain `task_name="cleanup"`. Assert stream labels do NOT contain a label named `task`. Covers FR-005.
- [x] T017 [P] [US2] Add integration test `test_field_to_label_missing_field` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("service", "service")`. Emit `tracing::info!("no service field")`. Assert event is received successfully. Assert stream labels contain only `host="test"` and `level="info"` (no `service` label). Covers FR-012.
- [x] T018 [P] [US2] Add integration test `test_field_to_label_span_field` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("span_field", "span_field")`. Create a span `tracing::info_span!("my_span", span_field = "span_value")`, enter it, emit `tracing::info!("inside span")`. Assert stream labels contain `span_field="span_value"`. Assert entry line does NOT contain `span_field`. Covers edge case for span fields.
- [x] T019 [P] [US2] Add integration test `test_field_to_label_metadata` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("_target", "target")`. Emit `tracing::info!("meta mapped")`. Assert stream labels contain `target="integration"` (the test module name). Covers FR-014.
- [x] T020 [P] [US2] Add integration tests for Builder validation errors in `tests/integration.rs`: (1) `test_field_to_label_rejects_level` — `.field_to_label("x", "level")` returns Err. (2) `test_field_to_label_rejects_conflict` — `.label("host", "test").field_to_label("x", "host")` returns Err. (3) `test_field_to_label_rejects_invalid_chars` — `.field_to_label("x", "bad-name")` returns Err. (4) `test_field_to_label_rejects_duplicate_source` — two `.field_to_label("x", "a")` and `.field_to_label("x", "b")` returns Err. Covers FR-006, FR-007, SC-005.

**Checkpoint**: User Story 2 complete. Field-to-label mapping works for event fields, span fields, and metadata. Validation errors caught at build time. 6 new tests pass.

---

## Phase 5: User Story 3 — Exclude Unmapped Fields (Priority: P3)

**Goal**: Enable `.exclude_unmapped_fields()` to drop non-mapped fields from entry lines.

**Independent Test**: Configure exclusion, emit event with mapped and unmapped fields, verify only mapped fields appear as labels and entry line is message-only.

### Implementation for User Story 3

- [x] T021 [US3] Implement unmapped field exclusion in `Layer::on_event` in `src/lib.rs`. When `self.exclude_unmapped_fields` is true, after extracting mapped fields into `dynamic_labels`, discard all remaining event fields and span fields before serialization. Extra fields are NOT discarded (they are always included per FR-013). In JSON mode, the serialized JSON should contain only `message`, extra fields, and metadata. In PlainText mode, the entry line should contain only the message and extra fields. Covers FR-009, FR-010.
- [x] T022 [US3] Add integration test `test_exclude_unmapped_plain_text` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test").field_to_label("service", "service").exclude_unmapped_fields()`. Emit `tracing::info!(service = "auth", request_id = "abc", "request received")`. Assert stream labels include `service="auth"`. Assert entry line equals `"request received"` (no `request_id`, no `service`). Covers FR-009, FR-010.
- [x] T023 [P] [US3] Add integration test `test_exclude_unmapped_json` in `tests/integration.rs`. Build Layer with `.label("host", "test").field_to_label("service", "service").exclude_unmapped_fields()` (JSON default). Emit `tracing::info!(service = "auth", request_id = "abc", "filtered")`. Parse entry line as JSON. Assert `json["message"] == "filtered"`. Assert `json["request_id"]` is absent (excluded). Assert `json["service"]` is absent (promoted to label). Assert `json["_target"]` is present (metadata stays in JSON). Covers FR-010, FR-015.
- [x] T024 [P] [US3] Add integration test `test_exclude_unmapped_preserves_extra_fields` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test").extra_field("pid", "1234").exclude_unmapped_fields()`. Emit `tracing::info!(task = "init", "starting")`. Assert entry line contains `pid` (extra field preserved). Assert entry line does NOT contain `task` (unmapped, excluded). Covers FR-013.
- [x] T025 [P] [US3] Add integration test `test_unmapped_fields_included_by_default` in `tests/integration.rs`. Build Layer with `.plain_text().label("host", "test").field_to_label("service", "service")` (exclude_unmapped_fields defaults false). Emit `tracing::info!(service = "auth", request_id = "abc", "with unmapped")`. Assert entry line contains `request_id` (unmapped fields included by default). Assert stream labels include `service="auth"`. Covers FR-011.

**Checkpoint**: User Story 3 complete. Unmapped field exclusion works in both JSON and plain text modes. Extra fields always preserved. 4 new tests pass.

---

## Phase 6: Polish & Validation

**Purpose**: Final validation, formatting, and documentation.

- [x] T026 Run `cargo fmt -- --check` on the entire workspace. Fix any formatting issues.
- [x] T027 [P] Run `cargo clippy --all-targets --all-features` and fix any warnings.
- [x] T028 [P] Run `cargo test --all` and verify all tests pass (existing + new). Verify no external services required. Verify SC-001 (backward compatibility).
- [x] T029 [P] Add rustdoc examples to the public `LogLineFormat` enum if it is made public, or verify the Builder method docs include complete usage examples in `src/builder.rs`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **User Stories (Phase 3-5)**: All depend on Foundational phase completion
  - US1 (Phase 3): No dependency on other stories
  - US2 (Phase 4): No dependency on other stories
  - US3 (Phase 5): Depends on US2 (field_to_label extraction logic in T014)
- **Polish (Phase 6)**: Depends on all phases being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Phase 2 — No dependencies on other stories
- **User Story 2 (P2)**: Can start after Phase 2 — No dependencies on other stories
- **User Story 3 (P3)**: Depends on US2 (T014) for field extraction logic — exclude_unmapped_fields operates on the same extraction pipeline

### Within Each User Story

- Implementation task first (establishes the pattern)
- Test tasks marked [P] can be written in parallel after implementation
- All tasks within a story target `tests/integration.rs` or `src/` files

### Parallel Opportunities

- T001 and T002 can run in parallel (different files: `builder.rs` vs `lib.rs`)
- T003 depends on T001 and T002 (uses types from both)
- T004 is independent (different file: `labels.rs`)
- After Phase 2, US1 and US2 can proceed in parallel
- US3 should follow US2 (depends on field extraction logic)
- Within each story, test tasks marked [P] can run in parallel

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T004)
2. Complete Phase 2: Foundational (T005-T007)
3. Complete Phase 3: User Story 1 (T008-T013)
4. **STOP and VALIDATE**: `cargo test --test integration` passes with 5 new plain text tests + all existing tests
5. This alone provides the core plain text format feature

### Incremental Delivery

1. Setup + Foundational -> Foundation ready
2. Add US1 (T008-T013) -> 5 new tests, plain text format works
3. Add US2 (T014-T020) -> 6 new tests, field-to-label mapping works
4. Add US3 (T021-T025) -> 4 new tests, unmapped field exclusion works
5. Polish (T026-T029) -> fmt + clippy + full validation
