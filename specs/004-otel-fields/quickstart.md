# Quickstart: 004-otel-fields

**Date**: 2026-03-09

## Prerequisites

- Rust 2021 edition (stable or nightly)
- Working `cargo build --all` on current master
- Working `cargo test --all` (39 integration + unit + doc tests pass)

## Key Files to Modify

1. **`Cargo.toml`** — Add optional `opentelemetry` + `tracing-opentelemetry` deps, new `opentelemetry` feature flag, add `opentelemetry` + `opentelemetry_sdk` + `tracing-opentelemetry` as dev-dependencies
2. **`src/lib.rs`** — Add conditional OTel field extraction in `on_event` behind `#[cfg(feature = "opentelemetry")]`
3. **`tests/integration.rs`** — Add OTel integration tests behind `#[cfg(feature = "opentelemetry")]`

## Build & Test

```bash
# Without OTel feature (default — must pass unchanged)
cargo build --all                           # Verify compilation
cargo test --all                            # All existing tests pass

# With OTel feature
cargo build --all --features opentelemetry  # Verify OTel compilation
cargo test --all --features opentelemetry   # All tests including OTel tests

# Quality checks
cargo clippy --all-targets --all-features   # Zero warnings
cargo fmt -- --check                        # Formatting check
```

## Verification Checklist

### Feature flag
- [ ] `opentelemetry` feature defined in `[features]` section
- [ ] `opentelemetry` feature NOT in `default` features
- [ ] `opentelemetry` and `tracing-opentelemetry` are optional deps gated by feature
- [ ] `cargo build --all` succeeds without the feature (no OTel deps compiled)
- [ ] `cargo build --all --features opentelemetry` succeeds with the feature

### OTel field extraction
- [ ] `#[cfg(feature = "opentelemetry")]` guards all OTel-specific code in `src/lib.rs`
- [ ] `trace_id` extracted from `OtelData.builder.trace_id` (root) or `parent_cx` (child)
- [ ] `span_id` extracted from `OtelData.builder.span_id`
- [ ] `span_name` extracted from `span_ref.name()`
- [ ] Invalid (all-zero) trace_id and span_id are filtered out
- [ ] Fallback: when OtelData absent, `span_id` from `Id::into_u64()` as 16-char hex
- [ ] Fallback: when OtelData absent, `trace_id` is omitted
- [ ] Fields injected into span_fields map before dynamic label extraction
- [ ] Events outside spans have no OTel fields

### Format support
- [ ] JSON format: OTel fields appear as top-level keys (`"trace_id"`, `"span_id"`, `"span_name"`)
- [ ] Plain text format: OTel fields appear as `key=value` pairs
- [ ] `field_to_label("trace_id", "trace_id")` promotes trace_id to Loki stream label
- [ ] `exclude_unmapped_fields` excludes unmapped OTel fields

### Tests
- [ ] All 39 existing integration tests pass without OTel feature
- [ ] All existing tests pass WITH OTel feature enabled
- [ ] New tests: OTel fields in JSON format with tracing-opentelemetry layer
- [ ] New tests: OTel fields in plain text format
- [ ] New tests: field_to_label promotion for OTel fields
- [ ] New tests: events outside spans have no OTel fields
- [ ] New tests: fallback span_id without tracing-opentelemetry layer
- [ ] New tests: exclude_unmapped_fields with OTel fields
- [ ] `cargo clippy --all-targets --all-features` zero warnings
- [ ] `cargo fmt -- --check` passes
