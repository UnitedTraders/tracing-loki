//! A [`tracing`] layer for shipping logs to [Grafana
//! Loki](https://grafana.com/oss/loki/).
//!
//! Usage
//! =====
//!
//! ```rust
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_subscriber::util::SubscriberInitExt;
//! use std::process;
//! use url::Url;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), tracing_loki::Error> {
//!     let (layer, task) = tracing_loki::builder()
//!         .label("host", "mine")?
//!         .extra_field("pid", format!("{}", process::id()))?
//!         .build_url(Url::parse("http://127.0.0.1:3100").unwrap())?;
//!
//!     // We need to register our layer with `tracing`.
//!     tracing_subscriber::registry()
//!         .with(layer)
//!         // One could add more layers here, for example logging to stdout:
//!         // .with(tracing_subscriber::fmt::Layer::new())
//!         .init();
//!
//!     // The background task needs to be spawned so the logs actually get
//!     // delivered.
//!     tokio::spawn(task);
//!
//!     tracing::info!(
//!         task = "tracing_setup",
//!         result = "success",
//!         "tracing successfully set up",
//!     );
//!
//!     Ok(())
//! }
//! ```

#![allow(clippy::or_fun_call)]
#![allow(clippy::type_complexity)]
#![deny(missing_docs)]

#[cfg(not(feature = "compat-0-2-1"))]
compile_error!(
    "The feature `compat-0-2-1` must be enabled to ensure \
    forward compatibility with future versions of this crate"
);

/// The re-exported `url` dependency of this crate.
///
/// Use this to avoid depending on a potentially-incompatible `url` version yourself.
pub extern crate url;

use loki_api::logproto as loki;
use loki_api::prost;
use std::cmp;
use std::collections::HashMap;
use std::error;
use std::fmt;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tracing::instrument::WithSubscriber;
use tracing_core::field::Field;
use tracing_core::field::Visit;
use tracing_core::span::Attributes;
use tracing_core::span::Id;
use tracing_core::span::Record;
use tracing_core::Event;
use tracing_core::Level;
use tracing_core::Subscriber;
use tracing_log::NormalizeEvent;
use tracing_subscriber::layer::Context as TracingContext;
use tracing_subscriber::registry::LookupSpan;
use url::Url;

use labels::FormattedLabels;
use no_subscriber::NoSubscriber;
use ErrorInner as ErrorI;

pub use builder::builder;
pub use builder::Builder;
pub use builder::FieldMapping;
pub use builder::LogLineFormat;

mod builder;
mod labels;
mod level_map;
mod log_support;
mod no_subscriber;

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

fn event_channel() -> (
    mpsc::Sender<Option<LokiEvent>>,
    mpsc::Receiver<Option<LokiEvent>>,
) {
    mpsc::channel(512)
}

/// The error type for constructing a [`Layer`].
///
/// Nothing except for the [`std::error::Error`] (and [`std::fmt::Debug`] and
/// [`std::fmt::Display`]) implementation of this type is exposed.
pub struct Error(ErrorInner);

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl error::Error for Error {}

#[derive(Debug)]
enum ErrorInner {
    DuplicateExtraField(String),
    DuplicateFieldMapping(String),
    DuplicateHttpHeader(String),
    DuplicateLabel(String),
    FieldMappingConflictsWithLabel(String),
    InvalidFieldMappingLabelCharacter(String, char),
    InvalidHttpHeaderName(String),
    InvalidHttpHeaderValue(String),
    InvalidLabelCharacter(String, char),
    InvalidLokiUrl,
    ReservedLabelLevel,
}

