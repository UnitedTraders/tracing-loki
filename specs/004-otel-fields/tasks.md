# Tasks: OpenTelemetry Field Extraction

**Input**: Design documents from `/specs/004-otel-fields/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/public-api.md, quickstart.md

**Tests**: Integration tests are included per constitution V (testing discipline) and spec acceptance scenarios.

**Organization**: Tasks are grouped by user story. US3 (feature flag) is foundational and blocks all other stories. US1 (core extraction) is MVP. US2 (label promotion) and US4 (plain text) build on US1.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: Add dependencies and feature flag infrastructure

- [x] T001 Add optional `opentelemetry` and `tracing-opentelemetry` dependencies to `Cargo.toml` with feature flag `opentelemetry = ["dep:opentelemetry", "dep:tracing-opentelemetry"]`
- [x] T002 Add `opentelemetry`, `opentelemetry_sdk`, and `tracing-opentelemetry` as dev-dependencies in `Cargo.toml`
- [x] T003 Verify `cargo build --all` succeeds without the feature (no OTel deps compiled)
- [x] T004 Verify `cargo build --all --features opentelemetry` succeeds with the feature

---

## Phase 2: Foundational — Feature-Gated Dependency (US3, Priority: P1)

**Purpose**: Ensure the feature flag correctly gates all OTel code and existing behavior is unchanged

**⚠️ CRITICAL**: No OTel extraction work can begin until this phase confirms compilation and existing tests pass with and without the feature flag.

**Goal**: `opentelemetry` and `tracing-opentelemetry` are only compiled when the feature is enabled. All 39 existing integration tests pass unchanged in both configurations.

**Independent Test**: `cargo test --all` passes without the feature. `cargo test --all --features opentelemetry` passes with the feature. Neither configuration changes existing behavior.

- [x] T005 [US3] Add conditional import block for `tracing_opentelemetry::OtelData` and `opentelemetry::trace::{TraceId, SpanId}` behind `#[cfg(feature = "opentelemetry")]` in `src/lib.rs`
- [x] T006 [US3] Verify all 39 existing integration tests pass without the feature: `cargo test --all`
- [x] T007 [US3] Verify all 39 existing integration tests pass with the feature: `cargo test --all --features opentelemetry`

**Checkpoint**: Feature flag works correctly, existing behavior unchanged in both configurations.

---

## Phase 3: User Story 1 — Include Trace/Span IDs in Log Entries (Priority: P1) 🎯 MVP

**Goal**: When the OTel feature is enabled and a `tracing-opentelemetry` layer is in the subscriber stack, log entries automatically include `trace_id` (32-char hex), `span_id` (16-char hex), and `span_name` fields.

**Independent Test**: Build a subscriber with both a tracing-loki layer and a tracing-opentelemetry layer. Emit events inside instrumented spans. Verify JSON log entries contain `trace_id`, `span_id`, and `span_name` with valid hex-formatted IDs.

### Implementation for User Story 1

- [x] T008 [US1] Implement OTel field extraction in `on_event` in `src/lib.rs`: behind `#[cfg(feature = "opentelemetry")]`, after span field collection, access `span_ref.extensions().get::<OtelData>()` to extract `trace_id` (from `builder.trace_id` or `parent_cx.span().span_context().trace_id()`), `span_id` (from `builder.span_id`), and `span_name` (from `span_ref.name()`). Filter invalid (all-zero) IDs. Inject as `serde_json::Value::String` entries into `span_fields` map.
- [x] T009 [US1] Implement fallback extraction in `on_event` in `src/lib.rs`: when `OtelData` is not present but a span exists (OTel feature enabled, no OTel layer registered), extract `span_id` from `span_ref.id().into_u64()` formatted as `"{:016x}"` and `span_name` from `span_ref.name()`. `trace_id` is omitted.
- [x] T010 [US1] Verify no OTel fields are added for events emitted outside any span in `src/lib.rs`.

### Integration Tests for User Story 1

