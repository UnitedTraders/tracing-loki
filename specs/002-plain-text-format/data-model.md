# Data Model: Plain Text Log Format with Field-to-Label Mapping

**Date**: 2026-02-27
**Feature**: 002-plain-text-format

## Entities

### LogLineFormat

A configuration value that determines how the entry `line` is serialized.

**Variants**:
- `Json` — Current behavior. Entry line is a JSON object containing event fields, extra fields, span fields, and metadata (`_target`, `_module_path`, `_file`, `_line`, `_spans`).
- `PlainText` — Entry line contains only the message text. Optionally, unmapped fields are appended as `key=value` pairs.

**Default**: `Json` (backward compatible).

**Lifecycle**: Set once at Builder configuration time. Immutable after `build_url` / `build_controller_url`.

### FieldMapping

Associates a tracing field name (source) with a Loki label name (target).

**Attributes**:
- `source_field: String` — The tracing field name to match (e.g., `"service"`, `"_target"`). Must be unique across all mappings.
- `target_label: String` — The Loki label name to produce (e.g., `"service"`, `"target"`). Must conform to `[A-Za-z_]+`. Must not conflict with static labels or `"level"`.

**Validation rules** (at Builder configuration time):
- `target_label` characters must match `[A-Za-z_]` (same as static labels).
- `target_label` must not equal `"level"` (reserved).
- `target_label` must not duplicate any existing static label name.
- `source_field` must not duplicate any other mapping's source field.

**Lifecycle**: Added incrementally via Builder. Immutable after build. Stored in `Layer` for use during `on_event`.

### LokiEvent (modified)

The message passed through the mpsc channel from `Layer` to `BackgroundTask`.

**Attributes**:
- `trigger_send: bool` — Whether this event should trigger a send (existing).
- `timestamp: SystemTime` — Event timestamp (existing).
- `level: Level` — Tracing level (existing).
- `message: String` — Serialized entry line in the configured format (existing, content changes).
- `dynamic_labels: HashMap<String, String>` — **NEW**. Extracted field-to-label values for this event. Key is the target label name, value is the field's string representation.

### SendQueue Keying (modified)

**Current**: `LevelMap<SendQueue>` — 5 fixed queues, one per level. Each has a static `encoded_labels: String`.

**New**: `HashMap<String, SendQueue>` — Keyed by the full encoded label string. Created lazily when a new label combination is encountered. The key is computed by combining static labels + level + dynamic labels into a Prometheus-format string.

### Builder (modified)

**New fields**:
- `log_format: LogLineFormat` — Default `Json`.
- `field_mappings: Vec<FieldMapping>` — Default empty. Checked for uniqueness on source and target.
- `exclude_unmapped_fields: bool` — Default `false`.

### Layer (modified)

**New fields**:
- `log_format: LogLineFormat` — Determines serialization path in `on_event`.
- `field_mappings: Vec<FieldMapping>` — Used to extract dynamic labels and filter fields.
- `exclude_unmapped_fields: bool` — Controls whether non-mapped fields appear in entry line.

## Relationships

```
Builder --(builds)--> Layer + BackgroundTask
Builder.field_mappings --(stored in)--> Layer.field_mappings
Layer.on_event --(extracts dynamic labels using)--> FieldMapping
Layer.on_event --(sends)--> LokiEvent (with dynamic_labels)
BackgroundTask --(routes events using)--> dynamic_labels + level + static labels → SendQueue key
SendQueue --(produces)--> StreamAdapter (with computed label string)
```

## Field Flow Through the System

1. **Builder time**: User configures `log_format`, `field_mappings`, `exclude_unmapped_fields`.
2. **Layer::on_event** (synchronous, on subscriber thread):
   - Collect event fields and span fields (existing logic).
   - For each `FieldMapping`, check if `source_field` exists in event fields, span fields, or metadata. If found, extract value as string, add to `dynamic_labels`, remove from fields collection.
   - Serialize remaining fields according to `log_format`:
     - `Json`: Use existing `SerializedEvent` (minus extracted fields).
     - `PlainText`: Extract `message` field; optionally append unmapped fields as `key=value`.
   - If `exclude_unmapped_fields`, drop non-mapped, non-extra fields before serialization.
   - Send `LokiEvent { ..., message, dynamic_labels }` through channel.
3. **BackgroundTask::poll** (async):
   - Receive `LokiEvent`.
   - Compute stream key: format static labels + level + `dynamic_labels` into Prometheus label string.
   - Look up or create `SendQueue` for that key.
   - Push event to queue.
   - On send: collect all non-empty queues into `PushRequest.streams`.
