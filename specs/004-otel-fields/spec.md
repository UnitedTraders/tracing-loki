# Feature Specification: OpenTelemetry Field Extraction

**Feature Branch**: `004-otel-fields`
**Created**: 2026-03-09
**Status**: Draft
**Input**: User description: "Add support for extracting OpenTelemetry fields 'span_id', 'trace_id', 'span_name' using tracing-opentelemetry and optionally including them as labels for Loki. Consult jraffeiner:tracing-loki:local-dev for possible impl"

## User Scenarios & Testing

### User Story 1 - Include Trace/Span IDs in Log Entries (Priority: P1)

As a developer operating a distributed system with OpenTelemetry tracing enabled, I want my Loki log entries to automatically include `trace_id`, `span_id`, and `span_name` fields so that I can correlate logs with distributed traces in Grafana.

**Why this priority**: This is the core value of the feature. Without OTel field extraction, users cannot correlate Loki logs with traces in Tempo/Jaeger. This is the primary reason for adopting this feature.

**Independent Test**: Build a layer with the OTel feature enabled and a `tracing-opentelemetry` layer in the subscriber stack. Emit events within an instrumented span. Verify that the log entry JSON contains `trace_id`, `span_id`, and `span_name` fields with valid hex-formatted IDs.

**Acceptance Scenarios**:

