# Data Model: 003-flume-channel

**Date**: 2026-03-09

## Entities

### Layer (modified)

The synchronous tracing layer that captures events and sends them through the channel.

| Field                  | Type                                      | Change    | Notes                                    |
|------------------------|-------------------------------------------|-----------|------------------------------------------|
| sender                 | `flume::Sender<Option<LokiEvent>>`        | Modified  | Was `mpsc::Sender<Option<LokiEvent>>`    |
| extra_fields           | `HashMap<String, String>`                 | Unchanged |                                          |
| log_format             | `LogLineFormat`                           | Unchanged |                                          |
| field_mappings         | `Vec<FieldMapping>`                       | Unchanged |                                          |
| exclude_unmapped_fields| `bool`                                    | Unchanged |                                          |
| dropped_count          | `Arc<AtomicU64>`                          | New       | Monotonic counter of dropped events      |
| last_drop_warning      | `Option<Instant>`                         | New       | Rate-limits warning logs on drop         |

### BackgroundTask (modified, becomes private)

The async task that batches and sends events to Loki.

| Field          | Type                                        | Change    | Notes                                     |
|----------------|---------------------------------------------|-----------|-------------------------------------------|
| loki_url       | `Url`                                       | Unchanged |                                           |
| receiver       | `flume::Receiver<Option<LokiEvent>>`        | Modified  | Was `mpsc::Receiver<Option<LokiEvent>>`   |
| labels         | `FormattedLabels`                           | Unchanged |                                           |
| queues         | `HashMap<String, SendQueue>`                | Unchanged |                                           |
| buffer         | `Buffer`                                    | Unchanged |                                           |
| http_client    | `reqwest::Client`                           | Unchanged |                                           |
| backoff        | `Duration`                                  | New       | Configurable base backoff interval        |
| backoff_count  | `u32`                                       | Unchanged |                                           |

Removed fields (no longer needed with async fn):
- `backoff: Option<Pin<Box<tokio::time::Sleep>>>` — replaced by `tokio::time::sleep().await`
- `quitting: bool` — handled by loop control flow
- `send_task: Option<Pin<Box<dyn Future<...>>>>` — replaced by inline `.await`

### BackgroundTaskController (modified)

| Field  | Type                                   | Change   | Notes                                  |
|--------|----------------------------------------|----------|----------------------------------------|
| sender | `flume::Sender<Option<LokiEvent>>`     | Modified | Was `mpsc::Sender<Option<LokiEvent>>`  |

### Builder (modified)

| Field          | Type       | Change    | Notes                              |
|----------------|------------|-----------|-------------------------------------|
| labels         | `FormattedLabels` | Unchanged |                              |
| extra_fields   | `HashMap<String, String>` | Unchanged |                    |
| http_headers   | `HeaderMap`| Unchanged |                                     |
| log_format     | `LogLineFormat` | Unchanged |                                |
| field_mappings | `Vec<FieldMapping>` | Unchanged |                          |
| exclude_unmapped_fields | `bool` | Unchanged |                             |
| backoff        | `Duration` | New       | Default: 500ms                      |
| channel_capacity | `usize`  | New       | Default: 512                        |

### ErrorInner (modified)

| Variant                | Change | Notes                                       |
|------------------------|--------|---------------------------------------------|
| ZeroChannelCapacity    | New    | Returned when `channel_capacity(0)` is called |

### Public Type Alias (new)

```
pub type BackgroundTaskFuture = Pin<Box<dyn Future<Output = ()> + Send>>
```

## State Transitions

### Background Task Loop

```
[Start] → Receive (recv_async) → Drain (try_recv loop) → Encode & Send → Handle Result → Sleep (backoff) → [Receive]
                                        ↓ None received
                                   Flush pending → [Exit]

On send error:
  [Handle Result] → Increment backoff_count → Compute backoff_time → Sleep (backoff_time) → [Receive]
  If backoff_time >= 30s: drop outstanding queued events

On send success:
  [Handle Result] → Reset backoff_count → Clear sent entries → Sleep (base backoff) → [Receive]
```