impl fmt::Display for ErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ErrorInner::*;
        match self {
            DuplicateExtraField(key) => write!(f, "duplicate extra field key {:?}", key),
            DuplicateFieldMapping(source) => {
                write!(f, "duplicate field mapping source {:?}", source)
            }
            DuplicateHttpHeader(name) => write!(f, "duplicate HTTP header {:?}", name),
            DuplicateLabel(key) => write!(f, "duplicate label key {:?}", key),
            FieldMappingConflictsWithLabel(target) => {
                write!(
                    f,
                    "field mapping target {:?} conflicts with an existing label",
                    target
                )
            }
            InvalidFieldMappingLabelCharacter(target, c) => {
                write!(
                    f,
                    "invalid character {:?} in field mapping target label {:?}",
                    c, target
                )
            }
            InvalidHttpHeaderName(name) => write!(f, "invalid HTTP header name {:?}", name),
            InvalidHttpHeaderValue(name) => write!(f, "invalid HTTP header value for {:?}", name),
            InvalidLabelCharacter(key, c) => {
                write!(f, "invalid label character {:?} in key {:?}", c, key)
            }
            InvalidLokiUrl => write!(f, "invalid Loki URL"),
            ReservedLabelLevel => write!(f, "cannot add custom label for \"level\""),
        }
    }
}

/// Construct a [`Layer`] and its corresponding [`BackgroundTask`].
///
/// The [`Layer`] needs to be registered with a
/// [`tracing_subscriber::Registry`], and the [`BackgroundTask`] needs to be
/// [`tokio::spawn`]ed.
///
/// **Note** that unlike the [`Builder::build_url`] function, this function
/// **strips off** the path component of `loki_url` before appending
/// `/loki/api/v1/push`.
///
/// See [`builder()`] and this crate's root documentation for a more flexible
/// method.
///
/// # Example
///
/// ```rust
/// use tracing_subscriber::layer::SubscriberExt;
/// use tracing_subscriber::util::SubscriberInitExt;
/// use url::Url;
///
/// #[tokio::main]
/// async fn main() -> Result<(), tracing_loki::Error> {
///     let (layer, task) = tracing_loki::layer(
///         Url::parse("http://127.0.0.1:3100").unwrap(),
///         vec![("host".into(), "mine".into())].into_iter().collect(),
///         vec![].into_iter().collect(),
///     )?;
///
///     // We need to register our layer with `tracing`.
///     tracing_subscriber::registry()
///         .with(layer)
///         // One could add more layers here, for example logging to stdout:
///         // .with(tracing_subscriber::fmt::Layer::new())
///         .init();
///
///     // The background task needs to be spawned so the logs actually get
///     // delivered.
///     tokio::spawn(task);
///
///     tracing::info!(
///         task = "tracing_setup",
///         result = "success",
///         "tracing successfully set up",
///     );
///
///     Ok(())
/// }
/// ```
pub fn layer(
    loki_url: Url,
    labels: HashMap<String, String>,
    extra_fields: HashMap<String, String>,
) -> Result<(Layer, BackgroundTask), Error> {
    let mut builder = builder();
    for (key, value) in labels {
        builder = builder.label(key, value)?;
    }
    for (key, value) in extra_fields {
        builder = builder.extra_field(key, value)?;
    }
    builder.build_url(
        loki_url
            .join("/")
            .map_err(|_| Error(ErrorI::InvalidLokiUrl))?,
    )
}

/// The [`tracing_subscriber::Layer`] implementation for the Loki backend.
///
/// See the crate's root documentation for an example.
pub struct Layer {
    extra_fields: HashMap<String, String>,
    sender: mpsc::Sender<Option<LokiEvent>>,
    log_format: builder::LogLineFormat,
    field_mappings: Vec<builder::FieldMapping>,
    exclude_unmapped_fields: bool,
}

struct LokiEvent {
    trigger_send: bool,
    timestamp: SystemTime,
    level: Level,
    message: String,
    dynamic_labels: HashMap<String, String>,
}

#[derive(Default)]
struct Fields {
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Fields {
    fn record_impl(&mut self, field: &Field, value: serde_json::Value) {
        self.fields.insert(field.name().into(), value);
    }
    fn record<T: Into<serde_json::Value>>(&mut self, field: &Field, value: T) {
        self.record_impl(field, value.into());
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record(field, format!("{:?}", value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record(field, value);
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value);
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value);
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value);
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, value);
    }
    fn record_error(&mut self, field: &Field, value: &(dyn error::Error + 'static)) {
        self.record(field, format!("{}", value));
    }
}

