# Data Model: 004-otel-fields

**Date**: 2026-03-09

## Entities

### Layer (unchanged structurally)

The synchronous tracing layer. No new fields. OTel extraction is performed inline in `on_event` via conditional compilation.

### LokiEvent (unchanged)

OTel fields (`trace_id`, `span_id`, `span_name`) are injected into the serialized `message` string during `on_event`, not stored as separate fields on `LokiEvent`. When promoted to labels via `field_to_label`, they flow through the existing `dynamic_labels: HashMap<String, String>` field.

### Cargo.toml (modified)

| Section | Change | Notes |
|---------|--------|-------|
| `[dependencies]` | New optional deps | `opentelemetry = { version = "0.27", optional = true }`, `tracing-opentelemetry = { version = "0.28", optional = true }` |
| `[features]` | New feature | `opentelemetry = ["dep:opentelemetry", "dep:tracing-opentelemetry"]` |
| `[dev-dependencies]` | New test deps | `opentelemetry = "0.27"`, `opentelemetry_sdk = "0.27"`, `tracing-opentelemetry = "0.28"` |

### on_event flow (modified, conditional)

When `#[cfg(feature = "opentelemetry")]` is enabled, the following extraction occurs after span field collection and before dynamic label extraction:

```
1. Get current span reference (already computed for span_fields)
2. Access span.extensions().get::<OtelData>()
3. If OtelData is present:
   a. Extract span_id from builder.span_id → format as "{:016x}"
   b. Extract trace_id: try builder.trace_id first, then parent_cx.span().span_context().trace_id()
   c. Filter invalid (all-zero) IDs
   d. Extract span_name from span_ref.name()
4. If OtelData is NOT present but span exists (fallback):
   a. Extract span_id from span_ref.id().into_u64() → format as "{:016x}"
   b. Extract span_name from span_ref.name()
   c. trace_id is None (omitted)
5. Insert non-None values into span_fields map as String values
6. Continue to existing dynamic label extraction + serialization
```

## Data Flow

```
Event emitted inside OTel-instrumented span
    │
    ▼
on_event() collects span_fields from span extensions (existing)
    │
    ▼
[#[cfg(feature = "opentelemetry")]]
Extract OTel fields from span extensions → inject into span_fields map
    │
    ▼
Dynamic label extraction (existing) — OTel fields participate if mapped via field_to_label
    │
    ▼
Serialization (JSON or PlainText) — remaining OTel fields appear in entry line
    │
    ▼
LokiEvent { message, dynamic_labels } sent through channel (unchanged)
```

## Type Mapping

| OTel Concept | Rust Type | String Format | JSON Key | Plain Text |
|---|---|---|---|---|
| Trace ID | `opentelemetry::trace::TraceId` | 32-char lowercase hex | `"trace_id"` | `trace_id=<hex>` |
| Span ID | `opentelemetry::trace::SpanId` | 16-char lowercase hex | `"span_id"` | `span_id=<hex>` |
| Span Name | `&str` (from `span_ref.name()`) | As-is | `"span_name"` | `span_name=<name>` |
| Fallback Span ID | `u64` (from `Id::into_u64()`) | 16-char zero-padded hex | `"span_id"` | `span_id=<hex>` |
