use super::event_channel;
use super::BackgroundTask;
use super::BackgroundTaskController;
use super::BackgroundTaskFuture;
use super::Error;
use super::ErrorI;
use super::FormattedLabels;
use super::Layer;
use std::collections::hash_map;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

/// Determines how log entry lines are serialized before being sent to Loki.
///
/// # Examples
///
/// ```
/// // Use plain text format via the Builder:
/// let builder = tracing_loki::builder().plain_text();
/// ```
#[derive(Clone, Debug, Default)]
pub enum LogLineFormat {
    /// Serialize log entries as JSON objects (default).
    ///
    /// The JSON object contains the event message, extra fields, span fields,
    /// and metadata (`_target`, `_module_path`, `_file`, `_line`, `_spans`).
    #[default]
    Json,
    /// Serialize log entries as plain text.
    ///
    /// The entry line contains the message text. When unmapped fields are
    /// included (the default), they are appended as `key=value` pairs.
    /// Metadata fields are excluded by default in plain text mode.
    PlainText,
}

/// Maps a tracing event/span field to a Loki stream label.
///
/// Created via [`Builder::field_to_label`].
#[derive(Clone, Debug)]
pub struct FieldMapping {
    /// The tracing field name to match (e.g., `"service"`, `"_target"`).
    pub source_field: String,
    /// The Loki label name to produce (e.g., `"service"`, `"target"`).
    pub target_label: String,
}

/// Create a [`Builder`] for constructing a [`Layer`] and its corresponding
/// [`BackgroundTaskFuture`].
///
/// See the crate's root documentation for an example.
pub fn builder() -> Builder {
    let mut http_headers = reqwest::header::HeaderMap::new();
    http_headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/x-snappy"),
    );
    Builder {
        labels: FormattedLabels::new(),
        extra_fields: HashMap::new(),
        http_headers,
        log_format: LogLineFormat::Json,
        field_mappings: Vec::new(),
        exclude_unmapped_fields: false,
        backoff: Duration::from_millis(500),
        channel_capacity: 512,
    }
}

/// Builder for constructing a [`Layer`] and its corresponding
/// [`BackgroundTaskFuture`].
///
/// See the crate's root documentation for an example.
#[derive(Clone)]
pub struct Builder {
    labels: FormattedLabels,
    extra_fields: HashMap<String, String>,
    http_headers: reqwest::header::HeaderMap,
    log_format: LogLineFormat,
    field_mappings: Vec<FieldMapping>,
    exclude_unmapped_fields: bool,
    backoff: Duration,
    channel_capacity: usize,
}

