# Feature Specification: Mock Loki Integration Tests

**Feature Branch**: `001-mock-loki-integration-tests`
**Created**: 2026-02-27
**Status**: Draft
**Input**: User description: "Create integration tests with mock of loki HTTP api and Proto API. Check message, labels, extra fields correctness."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Verify Log Messages Reach Loki Correctly (Priority: P1)

A library contributor changes event serialization logic and needs
confidence that log messages emitted via `tracing` macros arrive at the
Loki push endpoint with correct content. They run `cargo test --all` and
a test suite exercises the full pipeline — Layer captures an event,
BackgroundTask batches and sends it — against a fake Loki HTTP server
running in the same process. The test decodes the received protobuf
payload and asserts that the log message text matches what was emitted.

**Why this priority**: Message correctness is the most fundamental
contract of the library. If messages are garbled or missing, nothing
else matters.

**Independent Test**: Can be fully tested by emitting a single
`tracing::info!("hello")` event, waiting for the fake server to receive
it, decoding the protobuf payload, and asserting the entry line contains
"hello".

**Acceptance Scenarios**:

1. **Given** a Layer connected to a fake Loki HTTP endpoint, **When** a
   `tracing::info!("test message")` event is emitted, **Then** the fake
   server receives exactly one `PushRequest` whose `EntryAdapter` line
   contains "test message".
2. **Given** a Layer connected to a fake Loki endpoint, **When** events
   at each tracing level (trace, debug, info, warn, error) are emitted,
   **Then** each event arrives with the correct level label value
   ("trace", "debug", "info", "warn", "error").
3. **Given** a Layer connected to a fake Loki endpoint, **When** an
   event with structured key-value fields is emitted
   (`tracing::info!(key = "value", "msg")`), **Then** the received
   entry line contains the field name and value in its serialized JSON.

---

### User Story 2 - Verify Labels Are Correctly Applied (Priority: P2)

A library consumer configures custom labels via the Builder
(e.g., `builder.label("service", "my-app")?`) and needs assurance that
those labels appear in every push request sent to Loki, alongside the
automatic "level" label. A contributor modifying label handling logic
runs the test suite to confirm labels are validated, formatted, and
transmitted correctly.

**Why this priority**: Labels are the primary query dimension in Loki.
Incorrect labels make logs unfindable and can cause Loki ingestion
errors.

**Independent Test**: Can be tested by configuring a Builder with
custom labels, emitting an event, and asserting the fake server's
received `StreamAdapter` contains exactly the expected label set.

**Acceptance Scenarios**:

1. **Given** a Builder configured with `label("service", "my-app")` and
   connected to a fake Loki endpoint, **When** an info-level event is
   emitted, **Then** the received `StreamAdapter` labels contain both
   `service="my-app"` and `level="info"`.
2. **Given** a Builder configured with multiple labels
   (`label("host", "a")`, `label("env", "staging")`), **When** an event
   is emitted, **Then** all configured labels plus the "level" label
   appear in the received stream labels.
3. **Given** a Builder with no custom labels, **When** an event is
   emitted, **Then** the received stream contains only the "level"
   label.

---

### User Story 3 - Verify Extra Fields Appear in Log Entries (Priority: P3)

A library consumer configures extra fields via the Builder
(e.g., `builder.extra_field("pid", "1234")?`) and expects those fields
to be included in every log entry's serialized JSON body. A contributor
modifying field serialization runs the tests to confirm extra fields are
present and correctly formatted.

**Why this priority**: Extra fields enrich log entries with contextual
metadata (process ID, hostname, version). Incorrect or missing extra
fields degrade observability.

**Independent Test**: Can be tested by configuring a Builder with extra
fields, emitting an event, and asserting the fake server's received
entry line JSON contains the extra field key-value pairs.

**Acceptance Scenarios**:

1. **Given** a Builder configured with `extra_field("pid", "1234")` and
   connected to a fake Loki endpoint, **When** an info-level event is
   emitted, **Then** the received entry line JSON includes `"pid":"1234"`.
2. **Given** a Builder configured with multiple extra fields
   (`extra_field("pid", "1234")`, `extra_field("version", "0.2.6")`),
   **When** an event is emitted, **Then** all extra fields appear in the
   entry line JSON.
3. **Given** a Builder with extra fields and structured event fields
   (`tracing::info!(task = "init", "starting")`), **When** the event is
   emitted, **Then** both the extra fields and the event's own fields
   appear together in the entry line JSON without overwriting each other.

