# Public API Contract: 004-otel-fields

**Date**: 2026-03-09

## New Feature Flag

```toml
# Cargo.toml [features] section
opentelemetry = ["dep:opentelemetry", "dep:tracing-opentelemetry"]
```

The `opentelemetry` feature is NOT included in `default`. Users must opt in:

```toml
[dependencies]
tracing-loki = { version = "0.2", features = ["opentelemetry"] }
```

## No New Public Types or Methods

This feature adds no new public types, methods, or builder methods. OTel fields integrate entirely through existing mechanisms:

- **Entry line inclusion**: OTel fields appear as regular fields in JSON (`"trace_id": "..."`) or plain text (`trace_id=...`) log entries.
- **Label promotion**: OTel fields are eligible for `field_to_label` mapping via the existing builder method:
  ```rust
  tracing_loki::builder()
      .field_to_label("trace_id", "trace_id")?
      .field_to_label("span_id", "span_id")?
  ```

## Behavioral Changes (feature enabled only)

When `feature = "opentelemetry"` is enabled:

### With tracing-opentelemetry layer registered

Events emitted inside instrumented spans will include up to three additional fields:

| Field | Value | Condition |
|-------|-------|-----------|
| `trace_id` | 32-char lowercase hex string | OtelData present and trace_id is valid (non-zero) |
| `span_id` | 16-char lowercase hex string | OtelData present and span_id is valid (non-zero) |
| `span_name` | Span name string | Any span exists |

### Without tracing-opentelemetry layer (fallback)

Events emitted inside tracing spans (without OTel) will include:

| Field | Value | Condition |
|-------|-------|-----------|
| `span_id` | 16-char zero-padded hex from `Id::into_u64()` | Any span exists |
| `span_name` | Span name string | Any span exists |
| `trace_id` | *omitted* | No OTel context available |

### Events outside any span

No `trace_id`, `span_id`, or `span_name` fields are added.

## No Behavioral Changes (feature disabled)

When `feature = "opentelemetry"` is not enabled (default):

- No `trace_id`, `span_id`, or `span_name` fields are added to any log entry.
- `opentelemetry` and `tracing-opentelemetry` are not compiled.
- All existing behavior is identical to current master.

## Field Interaction with field_to_label

OTel fields participate in the existing dynamic label extraction pipeline:

```rust
// Promotes trace_id to a Loki stream label, removes it from entry line
.field_to_label("trace_id", "trace_id")?

// Maps span_id to a different label name
.field_to_label("span_id", "otel_span_id")?
```

When `exclude_unmapped_fields` is enabled, unmapped OTel fields are excluded from the entry line (same behavior as any other unmapped field).

## Dependency Changes

### New optional dependencies (feature-gated)

```toml
opentelemetry = { version = "0.27", optional = true }
tracing-opentelemetry = { version = "0.28", optional = true }
```

### New dev-dependencies (for tests)

```toml
opentelemetry = "0.27"
opentelemetry_sdk = "0.27"
tracing-opentelemetry = "0.28"
```
