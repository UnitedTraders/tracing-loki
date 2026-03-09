# Implementation Plan: OpenTelemetry Field Extraction

**Branch**: `004-otel-fields` | **Date**: 2026-03-09 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/004-otel-fields/spec.md`

## Summary

Add optional extraction of OpenTelemetry fields (`trace_id`, `span_id`, `span_name`) from span extensions when a `tracing-opentelemetry` layer is present in the subscriber stack. Fields are included in both JSON and plain text log entry formats and are eligible for promotion to Loki stream labels via the existing `field_to_label` builder method. The `opentelemetry` and `tracing-opentelemetry` dependencies are gated behind an optional Cargo feature flag. See [research.md](research.md) for technology decisions.

## Technical Context

**Language/Version**: Rust 2021 edition, stable + nightly (CI matrix)
**Primary Dependencies**: tracing-opentelemetry 0.28+ (optional), opentelemetry 0.27+ (optional), tracing 0.1, tracing-subscriber 0.3, reqwest 0.13, prost 0.14
**Storage**: N/A
**Testing**: `cargo test --all` (unit + integration + doc tests), fake Loki server (axum 0.8). OTel integration tests require `tracing-opentelemetry` + `opentelemetry` + `opentelemetry_sdk` as dev-dependencies.
**Target Platform**: Cross-platform library (Linux, macOS, Windows)
**Project Type**: Library
**Performance Goals**: OTel extraction adds at most one `extensions().get::<OtelData>()` lookup + two `format!` calls per event in the `on_event` path
**Constraints**: No `unsafe`, feature flag MUST be additive, existing behavior unchanged when feature is disabled
**Scale/Scope**: 2 source files modified (`src/lib.rs`, `Cargo.toml`), ~60 lines added in lib.rs, ~10 new integration tests

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Library-First API Design | PASS | No new public types or builder methods. OTel fields integrate via existing `field_to_label` mechanism. Feature flag name follows Cargo conventions. |
| II. Correctness & Safety | PASS | No unsafe. Fallback to tracing-internal span ID when OtelData is absent. Graceful degradation: `extensions().get::<OtelData>()` returns `None` when no OTel layer is registered. |
| III. Async Discipline | PASS | OTel extraction is synchronous (in `on_event`). No new async work. |
| IV. Protocol Fidelity | PASS | No changes to protobuf/snappy encoding or HTTP transport. OTel fields are simply additional key-value pairs in the log entry line or stream labels. |
| V. Testing Discipline | PASS | Integration tests use fake Loki server with `tracing-opentelemetry` + `opentelemetry_sdk` for OTel span setup. Tests verify both OTel-present and OTel-absent scenarios. |
| VI. Performance & Resource Bounds | PASS | One `extensions()` lookup per event (same cost as existing span field extraction). Two `format!` calls for hex strings. No unbounded allocation. |
| VII. Backward Compatibility | PASS | Feature is additive: disabled by default. When disabled, no code changes are compiled. When enabled but no OTel layer is registered, behavior is identical to disabled. No existing public API changes. |

## Project Structure

### Documentation (this feature)

```text
specs/004-otel-fields/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0: technology decisions
├── data-model.md        # Phase 1: entity changes
├── quickstart.md        # Phase 1: verification checklist
├── checklists/
│   └── requirements.md  # Specification quality checklist
├── contracts/
│   └── public-api.md    # Phase 1: API contract (feature flag only)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # OTel field extraction in on_event (behind #[cfg(feature)])
├── builder.rs           # Unchanged
├── labels.rs            # Unchanged
└── no_subscriber.rs     # Unchanged

tests/
└── integration.rs       # New OTel integration tests (behind #[cfg(feature)])

Cargo.toml               # Add optional opentelemetry + tracing-opentelemetry deps, new feature flag
```

**Structure Decision**: Single-project Rust library. No structural changes — only conditional compilation additions to `src/lib.rs`, new dev-dependencies, and a new feature flag in `Cargo.toml`.

## Complexity Tracking

> No constitution violations. Feature is additive with zero breaking changes.
