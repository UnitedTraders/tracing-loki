<!--
Sync Impact Report
===================
- Version change: 1.0.0 → 1.1.0
- Modified principles:
  - V. Testing Discipline — replaced "gate behind #[ignore]/feature flag"
    with mandatory fake Loki server for integration tests; added
    explicit "no external services" requirement for cargo test
- Added sections: none
- Removed sections: none
- Templates requiring updates:
  - .specify/templates/plan-template.md — ✅ no updates needed
  - .specify/templates/spec-template.md — ✅ no updates needed
  - .specify/templates/tasks-template.md — ✅ no updates needed
  - .specify/templates/commands/*.md — N/A (directory empty)
- Follow-up TODOs: none
-->
# tracing-loki Constitution

## Core Principles

### I. Library-First API Design

- The public API MUST be minimal, discoverable, and hard to misuse.
- Every public type, trait, and function MUST have a `rustdoc` doc comment
  with at least one code example where behavior is non-obvious.
- Builder patterns MUST validate inputs at construction time, not at
  runtime, returning `Result` for fallible configuration.
- Breaking API changes MUST follow the Rust API evolution guidelines:
  seal internal traits, use `#[non_exhaustive]` on public enums/structs
  where future variants are expected.
- New public surface area MUST be justified; prefer fewer, composable
  primitives over many specialized helpers.

### II. Correctness & Safety

- `unsafe` code is NOT PERMITTED in this crate. If an operation cannot be
  expressed in safe Rust, the approach MUST be redesigned or delegated to
  an audited dependency.
- All error paths MUST propagate structured errors via the crate's `Error`
  type. Panics (`unwrap`, `expect`) are forbidden outside of test code and
  cases where the invariant is proven by construction (with an inline
  comment explaining why).
- Integer arithmetic that could overflow MUST use checked or saturating
  operations. Bit-shift operands MUST be bounds-checked.
- All user-supplied strings used in label names or HTTP headers MUST be
  validated against their respective protocol grammars before use.

### III. Async Discipline

- The library MUST be runtime-agnostic at the `Layer` boundary: the
  synchronous `Layer` implementation MUST NOT call `.await` or spawn
  tasks. All async work is confined to `BackgroundTask`.
- Channel-based communication between `Layer` and `BackgroundTask` MUST
  use bounded channels with a documented capacity. Backpressure behavior
  (drop on full) MUST be documented in the public API.
- Blocking operations (DNS resolution, TLS handshake via synchronous
  paths) MUST NOT occur on the subscriber's calling thread.
- `BackgroundTask` MUST support graceful shutdown: drain pending events,
  flush final batch, then terminate.

### IV. Protocol Fidelity

- Payloads sent to Loki MUST conform to the Loki push API protobuf
  schema (`logproto.proto`). Timestamps MUST be nanosecond-precision
  UTC.
- Protobuf encoding MUST use `prost`; payloads MUST be Snappy-compressed
  before transmission (matching Loki's expected `Content-Encoding`).
- HTTP transport MUST use the configurable TLS backend (`native-tls` or
  `rustls`) via feature flags and MUST NOT hard-code a single backend.
- Retry logic MUST implement exponential backoff with jitter, capped at
  a maximum interval, and MUST NOT retry on 4xx client errors (except
  429).

### V. Testing Discipline

- Every bug fix MUST include a regression test that fails before the fix
  and passes after.
- Public API contracts (builder validation, label formatting, error
  cases) MUST have unit tests.
- Integration tests MUST use a fake Loki HTTP server (e.g., an
  in-process `hyper`/`axum` listener) that accepts push requests,
  decodes Snappy-compressed protobuf payloads, and asserts protocol
  correctness (valid `PushRequest`, correct labels, nanosecond
  timestamps). Tests MUST NOT depend on an external Loki instance.
- `cargo test --all` MUST pass without any running external services.
- `cargo test --all` MUST pass on both stable and nightly Rust with zero
  warnings under `#![deny(warnings)]` in CI.

### VI. Performance & Resource Bounds

- The `Layer::on_event` path is latency-critical. It MUST NOT allocate
  unboundedly or perform I/O. Serialization MUST be predictable in cost
  relative to the number of fields.
- Batch sizes and channel capacities MUST be bounded by constants, not
  grow without limit.
- Memory usage of `SendQueue` and internal buffers MUST be proportional
  to the configured batch size, not to total event volume.
- Dependencies MUST be minimal. New dependencies MUST justify their
  addition by reducing code complexity or improving correctness beyond
  what a simple inline implementation provides.

### VII. Backward Compatibility

- The `compat-0-2-1` feature flag pattern MUST be maintained for
  forward-compatible migrations.
- Dependency version ranges MUST use minimum-version pinning compatible
  with downstream consumers (e.g., `>=0.11.10,<0.13.0` style).
- MSRV (Minimum Supported Rust Version) changes are breaking and MUST be
  documented in `CHANGELOG.md` and reflected in CI matrix updates.
- Removing or renaming public items MUST go through a deprecation cycle:
  mark `#[deprecated]` in release N, remove in release N+1 (major bump).

## Rust Standards

- Code MUST pass `cargo fmt -- --check` with default `rustfmt`
  configuration. No custom `rustfmt.toml` overrides.
- Code MUST pass `cargo clippy --all-targets --all-features` with no
  allowed lints suppressed at the crate level unless documented in this
  constitution.
- All public items MUST have `rustdoc` documentation. `#![deny(missing_docs)]`
  SHOULD be enabled at the crate root.
- Error types MUST implement `std::error::Error` and `Display` with
  actionable messages.
- Feature flags MUST be additive: enabling a feature MUST NOT remove
  functionality or break compilation of code using default features.

## Development Workflow

- All changes MUST be submitted as pull requests against `master`.
- CI (GitHub Actions) MUST pass before merge: formatting check (stable),
  build + test (stable + nightly), benchmarks.
- Commit messages MUST follow conventional commits format
  (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, `test:`, `perf:`).
- Version bumps MUST follow SemVer 2.0.0. The workspace crates
  (`tracing-loki` and `loki-api`) are versioned independently.
- `CHANGELOG.md` MUST be updated for every user-facing change before
  release.

## Governance

- This constitution supersedes all ad-hoc practices. Any PR reviewer
  MUST verify compliance with these principles.
- Amendments require: (1) a PR modifying this file, (2) review by at
  least one maintainer, (3) a migration plan if the amendment
  invalidates existing code.
- Complexity beyond what these principles prescribe MUST be justified
  in the relevant PR description.
- Use `CLAUDE.md` for runtime development guidance and build commands.

**Version**: 1.1.0 | **Ratified**: 2026-02-27 | **Last Amended**: 2026-02-27