/// Format a JSON value for plain text `key=value` output.
/// Values containing spaces are quoted with double quotes.
fn format_plain_text_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => format_plain_text_str(s),
        other => {
            let s = other.to_string();
            format_plain_text_str(&s)
        }
    }
}

/// Format a string for plain text `key=value` output.
/// Values containing spaces, quotes, or backslashes are quoted.
fn format_plain_text_str(s: &str) -> String {
    if s.contains(' ') || s.contains('"') || s.contains('\\') {
        format!("{:?}", s)
    } else {
        s.to_string()
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> tracing_subscriber::Layer<S> for Layer {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: TracingContext<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();
        if extensions.get_mut::<Fields>().is_none() {
            let mut fields = Fields::default();
            attrs.record(&mut fields);
            extensions.insert(fields);
        }
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: TracingContext<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();
        let fields = extensions.get_mut::<Fields>().expect("unregistered span");
        values.record(fields);
    }
    fn on_event(&self, event: &Event<'_>, ctx: TracingContext<'_, S>) {
        let timestamp = SystemTime::now();
        let normalized_meta = event.normalized_metadata();
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());
        let mut span_fields: serde_json::Map<String, serde_json::Value> = Default::default();
        let spans = event
            .parent()
            .cloned()
            .or_else(|| ctx.current_span().id().cloned())
            .and_then(|id| {
                ctx.span_scope(&id).map(|scope| {
                    scope.from_root().fold(Vec::new(), |mut spans, span| {
                        span_fields.extend(
                            span.extensions()
                                .get::<Fields>()
                                .expect("unregistered span")
                                .fields
                                .iter()
                                .map(|(f, v)| (f.clone(), v.clone())),
                        );
                        spans.push(span.name());
                        spans
                    })
                })
            })
            .unwrap_or(Vec::new());

        // Collect event fields into a map for field extraction
        let mut event_fields = Fields::default();
        event.record(&mut event_fields);

        // Extract dynamic labels from field mappings
        let mut dynamic_labels = HashMap::new();
        for mapping in &self.field_mappings {
            let value = event_fields
                .fields
                .remove(&mapping.source_field)
                .or_else(|| span_fields.remove(&mapping.source_field))
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .or_else(|| match mapping.source_field.as_str() {
                    "_target" => Some(meta.target().to_string()),
                    "_module_path" => meta.module_path().map(|s| s.to_string()),
                    "_file" => meta.file().map(|s| s.to_string()),
                    "_line" => meta.line().map(|l| l.to_string()),
                    _ => None,
                });
            if let Some(val) = value {
                dynamic_labels.insert(mapping.target_label.clone(), val);
            }
        }

        // Determine which metadata fields were mapped (to exclude from output)
        let target_mapped = self
            .field_mappings
            .iter()
            .any(|m| m.source_field == "_target");
        let module_path_mapped = self
            .field_mappings
            .iter()
            .any(|m| m.source_field == "_module_path");
        let file_mapped = self
            .field_mappings
            .iter()
            .any(|m| m.source_field == "_file");
        let line_mapped = self
            .field_mappings
            .iter()
            .any(|m| m.source_field == "_line");

        // Optionally exclude unmapped fields
        if self.exclude_unmapped_fields {
            event_fields.fields.retain(|k, _| k == "message");
            span_fields.clear();
        }

        let message = match self.log_format {
            builder::LogLineFormat::Json => {
                // Strip log.* fields from the event fields map (matching existing behavior)
                event_fields.fields.retain(|k, _| !k.starts_with("log."));

                // Build JSON with remaining fields
                let mut map = serde_json::Map::new();

                // Event fields (including message)
                for (k, v) in &event_fields.fields {
                    map.insert(k.clone(), v.clone());
                }

                // Extra fields
                for (k, v) in &self.extra_fields {
                    map.insert(k.clone(), serde_json::Value::String(v.clone()));
                }

                // Span fields
                for (k, v) in &span_fields {
                    map.insert(k.clone(), v.clone());
                }

                // Metadata (unless mapped)
                map.insert(
                    "_spans".into(),
                    serde_json::Value::Array(
                        spans
                            .iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect(),
                    ),
                );
                if !target_mapped {
                    map.insert(
                        "_target".into(),
                        serde_json::Value::String(meta.target().to_string()),
                    );
                }
                if !module_path_mapped {
                    if let Some(mp) = meta.module_path() {
                        map.insert(
                            "_module_path".into(),
                            serde_json::Value::String(mp.to_string()),
                        );
                    }
                }
                if !file_mapped {
                    if let Some(f) = meta.file() {
                        map.insert("_file".into(), serde_json::Value::String(f.to_string()));
                    }
                }
                if !line_mapped {
                    if let Some(l) = meta.line() {
                        map.insert("_line".into(), serde_json::json!(l));
                    }
                }

                serde_json::to_string(&map).expect("json serialization shouldn't fail")
            }
            builder::LogLineFormat::PlainText => {
                // Extract message
                let msg = event_fields
                    .fields
                    .remove("message")
                    .and_then(|v| match v {
                        serde_json::Value::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_default();

                let mut line = msg;

                // Append remaining event fields and span fields (if not excluded)
                if !self.exclude_unmapped_fields {
                    // Strip log.* fields
                    event_fields.fields.retain(|k, _| !k.starts_with("log."));

                    for (k, v) in &event_fields.fields {
                        let val_str = format_plain_text_value(v);
                        line.push(' ');
                        line.push_str(k);
                        line.push('=');
                        line.push_str(&val_str);
                    }
                    for (k, v) in &span_fields {
                        let val_str = format_plain_text_value(v);
                        line.push(' ');
                        line.push_str(k);
                        line.push('=');
                        line.push_str(&val_str);
                    }
                }

                // Extra fields are always appended
                for (k, v) in &self.extra_fields {
                    line.push(' ');
                    line.push_str(k);
                    line.push('=');
                    line.push_str(&format_plain_text_str(v));
                }

                line
            }
        };

        // TODO: Anything useful to do when the capacity has been reached?
        let _ = self.sender.try_send(Some(LokiEvent {
            trigger_send: !meta.target().starts_with("tracing_loki"),
            timestamp,
            level: *meta.level(),
            message,
            dynamic_labels,
        }));
    }
}

struct SendQueue {
    encoded_labels: String,
    sending: Vec<LokiEvent>,
    to_send: Vec<LokiEvent>,
}

impl SendQueue {
    fn new(encoded_labels: String) -> SendQueue {
        SendQueue {
            encoded_labels,
            sending: Vec::new(),
            to_send: Vec::new(),
        }
    }
    fn push(&mut self, event: LokiEvent) {
        // TODO: Add limit.
        self.to_send.push(event);
    }
    fn drop_outstanding(&mut self) -> usize {
        let len = self.sending.len();
        self.sending.clear();
        len
    }
    fn on_send_result(&mut self, result: Result<(), ()>) {
        match result {
            Ok(()) => self.sending.clear(),
            Err(()) => {
                self.sending.append(&mut self.to_send);
                mem::swap(&mut self.sending, &mut self.to_send);
            }
        }
    }
    fn should_send(&self) -> bool {
        self.to_send.iter().any(|e| e.trigger_send)
    }
    fn prepare_sending(&mut self) -> loki::StreamAdapter {
        if !self.sending.is_empty() {
            panic!("can only prepare sending while no request is in flight");
        }
        mem::swap(&mut self.sending, &mut self.to_send);
        loki::StreamAdapter {
            labels: self.encoded_labels.clone(),
            entries: self
                .sending
                .iter()
                .map(|e| loki::EntryAdapter {
                    timestamp: Some(e.timestamp.into()),
                    line: e.message.clone(),
                })
                .collect(),
            // Couldn't find documentation except for the promtail source code:
            // https://github.com/grafana/loki/blob/8c06c546ab15a568f255461f10318dae37e022d3/clients/pkg/promtail/client/batch.go#L55-L58
            //
            // In the Go code, the hash value isn't initialized explicitly,
            // hence it is set to 0.
            hash: 0,
        }
    }
}

#[derive(Debug)]
struct BadRedirect {
    status: u16,
    to: Url,
}

impl fmt::Display for BadRedirect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Following such a redirect drops the request body, and will likely
        // give an HTTP 200 response even though nobody ever looked at the POST
        // body.
        //
        // This can e.g. happen for login redirects when you post to a
        // login-protected URL.
        write!(f, "invalid HTTP {} redirect to {}", self.status, self.to)
    }
}

