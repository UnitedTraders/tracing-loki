# Feature Specification: Plain Text Log Format with Field-to-Label Mapping

**Feature Branch**: `002-plain-text-format`
**Created**: 2026-02-27
**Status**: Draft
**Input**: User description: "Current library version uses JSON to stream log message to loki. Please add option (via 'Builder') to send plain text message and add other fields as optional labels. Other fields should be configurable (also via Builder): (1) mapping tracing's field name to label (2) excluding all unmapped fields."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Plain Text Log Format (Priority: P1)

A library user wants to send plain text log messages to Loki instead of the current JSON-serialized format. When they configure the Builder with a "plain text" format option, the entry `line` field sent to Loki contains only the human-readable message text (the `message` argument from `tracing::info!("my message")`) rather than a JSON object with all fields embedded.

**Why this priority**: This is the core enabler for the entire feature. Without a plain text format mode, the field-to-label mapping and field exclusion features have no purpose. Many Loki users prefer plain text log lines for readability in Grafana's log viewer.

**Independent Test**: Can be fully tested by configuring the Builder with plain text mode, emitting a tracing event, and verifying the received entry line contains only the message text (not JSON).

**Acceptance Scenarios**:

1. **Given** a Builder configured with plain text format, **When** a user emits `tracing::info!("hello world")`, **Then** the entry line received by Loki is `hello world` (plain text, not JSON).
2. **Given** a Builder configured with plain text format, **When** a user emits `tracing::info!(task = "setup", "starting up")`, **Then** the entry line received by Loki is `starting up` (fields are not included in the line by default).
3. **Given** a Builder with no format configuration (default), **When** a user emits a tracing event, **Then** the entry line is JSON (current behavior is preserved, backward compatible).

---

### User Story 2 - Map Tracing Fields to Loki Labels (Priority: P2)

A library user wants specific tracing event fields (and/or span fields) to be promoted to Loki stream labels. They configure the Builder with a mapping from a tracing field name to a Loki label name. When an event is emitted with that field, the field's value becomes a label on the Loki stream, enabling efficient filtering in Loki's LogQL.

**Why this priority**: Field-to-label mapping is the primary mechanism for making structured tracing data queryable in Loki. This enables users to filter logs by dynamic field values (e.g., `{service="auth"}`) rather than using expensive full-text search.

**Independent Test**: Can be fully tested by configuring a field-to-label mapping, emitting an event with that field, and verifying the mapped field appears as a label in the received stream.

**Acceptance Scenarios**:

1. **Given** a Builder with a field-to-label mapping `"service" -> "service"`, **When** a user emits `tracing::info!(service = "auth", "request")`, **Then** the received stream labels include `service="auth"` alongside the existing `level` label.
2. **Given** a Builder with a field-to-label mapping `"task" -> "task_name"` (renamed), **When** a user emits `tracing::info!(task = "cleanup", "done")`, **Then** the received stream labels include `task_name="cleanup"`.
3. **Given** a Builder with a field-to-label mapping for a field, **When** an event is emitted without that field, **Then** the event is still sent successfully and is assigned to a stream with only the static labels and level (the dynamic label is omitted entirely, not set to an empty or default value).
4. **Given** a Builder with a field-to-label mapping, **When** the mapped field is present, **Then** the field value does NOT also appear in the entry line (it is removed from the line content since it is already a label).

---

### User Story 3 - Exclude Unmapped Fields (Priority: P3)

A library user wants to exclude all tracing fields that are not explicitly mapped to labels from the log entry line. This gives them a clean, minimal log output containing only the message text, with all structured data living in labels.

**Why this priority**: This complements the field-to-label mapping by providing complete control over what appears in the log line. Users who adopt the label-based approach typically want a clean separation: labels for filtering, message for context.

**Independent Test**: Can be fully tested by configuring the Builder with field exclusion enabled, emitting an event with mapped and unmapped fields, and verifying unmapped fields are absent from both labels and the entry line.

**Acceptance Scenarios**:

1. **Given** a Builder with plain text format, a field mapping `"service" -> "service"`, and unmapped field exclusion enabled, **When** a user emits `tracing::info!(service = "auth", request_id = "abc", "request received")`, **Then** the stream labels include `service="auth"` and the entry line is `request received` (the unmapped `request_id` field does not appear anywhere).
2. **Given** a Builder with plain text format and unmapped field exclusion disabled (default), **When** a user emits an event with unmapped fields, **Then** unmapped fields are still included in the entry line in a key=value format appended after the message.
3. **Given** a Builder with JSON format (default) and unmapped field exclusion enabled, **When** a user emits an event, **Then** unmapped fields are excluded from the JSON entry line (only mapped fields that were not promoted to labels, extra fields, and metadata remain).

