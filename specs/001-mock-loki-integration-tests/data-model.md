# Data Model: Mock Loki Integration Tests

**Date**: 2026-02-27
**Feature**: 001-mock-loki-integration-tests

## Entities

### FakeLokiServer

An in-process HTTP server mimicking the Loki push API.

**Attributes**:
- `addr`: `SocketAddr` — bound address (`127.0.0.1:<os-assigned-port>`)
- `requests`: `Arc<Mutex<Vec<PushRequest>>>` — captured decoded requests

**Behavior**:
- Binds to `127.0.0.1:0`
- Accepts `POST /loki/api/v1/push`
- Content-Type: expects `application/x-snappy`
- Decodes body: Snappy raw decompress -> `PushRequest::decode()`
- Appends decoded request to shared `requests` vec
- Returns HTTP 200

### PushRequest (from loki-api)

**Attributes**:
- `streams`: `Vec<StreamAdapter>` — one per unique label set (one per level)

### StreamAdapter (from loki-api)

**Attributes**:
- `labels`: `String` — Prometheus-format label string, e.g. `{level="info",host="mine"}`
- `entries`: `Vec<EntryAdapter>` — log entries for this stream
- `hash`: `u64` — always 0

### EntryAdapter (from loki-api)

**Attributes**:
- `timestamp`: `Option<prost_types::Timestamp>` — nanosecond precision
- `line`: `String` — JSON-serialized event body

### Entry Line JSON Structure

The `line` field contains a JSON object with these fields:
- `message`: The log message text
- Event-specific fields from `tracing` macros (e.g. `"key": "value"`)
- Extra fields from `Builder::extra_field()` (flattened)
- Span fields from active spans (flattened)
- `_spans`: Array of span names
- `_target`: Module target string
- `_module_path`: Optional full module path
- `_file`: Optional source file
- `_line`: Optional source line number

### Label Format

Labels follow Prometheus format: `{key1="value1",key2="value2"}`
- Keys: ASCII letters and underscore only `[A-Za-z_]`
- Values: Rust debug-formatted strings (double-quoted, escaped)
- "level" is always the last label, auto-appended
- Level values: "trace", "debug", "info", "warn", "error"

## Relationships

```
FakeLokiServer --captures--> PushRequest
PushRequest --contains--> StreamAdapter (1..N, one per level used)
StreamAdapter --contains--> EntryAdapter (1..N, one per event)
StreamAdapter --has--> labels (string, includes level + custom labels)
EntryAdapter --has--> line (JSON string with message + extra fields + span fields)
```