---

### User Story 4 - Verify Protobuf and Snappy Encoding (Priority: P4)

A contributor modifying the encoding pipeline needs confidence that
payloads are correctly Snappy-compressed and protobuf-encoded. The fake
server decodes the raw HTTP body (Snappy decompress then protobuf
decode) and the tests verify the resulting `PushRequest` structure is
well-formed.

**Why this priority**: Encoding correctness is required for Loki to
accept payloads. This is foundational but lower priority because
encoding is unlikely to regress unless the pipeline is restructured.

**Independent Test**: Can be tested by emitting an event and verifying
the fake server successfully decodes the request body through
Snappy decompression followed by protobuf deserialization into a valid
`PushRequest`.

**Acceptance Scenarios**:

1. **Given** a Layer connected to a fake Loki endpoint, **When** an
   event is emitted, **Then** the fake server can Snappy-decompress the
   request body and protobuf-decode it into a valid `PushRequest` with
   at least one stream and one entry.
2. **Given** a Layer connected to a fake Loki endpoint, **When** an
   event with a known timestamp is emitted, **Then** the decoded
   `EntryAdapter` timestamp is in nanosecond precision and within a
   reasonable tolerance of the emission time.

---

### Edge Cases

- What happens when the fake server returns an HTTP 500 error? The
  BackgroundTask retries; the test verifies the same payload is resent.
- What happens when multiple events are emitted in rapid succession?
  The fake server receives a batch containing all events in a single
  `PushRequest` (or multiple requests), and none are lost.
- What happens when span fields are present? Events emitted inside a
  span include the span's fields in the serialized entry.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The test suite MUST include a fake Loki HTTP server that
  listens on a local port, accepts POST requests to
  `/loki/api/v1/push`, Snappy-decompresses the body, and protobuf-
  decodes it into a `PushRequest`.
- **FR-002**: The fake server MUST capture all received `PushRequest`
  messages and make them available for assertion by test code.
- **FR-003**: Tests MUST verify that emitted tracing events arrive with
  the correct message text in the `EntryAdapter` line field.
- **FR-004**: Tests MUST verify that the "level" label is set correctly
  for each tracing level (trace, debug, info, warn, error).
- **FR-005**: Tests MUST verify that custom labels configured via the
  Builder appear in the received `StreamAdapter` labels.
- **FR-006**: Tests MUST verify that extra fields configured via the
  Builder appear in the received entry line JSON.
- **FR-007**: Tests MUST verify that structured event fields
  (key-value pairs in `tracing` macros) appear in the entry line JSON.
- **FR-008**: Tests MUST verify that the protobuf payload is a valid
  `PushRequest` with well-formed streams and entries.
- **FR-009**: Tests MUST verify that entry timestamps are nanosecond-
  precision and temporally reasonable.
- **FR-010**: All tests MUST run without external services —
  `cargo test --all` MUST pass with no Loki instance running.
- **FR-011**: Tests MUST verify that span fields from active spans are
  included in the serialized entry when events are emitted within a
  span context.

### Key Entities

- **Fake Loki Server**: An in-process HTTP listener that mimics the
  Loki push API endpoint, captures decoded push requests.
- **PushRequest**: The top-level protobuf message containing one or
  more streams, each with labels and log entries.
- **StreamAdapter**: A stream within a PushRequest, identified by its
  label set (including the "level" label).
- **EntryAdapter**: A single log entry within a stream, containing a
  timestamp and a line (serialized JSON with message, fields, and extra
  fields).

### Assumptions

- The fake Loki server binds to `127.0.0.1` on an OS-assigned port
  (port 0) to avoid conflicts.
- Tests use the `tokio` runtime since BackgroundTask is a Future that
  requires an async executor.
- The fake server returns HTTP 200 by default; error-response tests
  configure it to return specific status codes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: `cargo test --all` passes with zero failures and no
  external Loki instance running.
- **SC-002**: Every tracing level (trace, debug, info, warn, error)
  has at least one test verifying correct label assignment.
- **SC-003**: Custom labels, extra fields, and structured event fields
  each have dedicated tests confirming correct round-trip through the
  pipeline.
- **SC-004**: Protobuf decoding and Snappy decompression are exercised
  in at least one test, confirming the wire format matches Loki's
  expected protocol.
- **SC-005**: Span field inclusion is verified by at least one test
  emitting an event inside an active span.