- [x] T011 [P] [US1] Add integration test `test_otel_json_trace_and_span_ids` in `tests/integration.rs` (behind `#[cfg(feature = "opentelemetry")]`): set up `TracerProvider` + `OpenTelemetryLayer` + Loki layer in subscriber, emit event inside instrumented span, verify JSON entry contains `trace_id` (32-char hex), `span_id` (16-char hex), and `span_name`.
- [x] T012 [P] [US1] Add integration test `test_otel_child_span_trace_id` in `tests/integration.rs`: emit event inside a child span (nested within a parent), verify `trace_id` matches the root span's trace ID (propagated via `parent_cx`).
- [x] T013 [P] [US1] Add integration test `test_otel_fallback_span_id_without_otel_layer` in `tests/integration.rs`: set up subscriber with Loki layer but WITHOUT OpenTelemetryLayer, emit event inside a span, verify `span_id` is present (16-char hex from tracing ID), `span_name` is present, and `trace_id` is absent.
- [x] T014 [P] [US1] Add integration test `test_otel_no_fields_outside_span` in `tests/integration.rs`: emit event outside any span, verify entry contains no `trace_id`, `span_id`, or `span_name` fields.

**Checkpoint**: OTel fields appear in JSON log entries for instrumented spans. Fallback works without OTel layer. No fields for events outside spans. All existing tests still pass.

---

## Phase 4: User Story 2 — Promote OTel Fields to Loki Labels (Priority: P2)

**Goal**: OTel fields can be promoted to Loki stream labels via the existing `field_to_label` builder method, enabling `{trace_id="abc123"}` queries in LogQL.

**Independent Test**: Build a layer with `.field_to_label("trace_id", "trace_id")` and OTel enabled. Emit events inside an instrumented span. Verify `trace_id` appears as a Loki stream label and is removed from the JSON entry line.

### Implementation for User Story 2

No code changes required — OTel fields are injected into `span_fields` before dynamic label extraction (done in T008), so `field_to_label` already works. This phase validates the integration through tests.

### Integration Tests for User Story 2

- [x] T015 [P] [US2] Add integration test `test_otel_field_to_label_trace_id` in `tests/integration.rs`: configure `.field_to_label("trace_id", "trace_id")`, emit event in OTel-instrumented span, verify `trace_id` appears in Loki stream labels and is removed from the JSON entry body.
- [x] T016 [P] [US2] Add integration test `test_otel_field_to_label_span_id` in `tests/integration.rs`: configure `.field_to_label("span_id", "span_id")`, emit event in span, verify `span_id` appears in Loki stream labels.
- [x] T017 [P] [US2] Add integration test `test_otel_no_label_promotion_without_mapping` in `tests/integration.rs`: emit event with OTel fields but no `field_to_label` mapping, verify OTel fields appear only in the JSON entry body and NOT as stream labels.

**Checkpoint**: OTel fields promoted to Loki stream labels via `field_to_label`. No promotion without explicit mapping. All existing tests still pass.

---

## Phase 5: User Story 4 — Plain Text Format Support (Priority: P2)

**Goal**: OTel fields appear as `key=value` pairs in plain text log entries, consistent with JSON format behavior.

**Independent Test**: Build a layer in plain text mode with OTel enabled. Emit events inside instrumented spans. Verify `trace_id=<hex> span_id=<hex> span_name=<name>` appear in the plain text entry.

### Implementation for User Story 4

No additional code changes required — OTel fields are injected into `span_fields` (T008/T009), and the plain text serialization path already iterates `span_fields` to produce `key=value` pairs. This phase validates through tests.

### Integration Tests for User Story 4

- [x] T018 [P] [US4] Add integration test `test_otel_plain_text_fields` in `tests/integration.rs`: configure `.plain_text()` with OTel, emit event in instrumented span, verify entry line contains `trace_id=<hex>`, `span_id=<hex>`, and `span_name=<name>` as key=value pairs.
- [x] T019 [P] [US4] Add integration test `test_otel_plain_text_exclude_unmapped` in `tests/integration.rs`: configure `.plain_text().exclude_unmapped_fields()` with `.field_to_label("trace_id", "trace_id")`, emit event in OTel span, verify `trace_id` is a stream label and is excluded from the entry line, while `span_id` and `span_name` are also excluded (unmapped).

