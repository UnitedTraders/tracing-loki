# Research: 004-otel-fields

**Date**: 2026-03-09

## R1: OtelData API for trace_id and span_id extraction

**Decision**: Access `OtelData.builder.trace_id` and `OtelData.builder.span_id` from span extensions using the published `tracing-opentelemetry` API (v0.28+).

**Rationale**:
- `tracing_opentelemetry::OtelData` is stored as a span extension by the `OpenTelemetryLayer` during `on_new_span`.
- In published versions (0.28–0.32), `OtelData` has two public fields: `parent_cx: opentelemetry::Context` and `builder: SpanBuilder`.
- `builder.span_id` is `Option<SpanId>` — populated for all spans by `tracer.new_span_id()`.
- `builder.trace_id` is `Option<TraceId>` — populated only for root spans (no active parent). For child spans, the trace_id must be extracted from `parent_cx.span().span_context().trace_id()`.
- Access pattern in `on_event`:
  ```rust
  let extensions = span_ref.extensions();
  let otel_data = extensions.get::<tracing_opentelemetry::OtelData>();
  ```
- `TraceId` formats as 32-char lowercase hex; `SpanId` formats as 16-char lowercase hex (both implement `Display` and `LowerHex`).

**Alternatives considered**:
- The jraffeiner fork uses an unreleased API with `OtelData.trace_id()` and `OtelData.span_id()` methods. These methods don't exist in any published version (0.28–0.32). Rejected for compatibility.
- Walking span parents to find trace_id: rejected because `parent_cx` already contains the propagated trace_id for child spans.

## R2: Reliable trace_id extraction for child spans

**Decision**: For child spans where `builder.trace_id` is `None`, extract trace_id from `otel_data.parent_cx.span().span_context().trace_id()`. Filter out invalid (all-zero) trace IDs.

**Rationale**:
- `tracing-opentelemetry` only sets `builder.trace_id` for root spans that have no active parent context.
- For child spans, the trace_id is propagated through `parent_cx`, which is a `opentelemetry::Context` containing the parent `SpanContext`.
- `parent_cx.span().span_context().trace_id()` returns the propagated trace_id.
- Must check `trace_id != TraceId::INVALID` (all zeros) to avoid emitting invalid IDs.
- Similarly, check `span_id != SpanId::INVALID`.

**Alternatives considered**:
- Only extract trace_id from builder (root spans only): would miss trace_id for all child spans, making the feature useless for most use cases.
- Walk the tracing span tree to find the root: more complex, slower, and `parent_cx` already has the answer.

## R3: Fallback span_id from tracing-internal span ID

**Decision**: When the OTel feature is enabled but `OtelData` is not present in span extensions (no `tracing-opentelemetry` layer registered), fall back to the tracing-internal span ID formatted as a 16-character zero-padded lowercase hex string.

**Rationale**:
- FR-004 requires fallback `span_id` when OTel data is not available but a tracing span exists.
- `tracing::span::Id::into_u64()` returns the internal span ID as `u64`.
- Format as `format!("{:016x}", id.into_u64())` for consistency with the OTel hex format.
- No `trace_id` fallback exists (tracing has no concept of trace IDs), so `trace_id` is simply omitted.
- `span_name` is always available from `span_ref.name()` regardless of OTel.

**Alternatives considered**:
- No fallback: rejected because it provides no span correlation when OTel layer is missing but feature is enabled.
- Generate a random trace_id: rejected as misleading — it wouldn't correlate with any real trace.

## R4: Feature flag design

**Decision**: Use a single feature flag named `opentelemetry` that enables both `tracing-opentelemetry` and `opentelemetry` dependencies.

**Rationale**:
- Cargo feature flags are additive (constitution requirement). Enabling `opentelemetry` adds OTel extraction; disabling it compiles out all OTel code.
- Feature flag controls: (1) optional dependencies in `[dependencies]`, (2) `#[cfg(feature = "opentelemetry")]` blocks in `src/lib.rs`.
- The feature is NOT included in `default` features — users must opt in.
- Dependencies: `tracing-opentelemetry = { version = "0.28", optional = true }`, `opentelemetry = { version = "0.27", optional = true }`.
- Using a wide version range (0.28+) allows users to bring their own compatible version.

**Alternatives considered**:
- Two separate feature flags (one for each dep): over-engineered, the deps are always used together.
- Name `otel`: rejected in favor of the full name `opentelemetry` for clarity.

## R5: OTel field injection point in on_event

**Decision**: Extract OTel fields after span field collection but before the format-specific serialization branch (JSON vs PlainText). Inject as synthetic fields into the event's field map so they participate in the existing `field_to_label` extraction.

**Rationale**:
- OTel fields need to be eligible for `field_to_label` promotion, which operates on the combined event + span field maps.
- By injecting `trace_id`, `span_id`, and `span_name` into the `span_fields` map before the dynamic label extraction loop, they automatically participate in label promotion.
- Fields that are not mapped to labels remain in the entry line (JSON object or plain text key=value pairs).
- This avoids duplicating the label extraction logic for OTel fields.

**Alternatives considered**:
- Special-case OTel fields in the label extraction loop: more code, harder to maintain.
- Add OTel fields after serialization: would bypass `field_to_label` mechanism.
- Add OTel fields to `event_fields` instead of `span_fields`: either works, but `span_fields` is semantically more appropriate since OTel data comes from span context.

## R6: Version compatibility strategy

**Decision**: Target `opentelemetry >= 0.27` and `tracing-opentelemetry >= 0.28` as minimum versions, with no upper bound cap.

**Rationale**:
- Published versions 0.28–0.32 of `tracing-opentelemetry` all expose `OtelData` with `pub builder: SpanBuilder` and `pub parent_cx: Context`.
- `SpanBuilder.span_id` and `SpanBuilder.trace_id` are `Option<SpanId>` / `Option<TraceId>` across all these versions.
- Using `>= 0.27` for `opentelemetry` allows users to bring any compatible version.
- No upper bound avoids artificial breakage when new minor versions are released.
- If a future version of `tracing-opentelemetry` makes `OtelData` opaque, we can add version-conditional compilation at that time.

**Alternatives considered**:
- Pin to exact versions (e.g., `=0.32`): too restrictive, forces users to match exactly.
- Target the unreleased API with `OtelData.trace_id()` methods: not available in any published crate.

## R7: Integration test setup for OTel

**Decision**: Add `opentelemetry`, `opentelemetry_sdk`, and `tracing-opentelemetry` as dev-dependencies (always compiled for tests). Use `opentelemetry_sdk::trace::TracerProvider` with a no-op exporter to create instrumented spans in tests.

**Rationale**:
- Integration tests need a real `tracing-opentelemetry` layer to store `OtelData` in span extensions.
- `opentelemetry_sdk` provides `TracerProvider` and `SimpleSpanProcessor` for constructing a minimal OTel pipeline.
- A no-op/in-memory exporter is sufficient — we don't need to export traces, just populate `OtelData`.
- Test pattern: `TracerProvider::builder().build()` → `OpenTelemetryLayer::new(tracer)` → add to subscriber stack alongside the Loki layer.
- OTel-specific tests are gated behind `#[cfg(feature = "opentelemetry")]` to avoid compilation errors when the feature is off.

**Alternatives considered**:
- Mock `OtelData` manually by inserting it into span extensions: fragile, depends on internal structure.
- Use `opentelemetry_stdout` exporter: adds unnecessary dependency for test infrastructure.
