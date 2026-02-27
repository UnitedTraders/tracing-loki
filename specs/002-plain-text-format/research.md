# Research: Plain Text Log Format with Field-to-Label Mapping

**Date**: 2026-02-27
**Feature**: 002-plain-text-format

## Decision 1: Message Extraction for Plain Text Mode

**Decision**: Extract the `message` field from tracing events by implementing a custom `Visit` that captures only the `message` field (tracing stores the format string argument as a field named `"message"`). Use `record_str` on the `"message"` field name to get the plain text string.

**Rationale**: The current `on_event` method serializes the entire event into a `SerializedEvent` struct via serde_json. For plain text mode, we need to extract just the message. The tracing crate stores the format string as a field named `"message"` which is visited via the `Visit` trait. A lightweight visitor that captures only this field avoids the overhead of full JSON serialization.

**Alternatives considered**:
- Parsing the JSON output to extract the `"message"` key: Wasteful and fragile.
- Using `fmt::Display` on the event: Not directly available; tracing events don't implement Display.

## Decision 2: Dynamic Label Architecture (SendQueue Multiplexing)

**Decision**: Change the `LokiEvent` to carry extracted dynamic label values alongside the message. In `BackgroundTask`, replace the fixed `LevelMap<SendQueue>` with a `HashMap<String, SendQueue>` keyed by the full encoded label string (static labels + level + dynamic label values). SendQueues are created lazily when a new label combination is encountered.

**Rationale**: Currently, `BackgroundTask` uses `LevelMap<SendQueue>` — 5 fixed queues, one per level, each with a pre-formatted label string. Dynamic labels mean the label set varies per-event based on field values. The label string is the natural key since Loki defines streams by their label set. Lazy creation avoids pre-allocating for unknown combinations.

**Alternatives considered**:
- Two-level map (`LevelMap<HashMap<DynamicLabels, SendQueue>>`): More complex, same result since the encoded label string already embeds the level.
- Moving label resolution to `Layer::on_event` and embedding the full label string in `LokiEvent::message`: Couples label formatting with the synchronous path; better to keep it in BackgroundTask.

## Decision 3: Field-to-Label Extraction in Layer::on_event

**Decision**: In `Layer::on_event`, after collecting event fields and span fields, iterate over the configured field mappings. For each mapping, check if the source field name exists in the event fields or span fields. If found, extract the value (converted to string), remove it from the fields collection, and attach it to `LokiEvent` as a `HashMap<String, String>` of dynamic label name → value pairs.

**Rationale**: Field extraction must happen in `on_event` because that's where we have access to the event's fields via the `Visit` trait. The extracted labels travel through the channel to BackgroundTask, which uses them to determine the correct SendQueue. Removing matched fields from the fields collection ensures they don't appear in the entry line (FR-008).

**Alternatives considered**:
- Extracting labels in BackgroundTask by parsing the JSON message: Would require JSON parsing on the hot path in BackgroundTask; also impossible for plain text mode.
- Using a separate channel for labels: Unnecessary complexity; adding a field to `LokiEvent` is simpler.

## Decision 4: Log Line Format Configuration

**Decision**: Add a `LogLineFormat` enum (`Json`, `PlainText`) to the Builder. Store it in the `Layer`. In `on_event`, branch on the format: JSON mode uses the existing `SerializedEvent` path; plain text mode extracts the message field and optionally appends unmapped fields as `key=value` pairs.

**Rationale**: The format affects serialization in `Layer::on_event`, which is the synchronous path. The enum is cheap to branch on. Keeping both paths in the same `on_event` method avoids duplicating the span field collection logic.

**Alternatives considered**:
- Trait object for format strategy: Over-engineered for two variants.
- Separate `Layer` types for each format: Breaks the single-Builder-builds-one-Layer pattern.

## Decision 5: Unmapped Field Exclusion

**Decision**: Add a `bool` flag `exclude_unmapped_fields` to the Builder (default `false`). Pass it to `Layer`. In `on_event`, after extracting mapped fields, if the flag is true, discard remaining event/span fields before serialization. Extra fields (from `Builder::extra_field()`) are always included.

**Rationale**: Simple boolean flag matches the spec requirement (FR-009). The exclusion happens at serialization time in `on_event`, before the message string is constructed.

**Alternatives considered**:
- Allow per-field include/exclude configuration: Exceeds spec scope; the mapping mechanism already provides field-level control.

## Decision 6: Plain Text Format with Unmapped Fields

**Decision**: When plain text format is selected and unmapped field exclusion is disabled, format the entry line as: `{message} {key1}={value1} {key2}={value2}`. Values containing spaces or special characters are quoted with double quotes (`key="value with spaces"`). Field order follows insertion order (event fields first, then span fields).

**Rationale**: This `key=value` format is widely used in structured logging (e.g., logfmt) and is parseable by Loki's `logfmt` pipeline stage. It balances human readability with machine parseability.

**Alternatives considered**:
- Logfmt strict format (`key=value` with no message prefix): Loses the message text or requires a special `msg=` key.
- Space-separated without quotes: Ambiguous for values containing spaces.

## Decision 7: Metadata Field Mapping to Labels

**Decision**: Metadata fields (`_target`, `_module_path`, `_file`, `_line`, `_spans`) are eligible for field-to-label mapping using the same `field_to_label` Builder method. The source field name includes the underscore prefix (e.g., `"_target"`). In plain text mode, unmapped metadata fields are excluded by default. In JSON mode, they remain in the entry line for backward compatibility (FR-015).

**Rationale**: Per FR-014, metadata fields should be mappable like any other field. Using the same mechanism keeps the API consistent. The underscore prefix in the source name matches the JSON key name users already see, avoiding confusion.

**Alternatives considered**:
- Separate `metadata_to_label` method: Unnecessary API surface; the general mapping works fine.
- Stripping the underscore prefix: Would create inconsistency between the field name users see in JSON output and the mapping source name.

## Decision 8: LokiEvent Structure Changes

**Decision**: Extend `LokiEvent` with a `dynamic_labels: HashMap<String, String>` field carrying the extracted label key-value pairs. The `message` field continues to hold the serialized line (JSON or plain text, depending on format). The BackgroundTask uses `dynamic_labels` + level + static labels to compute the stream key.

**Rationale**: The channel between Layer and BackgroundTask carries `LokiEvent`. Adding dynamic labels here is the minimal change needed. The message is already serialized by the time it reaches BackgroundTask, which is correct — BackgroundTask just needs to know which stream to assign it to.

**Alternatives considered**:
- Serializing dynamic labels into the message and parsing them out in BackgroundTask: Fragile and wasteful.
- Sending raw event data through the channel and doing all processing in BackgroundTask: Would require the event data to be `Send`, which tracing events are not (they borrow subscriber state).