1. **Given** a subscriber stack with both a tracing-loki layer and a tracing-opentelemetry layer, **When** an event is emitted inside an instrumented span, **Then** the log entry contains `trace_id` (32-char hex), `span_id` (16-char hex), and `span_name` (the span's name) fields.
2. **Given** a subscriber stack with a tracing-loki layer but WITHOUT a tracing-opentelemetry layer, **When** an event is emitted inside a span, **Then** the log entry contains `span_id` (derived from the tracing span ID as 16-char hex) and `span_name`, but does NOT contain `trace_id`.
3. **Given** an event emitted outside any span, **When** it is processed by the layer, **Then** the log entry does NOT contain `span_id`, `trace_id`, or `span_name` fields.

---

### User Story 2 - Promote OTel Fields to Loki Labels (Priority: P2)

As a developer querying logs in Grafana, I want to optionally promote `trace_id` or `span_id` to Loki stream labels so that I can query logs by trace ID directly in LogQL using label matchers (e.g., `{trace_id="abc123"}`).

**Why this priority**: Promoting OTel fields to labels enables efficient LogQL queries. However, high-cardinality labels (like `trace_id`) can cause performance issues in Loki, so this must be opt-in. The core value (US1) works without this.

**Independent Test**: Build a layer with the OTel feature enabled and use the existing `field_to_label` builder method to map `trace_id` to a Loki label. Emit events inside an instrumented span. Verify that the Loki push request contains `trace_id` as a stream label.

**Acceptance Scenarios**:

1. **Given** a builder configured with `.field_to_label("trace_id", "trace_id")`, **When** an event is emitted inside an OTel-instrumented span, **Then** the Loki stream labels include `trace_id` with the correct hex value, and the field is removed from the entry line.
2. **Given** a builder configured with `.field_to_label("span_id", "span_id")`, **When** an event is emitted inside a span, **Then** the Loki stream labels include `span_id` with the correct hex value.
3. **Given** a builder with NO field-to-label mapping for OTel fields, **When** events are emitted, **Then** OTel fields appear only in the entry line (JSON body), not as stream labels.

---

### User Story 3 - Feature-Gated OTel Dependency (Priority: P1)

As a library consumer who does NOT use OpenTelemetry, I do not want the `opentelemetry` and `tracing-opentelemetry` crate dependencies compiled into my binary. The OTel integration should be opt-in via a feature flag.

**Why this priority**: Adding mandatory dependencies for `opentelemetry` and `tracing-opentelemetry` significantly increases compile times and binary size for users who don't need OTel. This is equally important as US1 because it affects all existing users.

**Independent Test**: Build the library without the OTel feature flag enabled. Verify that `opentelemetry` and `tracing-opentelemetry` are NOT in the dependency tree. Verify all existing tests still pass.

**Acceptance Scenarios**:

1. **Given** the library built without the OTel feature flag, **When** checking the dependency tree, **Then** neither `opentelemetry` nor `tracing-opentelemetry` are present.
2. **Given** the library built with the OTel feature flag enabled, **When** an event is emitted inside an OTel-instrumented span, **Then** `trace_id`, `span_id`, and `span_name` are extracted and included in the log entry.
3. **Given** the library built without the OTel feature flag, **When** events are emitted inside spans, **Then** log entries are identical to current behavior — no `span_id`, `span_name`, or `trace_id` fields are added.

---

### User Story 4 - Plain Text Format Support (Priority: P2)

As a developer using plain text log format, I want OTel fields to also appear in plain text log entries so that trace correlation works regardless of the chosen format.

**Why this priority**: Plain text mode was added in feature 002 and is actively used. OTel fields should work consistently across both formats.

**Independent Test**: Build a layer in plain text mode with OTel enabled. Emit events inside instrumented spans. Verify that `span_id`, `trace_id`, and `span_name` appear as `key=value` pairs in the plain text output.

**Acceptance Scenarios**:

1. **Given** a plain text mode layer with OTel enabled, **When** an event is emitted inside an instrumented span, **Then** the entry line contains `trace_id=<hex> span_id=<hex> span_name=<name>`.
2. **Given** a plain text mode layer with `exclude_unmapped_fields` and `field_to_label("trace_id", "trace_id")`, **When** an event is emitted, **Then** `trace_id` appears as a label and is excluded from the entry line, while `span_id` and `span_name` are also excluded (unmapped).

---

### Edge Cases

- What happens when `OtelData` exists in span extensions but `span_id()` or `trace_id()` returns `None`? The system falls back: `span_id` uses the tracing-internal span ID; `trace_id` is omitted.
- What happens when the OTel feature is enabled but no `tracing-opentelemetry` layer is registered? The `OtelData` extension won't exist, so fallback behavior applies — identical to OTel-disabled behavior.
- What happens with nested spans? The `span_id` and `trace_id` come from the immediate parent span (or the span the event is attached to), not from the root span. `span_name` is the name of that same span.
- What happens with very high event volumes and OTel extraction? The extraction cost is one `extensions()` lookup per event — this is the same cost as the existing span field extraction and should not measurably impact performance.

## Requirements

### Functional Requirements

- **FR-001**: When the OTel feature is enabled and `OtelData` is available in the current span's extensions, the system MUST extract `trace_id` and format it as a lowercase hex string.
- **FR-002**: When the OTel feature is enabled and `OtelData` is available, the system MUST extract `span_id` and format it as a 16-character zero-padded lowercase hex string.
- **FR-003**: The system MUST extract `span_name` from the current span's name (this does not depend on OTel).
- **FR-004**: When the OTel feature is enabled and `OtelData` is NOT available but a tracing span exists, the system MUST fall back to using the tracing-internal span ID formatted as a 16-character zero-padded lowercase hex string for `span_id`.
- **FR-005**: When no span context exists (event emitted outside any span), the system MUST omit `span_id`, `trace_id`, and `span_name` from the log entry.
- **FR-006**: In JSON format, OTel fields MUST be serialized as top-level keys (`span_id`, `trace_id`, `span_name`) and MUST be omitted when their value is absent.
- **FR-007**: In plain text format, OTel fields MUST be appended as `key=value` pairs when present.
- **FR-008**: OTel fields (`trace_id`, `span_id`, `span_name`) MUST be eligible for promotion to Loki stream labels via the existing `field_to_label` builder method.
- **FR-009**: The `opentelemetry` and `tracing-opentelemetry` dependencies MUST be gated behind an optional feature flag (not compiled by default).
- **FR-010**: When the OTel feature flag is disabled, the system MUST NOT extract or include `span_id`, `span_name`, or `trace_id` — existing behavior is unchanged.
- **FR-011**: Enabling the OTel feature flag MUST NOT change any existing behavior for users who don't have a `tracing-opentelemetry` layer registered — fallback behavior is identical to the non-OTel path.
- **FR-012**: The OTel field extraction MUST NOT block or introduce allocations beyond the hex string formatting (two `format!` calls per event when OTel data is present).

### Key Entities

- **OTel Fields**: Three optional string fields (`trace_id`, `span_id`, `span_name`) extracted per event from the span context. These are computed during `on_event` processing and included in the log entry.
- **Feature Flag**: A build-time opt-in gate that controls whether `opentelemetry` and `tracing-opentelemetry` are compiled as dependencies and whether `OtelData` extraction is attempted.

## Success Criteria

### Measurable Outcomes

- **SC-001**: When OTel is enabled and a tracing-opentelemetry layer is active, 100% of events emitted within instrumented spans contain valid `trace_id`, `span_id`, and `span_name` fields.
- **SC-002**: When OTel is disabled (feature flag off), the library compiles and all existing tests pass without `opentelemetry` or `tracing-opentelemetry` in the dependency tree.
- **SC-003**: OTel fields can be promoted to Loki stream labels using the existing `field_to_label` mechanism, enabling `{trace_id="<hex>"}` queries in LogQL.
- **SC-004**: The OTel field extraction adds no measurable latency to the synchronous `on_event` path (one span extension lookup, two string format operations).
- **SC-005**: All existing tests (39 integration + 3 unit + 13 doc tests) continue to pass with no changes when the OTel feature is disabled.
- **SC-006**: OTel fields work correctly in both JSON and plain text log formats.

## Clarifications

### Session 2026-03-09

- Q: Should `span_id` and `span_name` always be extracted (even without OTel feature flag)? → A: No. All three fields (`span_id`, `span_name`, `trace_id`) are only extracted when the OTel feature flag is enabled. No behavioral change for existing users without the flag.

## Assumptions

- The `tracing-opentelemetry` crate provides `OtelData` as a span extension type that is accessible via `span.extensions().get::<OtelData>()`.
- `OtelData` provides methods to retrieve span and trace IDs from the OpenTelemetry span context.
- The `span_id` fallback (tracing-internal ID as hex) is used when the OTel feature is enabled but no `tracing-opentelemetry` layer is registered. When the OTel feature is disabled, no `span_id` is extracted at all.
- The feature flag name will follow the crate's existing naming convention (lowercase, hyphenated).
- High-cardinality warnings for `trace_id` as a Loki label are the user's responsibility — the library does not validate label cardinality.

## Scope

### In Scope

- Extracting `trace_id`, `span_id`, `span_name` from span extensions
- Feature-gated `opentelemetry` / `tracing-opentelemetry` dependencies
- Including OTel fields in both JSON and plain text log entry formats
- Making OTel fields eligible for `field_to_label` promotion
- Fallback behavior when OTel data is not available

### Out of Scope

- Configuring or initializing OpenTelemetry tracing pipelines
- Exporting traces to OTel collectors
- Adding `_span_ids` array field (from reference implementation — not requested)
- Automatic promotion of OTel fields to labels (must be explicit via `field_to_label`)
- W3C trace context propagation headers