---

### Edge Cases

- What happens when a mapped field has a value that contains characters invalid for Loki labels (e.g., spaces, special characters)? The value is sent as-is; Loki label values can contain any characters (only label names are restricted to `[A-Za-z_]`).
- What happens when a mapped field name conflicts with an existing static label (e.g., `"host"`)? The Builder rejects mappings whose target label name conflicts with static labels at configuration time.
- What happens when a mapped field name is `"level"`? The Builder rejects this mapping since `"level"` is a reserved label.
- What happens when the same event field is mapped to two different label names? The Builder rejects duplicate source field mappings at configuration time.
- What happens when span fields are mapped to labels? Span fields are treated identically to event fields for mapping purposes. If a span field matches a mapping, it is promoted to a label.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The Builder MUST provide a method to select the log line format: JSON (current default) or plain text.
- **FR-002**: When plain text format is selected, the entry `line` sent to Loki MUST contain only the event's message string, without structured field data.
- **FR-003**: The default format MUST remain JSON to preserve backward compatibility.
- **FR-004**: The Builder MUST provide a method to map a tracing field name to a Loki label name, where the field value at event time becomes the label value.
- **FR-005**: Field-to-label mappings MUST support renaming (source field name differs from target label name).
- **FR-006**: The Builder MUST reject field-to-label mappings whose target label name conflicts with an already-configured static label or the reserved `"level"` label.
- **FR-007**: The Builder MUST reject field-to-label mappings whose target label name contains characters outside `[A-Za-z_]` (same validation as static labels).
- **FR-008**: Fields promoted to labels MUST be removed from the entry line content (both JSON and plain text modes).
- **FR-009**: The Builder MUST provide a method to enable or disable exclusion of unmapped fields from the entry line.
- **FR-010**: When unmapped field exclusion is enabled, only the message (and extra fields) appear in the entry line; all other event and span fields are omitted.
- **FR-011**: When unmapped field exclusion is disabled (default), unmapped fields MUST be included in the entry line in a format appropriate to the selected line format (JSON object fields for JSON mode; key=value pairs appended to the message for plain text mode).
- **FR-012**: When an event does not contain a mapped field, the event MUST still be sent successfully to Loki. The dynamic label is omitted entirely (not set to empty or a default value); the event is assigned to a stream based on only the static labels and level that are present.
- **FR-013**: Extra fields configured via `Builder::extra_field()` MUST continue to appear in the entry line regardless of the unmapped field exclusion setting (they are always included).
- **FR-014**: Metadata fields (`_target`, `_module_path`, `_file`, `_line`, `_spans`) MUST be eligible for field-to-label mapping using the same mechanism as event/span fields. By default (without an explicit mapping), metadata fields MUST be excluded from the entry line when plain text format is selected.
- **FR-015**: Metadata fields MUST continue to appear in the entry line when JSON format is selected (backward compatible).

### Key Entities

- **Field Mapping**: Associates a tracing field name (source) with a Loki label name (target). When a tracing event or span contains the source field, its value is used as the target label's value on the stream.
- **Log Line Format**: A configuration option that determines how the entry `line` is serialized: JSON (structured, current behavior) or plain text (message-only with optional key=value suffix for unmapped fields).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All existing tests continue to pass without modification (backward compatibility preserved).
- **SC-002**: Library users can switch from JSON to plain text format with a single Builder method call.
- **SC-003**: Library users can configure field-to-label mappings and verify the mapped fields appear as labels in Loki queries.
- **SC-004**: Library users can exclude unmapped fields to produce clean, minimal log lines.
- **SC-005**: Invalid configurations (conflicting labels, reserved names, invalid characters) are rejected at build time with clear error messages, not at event-emission time.

## Clarifications

### Session 2026-02-27

- Q: When an event lacks a mapped field, what stream should it be assigned to? → A: Stream uses only static labels + level (dynamic label omitted entirely, not set to empty or default value).

## Assumptions

- Label values in Loki can contain arbitrary UTF-8 strings; only label names are restricted to `[A-Za-z_]`.
- Dynamic labels (from field mappings) create new streams in Loki. Users are expected to understand the cardinality implications (Loki performs best with low-cardinality labels).
- The `message` field in tracing events is the value passed as the format string argument (e.g., `tracing::info!("this is the message")`).
- When plain text mode is selected with unmapped fields included (exclusion disabled), fields are appended as `key=value` pairs separated by spaces after the message, e.g., `my message key1=value1 key2=value2`.
- Extra fields (from `Builder::extra_field()`) are considered static metadata and are always included in the entry line, regardless of the exclusion setting. This matches their current behavior of being embedded in every log entry.
