# Implementation Plan: Flume Channel with Overflow Drop Policy and Configurable Backoff

**Branch**: `003-flume-channel` | **Date**: 2026-03-09 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-flume-channel/spec.md`

## Summary

Replace the tokio mpsc channel with a flume bounded channel for event transport between the synchronous Layer and async BackgroundTask. Add drop-on-overflow with observability (atomic counter + rate-limited warning), configurable channel capacity and backoff duration via the builder, and refactor BackgroundTask from a manual `Future` implementation to an `async fn` loop. See [research.md](research.md) for technology decisions.

## Technical Context

**Language/Version**: Rust 2021 edition, stable + nightly (CI matrix)
**Primary Dependencies**: flume 0.11 (new), reqwest 0.13, prost 0.14, snap 1.0, tracing 0.1, tracing-subscriber 0.3, tokio 1.x (time feature)
**Storage**: N/A
**Testing**: `cargo test --all` (unit + integration + doc tests), fake Loki server (axum 0.8)
**Target Platform**: Cross-platform library (Linux, macOS, Windows)
**Project Type**: Library
**Performance Goals**: `on_event` path must be non-blocking; channel send < 1us
**Constraints**: No `unsafe`, bounded memory proportional to channel capacity
**Scale/Scope**: 4 source files modified, ~150 lines changed, ~50 lines added in tests

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Library-First API Design | PASS | New builder methods have docs + examples. `channel_capacity()` validates at construction time. `BackgroundTaskFuture` type alias is minimal public surface. |
| II. Correctness & Safety | PASS | No unsafe. `AtomicU64` for drop counter. Errors propagated via `Error` type. `backoff_count` shl uses `checked_shl`. |
| III. Async Discipline | PASS | Layer remains synchronous (try_send). Bounded channel with documented drop policy. BackgroundTask supports graceful shutdown with flush. |
| IV. Protocol Fidelity | PASS | No changes to protobuf/snappy encoding or HTTP transport. |
| V. Testing Discipline | PASS | Integration tests use fake Loki server. New tests for overflow behavior. |
| VI. Performance & Resource Bounds | PASS | on_event uses try_send (non-blocking). Channel capacity bounded by config. flume justified by non-blocking semantics + runtime independence. |
| VII. Backward Compatibility | JUSTIFIED | `BackgroundTask` struct becomes private; replaced by `BackgroundTaskFuture` type alias. This is a breaking change per FR-012, justified by simplification. Users only need to change type annotations (if any) — `tokio::spawn(task)` pattern unchanged. |

## Project Structure

### Documentation (this feature)

```text
specs/003-flume-channel/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0: technology decisions
├── data-model.md        # Phase 1: entity changes
├── quickstart.md        # Phase 1: verification checklist
├── contracts/
│   └── public-api.md    # Phase 1: API contract changes
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # BackgroundTask refactor, channel swap, drop counter, type alias
├── builder.rs           # backoff() + channel_capacity() methods, updated return types
├── labels.rs            # Unchanged
└── no_subscriber.rs     # Unchanged

tests/
└── integration.rs       # New overflow/drop tests

Cargo.toml               # Add flume, update tokio features
```

**Structure Decision**: Single-project Rust library. No structural changes — only modifications to existing files and one new dependency.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| BackgroundTask type change (VII) | Manual Future impl is 100+ lines of complex poll logic; async fn is 40 lines | Keeping Future impl would preserve backward compat but miss the primary simplification goal; the type alias preserves the spawn ergonomics |
