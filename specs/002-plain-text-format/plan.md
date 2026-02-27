# Implementation Plan: Plain Text Log Format with Field-to-Label Mapping

**Branch**: `002-plain-text-format` | **Date**: 2026-02-27 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/002-plain-text-format/spec.md`

## Summary

Add a plain text log line format option and field-to-label mapping to the
tracing-loki Builder. Currently, all log entries are serialized as JSON objects.
This feature adds: (1) a `.plain_text()` Builder method to emit message-only
entry lines, (2) `.field_to_label(source, target)` to promote event/span fields
to Loki stream labels, and (3) `.exclude_unmapped_fields()` to drop unmapped
fields from the entry line. The implementation modifies `Builder`, `Layer`,
`LokiEvent`, and `BackgroundTask` to support dynamic label routing and
configurable serialization.

## Technical Context

**Language/Version**: Rust 2021 edition, stable + nightly (CI matrix)
**Primary Dependencies**: tracing 0.1, tracing-subscriber 0.3, reqwest, prost, snap, serde_json, loki-api (workspace)
**Storage**: N/A
**Testing**: `cargo test --all` with `tokio::test` async runtime, fake Loki server (axum)
**Target Platform**: Cross-platform (macOS, Linux)
**Project Type**: Library (public API extension)
**Performance Goals**: `Layer::on_event` must remain low-latency; no unbounded allocations
**Constraints**: No `unsafe`; backward compatible; no new required dependencies
**Scale/Scope**: ~4 modified source files, ~200-400 lines new code, ~10-15 new integration tests

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Library-First API Design | PASS | New Builder methods follow existing fluent pattern; return `Result` for fallible config |
| II. Correctness & Safety | PASS | No `unsafe`; validation at build time; label names checked against `[A-Za-z_]` |
| III. Async Discipline | PASS | All new logic in synchronous `on_event` (field extraction, serialization) or `BackgroundTask` (queue routing). No new `.await` in Layer. |
| IV. Protocol Fidelity | PASS | Protobuf `PushRequest` structure unchanged; only label strings and entry line content change |
| V. Testing Discipline | PASS | Integration tests use fake Loki server; no external services |
| VI. Performance & Resource Bounds | PASS | Dynamic labels create bounded SendQueues (bounded by distinct label combinations); field extraction is O(mappings * fields) per event |
| VII. Backward Compatibility | PASS | Default format remains JSON; default exclusion disabled; no existing API changes |
| Rust Standards | PASS | Must pass fmt + clippy; new public items get rustdoc |

## Project Structure

### Documentation (this feature)

```text
specs/002-plain-text-format/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── builder.rs           # Modified: add log_format, field_mappings, exclude_unmapped_fields
├── lib.rs               # Modified: Layer fields, LokiEvent struct, on_event serialization,
│                        #           BackgroundTask queue routing (HashMap<String, SendQueue>)
├── labels.rs            # Modified: add method to format dynamic labels into label string
├── level_map.rs         # Unchanged (no longer used for queue keying, but kept for compat)
├── log_support.rs       # Unchanged
└── no_subscriber.rs     # Unchanged

tests/
└── integration.rs       # Modified: add ~10-15 new tests for plain text, field mapping, exclusion
```

**Structure Decision**: Single-crate library. All changes are within the existing
source files. No new modules needed — the `LogLineFormat` enum and `FieldMapping`
struct are small enough to live in `builder.rs`. The `Layer` and `BackgroundTask`
modifications are in `lib.rs` alongside their existing implementations.

## Design Details

### Builder API Additions

Three new methods on `Builder`:

- `plain_text(self) -> Builder` — Sets format to `PlainText`. No-arg, infallible.
- `field_to_label(self, source: S, target: T) -> Result<Builder, Error>` — Adds a field mapping. Validates target label name characters, checks for conflicts with static labels and `"level"`, checks for duplicate source fields.
- `exclude_unmapped_fields(self) -> Builder` — Enables exclusion. No-arg, infallible.

### Layer::on_event Changes

The `on_event` method gains three new behaviors:

1. **Field extraction**: After collecting event fields and span fields, iterate configured `field_mappings`. For each mapping with a matching source field in event/span fields or metadata, extract the value as a string, add to a `HashMap<String, String>` of dynamic labels, and remove the field from the collection.

2. **Format branching**:
   - `Json`: Existing `SerializedEvent` path, but with mapped fields removed and optionally unmapped fields excluded.
   - `PlainText`: Extract `message` field from event. If `exclude_unmapped_fields` is false, append remaining fields as `key=value` pairs. Extra fields are always appended.

3. **LokiEvent construction**: Add `dynamic_labels` field to `LokiEvent`.

### BackgroundTask Queue Routing

Replace `queues: LevelMap<SendQueue>` with `queues: HashMap<String, SendQueue>`.

On receiving a `LokiEvent`:
1. Build the label string from static labels + level + dynamic labels (sorted for consistency).
2. Look up or insert a `SendQueue` with that label string.
3. Push event to queue.

`prepare_sending`, `should_send`, `on_send_result`, `drop_outstanding` iterate over `HashMap` values instead of `LevelMap` values.

### Label String Construction

Add a helper to `FormattedLabels` (or a standalone function) that takes the base formatted labels string, a level, and a `HashMap<String, String>` of dynamic labels, and produces the final Prometheus-format label string. Dynamic label names are sorted alphabetically for deterministic ordering.

### Error Type Extensions

Add new `ErrorInner` variants:
- `DuplicateFieldMapping(String)` — Duplicate source field name
- `FieldMappingConflictsWithLabel(String)` — Target label conflicts with static label
- `InvalidFieldMappingLabelCharacter(String, char)` — Target label contains invalid characters

## Complexity Tracking

No constitution violations. No complexity justification needed.