impl error::Error for BadRedirect {}

/// The background task that ships logs to Loki. It must be [`tokio::spawn`]ed
/// by the calling application.
///
/// See the crate's root documentation for an example.
pub struct BackgroundTask {
    loki_url: Url,
    receiver: mpsc::Receiver<Option<LokiEvent>>,
    labels: FormattedLabels,
    queues: HashMap<String, SendQueue>,
    buffer: Buffer,
    http_client: reqwest::Client,
    backoff_count: u32,
    backoff: Option<Pin<Box<tokio::time::Sleep>>>,
    quitting: bool,
    send_task:
        Option<Pin<Box<dyn Future<Output = Result<(), Box<dyn error::Error>>> + Send + 'static>>>,
}

impl BackgroundTask {
    fn new(
        loki_url: Url,
        http_headers: reqwest::header::HeaderMap,
        receiver: mpsc::Receiver<Option<LokiEvent>>,
        labels: &FormattedLabels,
    ) -> Result<BackgroundTask, Error> {
        Ok(BackgroundTask {
            receiver,
            loki_url: loki_url
                .join("loki/api/v1/push")
                .map_err(|_| Error(ErrorI::InvalidLokiUrl))?,
            labels: labels.clone(),
            queues: HashMap::new(),
            buffer: Buffer::new(),
            http_client: reqwest::Client::builder()
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .default_headers(http_headers)
                .redirect(reqwest::redirect::Policy::custom(|a| {
                    let status = a.status().as_u16();
                    if status == 302 || status == 303 {
                        let to = a.url().clone();
                        return a.error(BadRedirect { status, to });
                    }
                    reqwest::redirect::Policy::default().redirect(a)
                }))
                .build()
                .expect("reqwest client builder"),
            backoff_count: 0,
            backoff: None,
            quitting: false,
            send_task: None,
        })
    }
    fn backoff_time(&self) -> (bool, Duration) {
        let backoff_time = if self.backoff_count >= 1 {
            Duration::from_millis(
                500u64
                    .checked_shl(self.backoff_count - 1)
                    .unwrap_or(u64::MAX),
            )
        } else {
            Duration::from_millis(0)
        };
        (
            backoff_time >= Duration::from_secs(30),
            cmp::min(backoff_time, Duration::from_secs(600)),
        )
    }
}