**Checkpoint**: OTel fields work correctly in plain text mode. Exclusion rules apply consistently. All tests pass.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, quality checks, and cleanup

- [x] T020 Verify all existing 39 integration tests pass without OTel feature: `cargo test --all`
- [x] T021 Verify all tests (existing + new OTel) pass with OTel feature: `cargo test --all --features opentelemetry`
- [x] T022 Run `cargo clippy --all-targets --all-features` and fix any warnings
- [x] T023 Run `cargo fmt -- --check` and fix any formatting issues
- [x] T024 Run quickstart.md verification checklist

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational/US3 (Phase 2)**: Depends on Setup completion — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Phase 2 — core extraction
- **US2 (Phase 4)**: Depends on Phase 3 (US1) — tests validate label promotion of OTel fields
- **US4 (Phase 5)**: Depends on Phase 3 (US1) — tests validate plain text formatting of OTel fields
- **Polish (Phase 6)**: Depends on all previous phases

### User Story Dependencies

- **US3 (Feature Flag, P1)**: Foundational — blocks all other stories
- **US1 (Core Extraction, P1)**: Depends on US3 — the MVP
- **US2 (Label Promotion, P2)**: Depends on US1 — can run in parallel with US4
- **US4 (Plain Text, P2)**: Depends on US1 — can run in parallel with US2

### Within Each User Story

- Implementation tasks before integration tests (for US1: T008-T010 before T011-T014)
- Tests marked [P] can run in parallel within their story

### Parallel Opportunities

- T001 and T002 can be combined (same file `Cargo.toml`)
- T011, T012, T013, T014 can all run in parallel (independent test functions)
- T015, T016, T017 can all run in parallel
- T018, T019 can run in parallel
- US2 (Phase 4) and US4 (Phase 5) can run in parallel after US1 completes

---

## Parallel Example: User Story 1 Tests

```bash
# After T008-T010 are complete, launch all US1 tests in parallel:
Task: "test_otel_json_trace_and_span_ids in tests/integration.rs"
Task: "test_otel_child_span_trace_id in tests/integration.rs"
Task: "test_otel_fallback_span_id_without_otel_layer in tests/integration.rs"
Task: "test_otel_no_fields_outside_span in tests/integration.rs"
```

## Parallel Example: US2 and US4

```bash
# After US1 is complete, launch US2 and US4 phases in parallel:
# US2 tests:
Task: "test_otel_field_to_label_trace_id in tests/integration.rs"
Task: "test_otel_field_to_label_span_id in tests/integration.rs"
Task: "test_otel_no_label_promotion_without_mapping in tests/integration.rs"

# US4 tests (parallel with US2):
Task: "test_otel_plain_text_fields in tests/integration.rs"
Task: "test_otel_plain_text_exclude_unmapped in tests/integration.rs"
```

---

## Implementation Strategy

### MVP First (US3 + US1)

1. Complete Phase 1: Setup (add deps + feature flag to Cargo.toml)
2. Complete Phase 2: US3 — verify feature flag works, existing tests pass
3. Complete Phase 3: US1 — implement OTel extraction + fallback + tests
4. **STOP and VALIDATE**: `cargo test --all --features opentelemetry` passes, JSON entries contain OTel fields
5. This is a fully functional deliverable

### Incremental Delivery

1. Setup + US3 → Feature flag infrastructure ready
2. US1 → Core OTel extraction works in JSON → **MVP complete**
3. US2 → Label promotion validated → LogQL queries by trace_id possible
4. US4 → Plain text format validated → Full format parity
5. Polish → All quality checks pass

---

## Notes

- All new test functions in `tests/integration.rs` must be gated with `#[cfg(feature = "opentelemetry")]`
- The `FakeLokiServer` and test helpers from existing tests are reused as-is
- OTel test setup pattern: `TracerProvider::builder().build()` → `provider.tracer("test")` → `OpenTelemetryLayer::new(tracer)` → add to subscriber with Loki layer
- US2 and US4 require NO code changes — they are test-only phases that validate the injection point design from R5
- The `span_fields` injection approach (R5) means `field_to_label` and plain text serialization work automatically