impl Builder {
    /// Add a label to the logs sent to Loki through the built `Layer`.
    ///
    /// Labels are supposed to be closed categories with few possible values.
    /// For example, `"environment"` with values `"ci"`, `"development"`,
    /// `"staging"` or `"production"` would work well.
    ///
    /// For open categories, extra fields are a better fit. See
    /// [`Builder::extra_field`].
    ///
    /// No two labels can share the same name, and the key `"level"` is
    /// reserved for the log level.
    ///
    /// # Errors
    ///
    /// This function will return an error if a key is a duplicate or when the
    /// key is `"level"`.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_loki::Error;
    /// # fn main() -> Result<(), Error> {
    /// let builder = tracing_loki::builder()
    ///     .label("environment", "production")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn label<S: Into<String>, T: AsRef<str>>(
        mut self,
        key: S,
        value: T,
    ) -> Result<Builder, Error> {
        self.labels.add(key.into(), value.as_ref())?;
        Ok(self)
    }
    /// Set an extra field that is sent with all log records sent to Loki
    /// through the built layer.
    ///
    /// Fields are meant to be used for open categories or closed categories
    /// with many options. For example, `"run_id"` with randomly generated
    /// [UUIDv4](https://en.wikipedia.org/w/index.php?title=Universally_unique_identifier&oldid=1105876960#Version_4_(random))s
    /// would be a good fit for these extra fields.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_loki::Error;
    /// # fn main() -> Result<(), Error> {
    /// let builder = tracing_loki::builder()
    ///     .extra_field("run_id", "5b6aedb4-e2c1-4ad9-b8a7-3ef92b5c8120")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn extra_field<S: Into<String>, T: Into<String>>(
        mut self,
        key: S,
        value: T,
    ) -> Result<Builder, Error> {
        match self.extra_fields.entry(key.into()) {
            hash_map::Entry::Occupied(o) => {
                return Err(Error(ErrorI::DuplicateExtraField(o.key().clone())));
            }
            hash_map::Entry::Vacant(v) => {
                v.insert(value.into());
            }
        }
        Ok(self)
    }
    /// Set an extra HTTP header to be sent with all requests sent to Loki.
    ///
    /// This can be useful to set the `X-Scope-OrgID` header which Loki
    /// processes as the tenant ID in a multi-tenant setup.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_loki::Error;
    /// # fn main() -> Result<(), Error> {
    /// let builder = tracing_loki::builder()
    ///     // Set the tenant ID for Loki.
    ///     .http_header("X-Scope-OrgID", "7662a206-fa0f-407f-abe9-261d652c750b")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn http_header<S: AsRef<str>, T: AsRef<str>>(
        mut self,
        key: S,
        value: T,
    ) -> Result<Builder, Error> {
        let key = key.as_ref();
        let value = value.as_ref();
        if self
            .http_headers
            .insert(
                reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    .map_err(|_| Error(ErrorI::InvalidHttpHeaderName(key.into())))?,
                reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| Error(ErrorI::InvalidHttpHeaderValue(key.into())))?,
            )
            .is_some()
        {
            return Err(Error(ErrorI::DuplicateHttpHeader(key.into())));
        }
        Ok(self)
    }
    /// Switch the log entry format to plain text.
    ///
    /// In plain text mode, the entry line contains only the message text.
    /// When unmapped fields are included (the default), they are appended as
    /// `key=value` pairs. Metadata fields (`_target`, `_module_path`, etc.)
    /// are excluded by default in plain text mode.
    ///
    /// The default format is JSON.
    ///
    /// # Example
    ///
    /// ```
    /// let builder = tracing_loki::builder()
    ///     .plain_text();
    /// ```
    pub fn plain_text(mut self) -> Builder {
        self.log_format = LogLineFormat::PlainText;
        self
    }
    /// Map a tracing event or span field to a Loki stream label.
    ///
    /// When an event is emitted, if it contains a field matching
    /// `source_field`, the field's value is promoted to a Loki stream label
    /// named `target_label`, and the field is removed from the entry line.
    ///
    /// The `target_label` must contain only `[A-Za-z_]` characters, must not
    /// be `"level"` (reserved), and must not conflict with any existing
    /// static label. Each `source_field` can only be mapped once.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `target_label` contains invalid characters
    /// - `target_label` is `"level"`
    /// - `target_label` conflicts with an existing static label
    /// - `source_field` is already mapped
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_loki::Error;
    /// # fn main() -> Result<(), Error> {
    /// let builder = tracing_loki::builder()
    ///     .label("host", "mine")?
    ///     .field_to_label("service", "service")?
    ///     .field_to_label("task", "task_name")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn field_to_label<S: Into<String>, T: Into<String>>(
        mut self,
        source: S,
        target: T,
    ) -> Result<Builder, Error> {
        let source_field = source.into();
        let target_label = target.into();

        // Validate target label characters: [A-Za-z_]
        for (i, b) in target_label.bytes().enumerate() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'_' => {}
                _ => {
                    let c = target_label[i..].chars().next().unwrap();
                    return Err(Error(ErrorI::InvalidFieldMappingLabelCharacter(
                        target_label,
                        c,
                    )));
                }
            }
        }

        // Reject "level"
        if target_label == "level" {
            return Err(Error(ErrorI::ReservedLabelLevel));
        }

        // Check conflict with static labels
        if self.labels.contains(&target_label) {
            return Err(Error(ErrorI::FieldMappingConflictsWithLabel(target_label)));
        }

        // Check duplicate source field
        if self
            .field_mappings
            .iter()
            .any(|m| m.source_field == source_field)
        {
            return Err(Error(ErrorI::DuplicateFieldMapping(source_field)));
        }

        self.field_mappings.push(FieldMapping {
            source_field,
            target_label,
        });
        Ok(self)
    }
    /// Exclude unmapped fields from the log entry line.
    ///
    /// When enabled, only the message text and extra fields (set via
    /// [`Builder::extra_field`]) appear in the entry line. Event fields and
    /// span fields that are not mapped to labels via [`Builder::field_to_label`]
    /// are discarded.
    ///
    /// By default, all fields are included in the entry line.
    ///
    /// # Example
    ///
    /// ```
    /// let builder = tracing_loki::builder()
    ///     .plain_text()
    ///     .exclude_unmapped_fields();
    /// ```
    pub fn exclude_unmapped_fields(mut self) -> Builder {
        self.exclude_unmapped_fields = true;
        self
    }
    /// Set the base backoff interval for the background task send loop.
    ///
    /// The background task sleeps for this duration between send cycles.
    /// On send failure, exponential backoff is applied starting from this base.
    ///
    /// Default: 500ms.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let builder = tracing_loki::builder()
    ///     .backoff(Duration::from_millis(100));
    /// ```
    pub fn backoff(mut self, backoff: Duration) -> Builder {
        self.backoff = backoff;
        self
    }
    /// Set the capacity of the internal event channel.
    ///
    /// When the channel is full, new events are dropped without blocking.
    /// Higher values use more memory but reduce drop probability under load.
    ///
    /// Default: 512. Must be > 0.
    ///
    /// Returns `Err` if `capacity` is 0.
    ///
    /// # Example
    ///
    /// ```
    /// # use tracing_loki::Error;
    /// # fn main() -> Result<(), Error> {
    /// let builder = tracing_loki::builder()
    ///     .channel_capacity(1024)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn channel_capacity(mut self, capacity: usize) -> Result<Builder, Error> {
        if capacity == 0 {
            return Err(Error(ErrorI::ZeroChannelCapacity));
        }
        self.channel_capacity = capacity;
        Ok(self)
    }
    /// Build the tracing [`Layer`] and its corresponding [`BackgroundTaskFuture`].
    ///
    /// The `loki_url` is the URL of the Loki server, like
    /// `https://127.0.0.1:3100`.
    ///
    /// The [`Layer`] needs to be registered with a
    /// [`tracing_subscriber::Registry`], and the [`BackgroundTaskFuture`] needs to
    /// be [`tokio::spawn`]ed.
    ///
    /// **Note** that unlike the [`layer`](`crate::layer`) function, this
    /// function **does not strip off** the path component of `loki_url` before
    /// appending `/loki/api/v1/push`.
    ///
    /// See the crate's root documentation for an example.
    pub fn build_url(self, loki_url: Url) -> Result<(Layer, BackgroundTaskFuture), Error> {
        let (sender, receiver) = event_channel(self.channel_capacity);
        Ok((
            Layer {
                sender,
                extra_fields: self.extra_fields,
                log_format: self.log_format,
                field_mappings: self.field_mappings,
                exclude_unmapped_fields: self.exclude_unmapped_fields,
                dropped_count: Arc::new(AtomicU64::new(0)),
                last_drop_warning: Arc::new(std::sync::Mutex::new(None)),
            },
            Box::pin(
                BackgroundTask::new(
                    loki_url,
                    self.http_headers,
                    receiver,
                    &self.labels,
                    self.backoff,
                )?
                .start(),
            ),
        ))
    }
    /// Build the tracing [`Layer`], [`BackgroundTaskFuture`] and its
    /// [`BackgroundTaskController`].
    ///
    /// The [`BackgroundTaskController`] can be used to signal the background
    /// task to shut down.
    ///
    /// The `loki_url` is the URL of the Loki server, like
    /// `https://127.0.0.1:3100`.
    ///
    /// The [`Layer`] needs to be registered with a
    /// [`tracing_subscriber::Registry`], and the [`BackgroundTaskFuture`] needs to
    /// be [`tokio::spawn`]ed.
    ///
    /// **Note** that unlike the [`layer`](`crate::layer`) function, this
    /// function **does not strip off** the path component of `loki_url` before
    /// appending `/loki/api/v1/push`.
    ///
    /// See the crate's root documentation for an example.
    pub fn build_controller_url(
        self,
        loki_url: Url,
    ) -> Result<(Layer, BackgroundTaskController, BackgroundTaskFuture), Error> {
        let (sender, receiver) = event_channel(self.channel_capacity);
        Ok((
            Layer {
                sender: sender.clone(),
                extra_fields: self.extra_fields,
                log_format: self.log_format,
                field_mappings: self.field_mappings,
                exclude_unmapped_fields: self.exclude_unmapped_fields,
                dropped_count: Arc::new(AtomicU64::new(0)),
                last_drop_warning: Arc::new(std::sync::Mutex::new(None)),
            },
            BackgroundTaskController { sender },
            Box::pin(
                BackgroundTask::new(
                    loki_url,
                    self.http_headers,
                    receiver,
                    &self.labels,
                    self.backoff,
                )?
                .start(),
            ),
        ))
    }
}