impl Future for BackgroundTask {
    type Output = ();
    fn poll(mut self: Pin<&mut BackgroundTask>, cx: &mut Context<'_>) -> Poll<()> {
        let mut default_guard = tracing::subscriber::set_default(NoSubscriber::default());

        while let Poll::Ready(maybe_maybe_item) = Pin::new(&mut self.receiver).poll_recv(cx) {
            match maybe_maybe_item {
                Some(Some(item)) => {
                    let label_key = self
                        .labels
                        .finish_with_dynamic(item.level, &item.dynamic_labels);
                    self.queues
                        .entry(label_key.clone())
                        .or_insert_with(|| SendQueue::new(label_key))
                        .push(item);
                }
                Some(None) => self.quitting = true, // Explicit close.
                None => self.quitting = true,       // The sender was dropped.
            }
        }

        let mut backing_off = if let Some(backoff) = &mut self.backoff {
            matches!(Pin::new(backoff).poll(cx), Poll::Pending)
        } else {
            false
        };
        if !backing_off {
            self.backoff = None;
        }
        loop {
            if let Some(send_task) = &mut self.send_task {
                match Pin::new(send_task).poll(cx) {
                    Poll::Ready(res) => {
                        if let Err(e) = &res {
                            let (drop_outstanding, backoff_time) = self.backoff_time();
                            drop(default_guard);
                            tracing::error!(
                                error_count = self.backoff_count + 1,
                                ?backoff_time,
                                error = %e,
                                "couldn't send logs to loki",
                            );
                            default_guard =
                                tracing::subscriber::set_default(NoSubscriber::default());
                            if drop_outstanding {
                                let num_dropped: usize =
                                    self.queues.values_mut().map(|q| q.drop_outstanding()).sum();
                                drop(default_guard);
                                tracing::error!(
                                    num_dropped,
                                    "dropped outstanding messages due to sending errors",
                                );
                                default_guard =
                                    tracing::subscriber::set_default(NoSubscriber::default());
                            }
                            self.backoff = Some(Box::pin(tokio::time::sleep(backoff_time)));
                            self.backoff_count += 1;
                            backing_off = true;
                        } else {
                            self.backoff_count = 0;
                        }
                        let res = res.map_err(|_| ());
                        for q in self.queues.values_mut() {
                            q.on_send_result(res);
                        }
                        self.send_task = None;
                    }
                    Poll::Pending => {}
                }
            }
            if self.send_task.is_none()
                && !backing_off
                && self.queues.values().any(|q| q.should_send())
            {
                let streams = self
                    .queues
                    .values_mut()
                    .map(|q| q.prepare_sending())
                    .filter(|s| !s.entries.is_empty())
                    .collect();
                let body = self
                    .buffer
                    .encode(&loki::PushRequest { streams })
                    .to_owned();
                let request_builder = self.http_client.post(self.loki_url.clone());
                self.send_task = Some(Box::pin(
                    async move {
                        request_builder
                            .header(reqwest::header::CONTENT_TYPE, "application/x-snappy")
                            .body(body)
                            .send()
                            .await?
                            .error_for_status()?;
                        Ok(())
                    }
                    .with_subscriber(NoSubscriber::default()),
                ));
            } else {
                break;
            }
        }
        if self.quitting && self.send_task.is_none() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

struct Buffer {
    encoded: Vec<u8>,
    snappy: Vec<u8>,
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer {
            encoded: Vec::new(),
            snappy: Vec::new(),
        }
    }
    pub fn encode<'a, T: prost::Message>(&'a mut self, message: &T) -> &'a [u8] {
        self.encoded.clear();
        message
            .encode(&mut self.encoded)
            .expect("protobuf encoding is infallible");
        self.compress_encoded()
    }
    fn compress_encoded(&mut self) -> &[u8] {
        self.snappy
            .resize(snap::raw::max_compress_len(self.encoded.len()), 0);
        // Couldn't find documentation except for the promtail source code:
        // https://github.com/grafana/loki/blob/8c06c546ab15a568f255461f10318dae37e022d3/clients/pkg/promtail/client/batch.go#L101
        //
        // In the Go code, `snappy.Encode` is used, which corresponds to the
        // snappy block format, and not the snappy stream format. hence
        // `snap::raw` instead of `snap::write` is needed.
        let snappy_len = snap::raw::Encoder::new()
            .compress(&self.encoded, &mut self.snappy)
            .expect("snappy encoding is infallible");
        &self.snappy[..snappy_len]
    }
}

/// Handle to cleanly shut down the `BackgroundTask`.
///
/// It'll still try to send all available data and then quit.
pub struct BackgroundTaskController {
    sender: mpsc::Sender<Option<LokiEvent>>,
}

impl BackgroundTaskController {
    /// Shut down the associated `BackgroundTask`.
    pub async fn shutdown(&self) {
        // Ignore the error. If no one is listening, it already shut down.
        let _ = self.sender.send(None).await;
    }
}
