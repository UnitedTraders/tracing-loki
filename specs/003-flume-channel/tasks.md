# Tasks: Flume Channel with Overflow Drop Policy and Configurable Backoff

**Input**: Design documents from `/specs/003-flume-channel/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/public-api.md

**Tests**: Included — spec requires integration tests for overflow/drop behavior (SC-003, SC-006).

**Organization**: Tasks are grouped by implementation phase. US1 and US2 share foundational channel work; US4 (async refactor) is a prerequisite for all stories since it restructures the BackgroundTask. Phases are ordered by compilation dependency, not purely by story priority.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: Add flume dependency and update tokio features

- [ ] T001 Add `flume = "0.11"` to `[dependencies]` in Cargo.toml and remove `sync` from tokio features (keep `time` if not already present via other paths)

---

## Phase 2: Core Refactor (US4 + Foundation for US1/US2/US3)

**Purpose**: Swap mpsc → flume, add BackgroundTaskFuture type alias, add builder fields, refactor BackgroundTask to async fn. These changes are tightly coupled — Rust requires them as a single compilable unit.

**Goal**: After this phase, the library compiles with flume, the BackgroundTask is an async fn loop, and the builder accepts backoff + channel_capacity.

- [ ] T002 [US4] Add `BackgroundTaskFuture` type alias and `ZeroChannelCapacity` error variant in src/lib.rs: add `pub type BackgroundTaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>`, add `ZeroChannelCapacity` to `ErrorInner` enum with Display impl, replace `use tokio::sync::mpsc` with `use flume::{Sender, Receiver}`, update `event_channel()` to accept `cap: usize` and use `flume::bounded(cap)`
- [ ] T003 [US4] Update Layer, BackgroundTaskController, and shutdown to use flume types in src/lib.rs: change `Layer.sender` to `flume::Sender<Option<LokiEvent>>`, change `BackgroundTaskController.sender` to `flume::Sender<Option<LokiEvent>>`, update `shutdown()` to use `self.sender.send_async(None).await`
- [ ] T004 [US4] Add `backoff` and `channel_capacity` fields to Builder with methods in src/builder.rs: add `backoff: Duration` (default 500ms) and `channel_capacity: usize` (default 512) fields, add `pub fn backoff(self, backoff: Duration) -> Builder` method with doc example, add `pub fn channel_capacity(self, capacity: usize) -> Result<Builder, Error>` method with zero-validation and doc example, update `builder()` constructor with defaults
- [ ] T005 [US4] Refactor BackgroundTask struct to remove manual Future fields in src/lib.rs: remove `backoff: Option<Pin<Box<tokio::time::Sleep>>>`, `quitting: bool`, `send_task: Option<Pin<Box<...>>>` fields, add `backoff: Duration` field, update `BackgroundTask::new()` to accept `backoff: Duration` parameter, make struct private (remove `pub`)
- [ ] T006 [US4] Implement `async fn start(mut self)` on BackgroundTask in src/lib.rs: replace the `impl Future for BackgroundTask` block with an `async fn start(mut self)` method containing: outer loop with `recv_async().await` to wait for events, inner `try_recv()` loop to drain channel, encode+send batch to Loki with `.await`, handle success (reset backoff_count, clear sent) and error (exponential backoff, drop outstanding after 30s threshold), `tokio::time::sleep(backoff).await` between cycles, return on `None` (shutdown signal) after flushing pending queues
- [ ] T007 [US4] Update build_url and build_controller_url return types in src/builder.rs: change `build_url` return to `Result<(Layer, BackgroundTaskFuture), Error>`, change `build_controller_url` return to `Result<(Layer, BackgroundTaskController, BackgroundTaskFuture), Error>`, pass `self.channel_capacity` to `event_channel()`, pass `self.backoff` to `BackgroundTask::new()`, wrap `BackgroundTask::new(...)?.start()` in `Box::pin()`
- [ ] T008 [US4] Update `layer()` convenience function return type in src/lib.rs: change return to `Result<(Layer, BackgroundTaskFuture), Error>`
- [ ] T009 [US4] Update doc examples and doctests in src/lib.rs: update crate-level doc example to use `BackgroundTaskFuture` type annotation on `tokio::spawn`, update `layer()` function doc example, update README code examples if they reference `BackgroundTask` type
- [ ] T010 [US4] Remove unused imports in src/lib.rs: remove `std::task::{Context, Poll}`, remove `tracing::instrument::WithSubscriber` if no longer needed, remove `tokio::sync::mpsc`, verify `use no_subscriber::NoSubscriber` is still needed (keep if used in error logging within async fn)

**Checkpoint**: Library compiles with flume. All existing tests should pass (with possible type annotation updates in test code). BackgroundTask is an async fn. Builder accepts backoff + channel_capacity.

---

## Phase 3: User Story 1 - Bounded Channel with Overflow Drop (Priority: P1)

**Goal**: Add drop counter and rate-limited warning when events are dropped due to channel overflow.

**Independent Test**: Emit more events than channel capacity without consuming, verify drop counter increments and warning is logged.

### Implementation for User Story 1

- [ ] T011 [US1] Add `dropped_count: Arc<AtomicU64>` and `last_drop_warning: Option<Instant>` fields to Layer struct in src/lib.rs, initialize in builder's `build_url` and `build_controller_url`
- [ ] T012 [US1] Update `on_event` in src/lib.rs: on `try_send` returning `Err(TrySendError::Full(_))`, increment `self.dropped_count` with `fetch_add(1, Relaxed)`, check `last_drop_warning` elapsed time and emit `tracing::warn!("tracing-loki: event dropped, channel full (total dropped: {})", count)` if rate limit exceeded, update `last_drop_warning` timestamp
- [ ] T013 [US1] Add `pub fn dropped_count(&self) -> u64` method on Layer in src/lib.rs with doc comment
- [ ] T014 [US1] Integration test `test_overflow_drop_counter` in tests/integration.rs: build layer with `.channel_capacity(2)`, do NOT spawn background task, emit 10 events, assert `layer.dropped_count() >= 8`
- [ ] T015 [US1] Integration test `test_normal_flow_no_drops` in tests/integration.rs: build layer with default capacity, spawn task, emit 5 events, shutdown, assert `layer.dropped_count() == 0` and all events arrive at fake Loki

**Checkpoint**: Drop observability works. Counter increments on overflow, warning is logged rate-limited.

---

## Phase 4: User Story 2 - Configurable Channel Capacity (Priority: P1)

**Goal**: Verify channel_capacity builder method works end-to-end with overflow behavior.

**Independent Test**: Build with custom capacity, verify the channel respects that limit.

### Implementation for User Story 2

- [ ] T016 [US2] Integration test `test_custom_channel_capacity` in tests/integration.rs: build layer with `.channel_capacity(1024)`, spawn task, emit 500 events, shutdown, verify all arrive (no drops with large capacity)
- [ ] T017 [US2] Integration test `test_zero_channel_capacity_rejected` in tests/integration.rs: assert `builder.channel_capacity(0).is_err()`
- [ ] T018 [US2] Integration test `test_default_channel_capacity` in tests/integration.rs: build layer with no explicit capacity, verify default 512 is used (emit 500 events with background task running, all should arrive)

**Checkpoint**: Channel capacity is configurable and validated. Default matches previous behavior.

---

## Phase 5: User Story 3 - Configurable Background Task Backoff (Priority: P2)

**Goal**: Verify backoff builder method is accepted and used by the background task.

**Independent Test**: Build with custom backoff, verify the background task uses it.

### Implementation for User Story 3

- [ ] T019 [US3] Integration test `test_custom_backoff` in tests/integration.rs: build layer with `.backoff(Duration::from_millis(10))`, emit events, shutdown, verify events arrive (shorter backoff means faster delivery)
- [ ] T020 [US3] Integration test `test_default_backoff` in tests/integration.rs: build layer with no explicit backoff, emit events, shutdown, verify events arrive (default 500ms backoff)

**Checkpoint**: Backoff is configurable via builder. Default matches previous behavior.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, formatting, and cleanup

- [ ] T021 Run `cargo test --all` and verify all existing + new tests pass
- [ ] T022 Run `cargo clippy --all-targets --all-features` and fix any warnings
- [ ] T023 Run `cargo fmt -- --check` and fix any formatting issues
- [ ] T024 Review and update README.md if examples reference BackgroundTask type directly

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Core Refactor (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Phase 2 completion
- **US2 (Phase 4)**: Depends on Phase 2 completion (can run parallel with US1)
- **US3 (Phase 5)**: Depends on Phase 2 completion (can run parallel with US1/US2)
- **Polish (Phase 6)**: Depends on all user story phases complete

### User Story Dependencies

- **US4 (P2)**: Implemented in Phase 2 as foundation — must complete first
- **US1 (P1)**: Can start after Phase 2 — no dependencies on US2/US3
- **US2 (P1)**: Can start after Phase 2 — no dependencies on US1/US3
- **US3 (P2)**: Can start after Phase 2 — no dependencies on US1/US2

### Within Phase 2 (Core Refactor)

Tasks T002-T010 must be executed sequentially — each builds on previous changes to src/lib.rs and src/builder.rs. The code will not compile until T010 is complete.

### Parallel Opportunities

- T014 and T015 (US1 tests) can run in parallel
- T016, T017, T018 (US2 tests) can run in parallel
- T019 and T020 (US3 tests) can run in parallel
- Phase 3, 4, 5 can run in parallel after Phase 2 completes

---

## Parallel Example: After Phase 2

```
# All user story phases can start simultaneously:
Phase 3 (US1): T011 → T012 → T013 → T014 [P] T015
Phase 4 (US2): T016 [P] T017 [P] T018
Phase 5 (US3): T019 [P] T020
```

---

## Implementation Strategy

### MVP First (Phase 1 + Phase 2 + Phase 3)

1. Complete Phase 1: Add flume dependency
2. Complete Phase 2: Core refactor (channel swap + async fn + builder fields)
3. Complete Phase 3: US1 — overflow drop observability
4. **STOP and VALIDATE**: Run full test suite, verify backward compatibility
5. This delivers the core value: non-blocking channel with drop observability

### Incremental Delivery

1. Phase 1 + Phase 2 → Code compiles with flume, BackgroundTask simplified
2. Add Phase 3 (US1) → Drop counter + warning, overflow tests pass
3. Add Phase 4 (US2) → Channel capacity configuration verified
4. Add Phase 5 (US3) → Backoff configuration verified
5. Phase 6 → Polish, clippy, fmt, README

---

## Notes

- Phase 2 is the critical path — it contains the largest code change and must be done atomically for Rust compilation
- US4 is implemented as part of Phase 2 (foundation) even though it's P2 priority, because the async refactor is a prerequisite for the simplified channel integration
- The existing `try_send` in `on_event` (line 523 of current src/lib.rs) means the non-blocking behavior already exists — flume's `try_send` preserves this
- The TODO comment at line 522 ("Anything useful to do when the capacity has been reached?") is directly addressed by US1 (drop counter + warning)
