# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build --all          # Build all workspace crates
cargo test --all           # Run all tests (unit + doctests)
cargo test --all -- --nocapture  # Tests with stdout
cargo fmt -- --check       # Check formatting (CI enforced)
cargo bench                # Run benchmarks
```

CI runs on stable and nightly Rust: fmt check (stable only), build, test, bench.

## Architecture

**tracing-loki** is a `tracing` Layer that ships logs to Grafana Loki over HTTP. It uses a two-part design:

1. **`Layer`** (`src/lib.rs`) — implements `tracing_subscriber::Layer`, captures events synchronously, serializes them to JSON (including span fields), and sends `LokiEvent` structs through an mpsc channel (capacity 512).

2. **`BackgroundTask`** (`src/lib.rs`) — a `Future` that receives events from the channel, batches them into per-level `SendQueue`s, encodes via protobuf (`prost`), compresses with Snappy, and POSTs to `{loki_url}/loki/api/v1/push`. Handles retry with exponential backoff (500ms×2^n, capped at 600s). Uses `NoSubscriber` internally to prevent recursive tracing.

**`Builder`** (`src/builder.rs`) — fluent API for configuring labels, extra fields, and HTTP headers. Terminal methods: `build_url()` returns `(Layer, BackgroundTask)`, `build_controller_url()` adds a `BackgroundTaskController` for graceful shutdown.

## Workspace Layout

- **Root crate (`tracing-loki`)** — the main library
- **`loki-api/`** — generated protobuf types (`PushRequest`, `StreamAdapter`, `EntryAdapter`) for the Loki push API
- **`loki-api/generate/`** — code generator that produces `loki-api/src/logproto.rs` and `stats.rs` from `.proto` files (not published)

## Key Modules

- `src/labels.rs` — `FormattedLabels`: validates label names (`[A-Za-z_]` only), prevents duplicates, reserves "level" label
- `src/level_map.rs` — `LevelMap<T>`: fixed-size array indexed by tracing `Level` (5 levels)
- `src/log_support.rs` — strips `log.` prefixed fields from `tracing-log` bridge events during serialization
- `src/no_subscriber.rs` — `NoSubscriber` to avoid infinite recursion when the background task itself logs

## Features

- `default = ["compat-0-2-1", "native-tls"]`
- `native-tls` / `rustls` — mutually exclusive TLS backends for reqwest
- `compat-0-2-1` — forward compatibility flag (required)

## Active Technologies
- Rust 2021 edition, stable + nightly (CI matrix) + racing 0.1, tracing-subscriber 0.3, reqwest, prost, snap, loki-api (workspace) (001-mock-loki-integration-tests)

## Recent Changes
- 001-mock-loki-integration-tests: Added Rust 2021 edition, stable + nightly (CI matrix) + racing 0.1, tracing-subscriber 0.3, reqwest, prost, snap, loki-api (workspace)
