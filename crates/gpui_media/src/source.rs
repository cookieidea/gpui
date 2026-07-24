use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};

/// A media URI accepted by the playback backend.
#[derive(Clone, PartialEq, Eq)]
pub struct MediaSource {
    uri: String,
    display_name: String,
    network: NetworkSourceOptions,
}

impl fmt::Debug for MediaSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MediaSource")
            .field("uri", &redacted_uri(&self.uri))
            .field("display_name", &self.display_name)
            .field("network", &self.network)
            .finish()
    }
}

/// HTTP-oriented options available to playback backends.
///
/// The built-in GStreamer backend applies supported properties to its URI
/// source elements. Custom backends may interpret the same options.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct NetworkSourceOptions {
    headers: BTreeMap<String, String>,
    user_id: Option<String>,
    user_password: Option<String>,
    user_agent: Option<String>,
    proxy: Option<String>,
    timeout: Option<Duration>,
    retry_count: Option<u32>,
    retry_backoff_factor: Option<Duration>,
    retry_backoff_max: Option<Duration>,
    automatic_redirect: Option<bool>,
    keep_alive: Option<bool>,
    strict_tls: Option<bool>,
    buffer_duration: Option<Duration>,
    buffer_size: Option<u32>,
    connection_speed_kbps: Option<u64>,
}

impl NetworkSourceOptions {
    pub fn with_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        validate_header(&name, &value)?;
        if let Some(previous_name) = self
            .headers
            .keys()
            .find(|previous| previous.eq_ignore_ascii_case(&name))
            .cloned()
        {
            self.headers.remove(&previous_name);
        }
        self.headers.insert(name, value);
        Ok(self)
    }

    pub fn with_bearer_token(self, token: impl AsRef<str>) -> Result<Self> {
        self.with_header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Configures HTTP Basic/Digest credentials without embedding them in the
    /// media URI.
    pub fn with_basic_auth(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.user_id = Some(username.into());
        self.user_password = Some(password.into());
        self
    }

    pub fn with_referer(self, referer: impl Into<String>) -> Result<Self> {
        self.with_header("Referer", referer)
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = Some(retry_count);
        self
    }

    pub fn with_retry_backoff(mut self, factor: Duration, maximum: Duration) -> Self {
        self.retry_backoff_factor = Some(factor);
        self.retry_backoff_max = Some(maximum);
        self
    }

    pub fn with_automatic_redirect(mut self, enabled: bool) -> Self {
        self.automatic_redirect = Some(enabled);
        self
    }

    pub fn with_keep_alive(mut self, enabled: bool) -> Self {
        self.keep_alive = Some(enabled);
        self
    }

    pub fn with_strict_tls(mut self, enabled: bool) -> Self {
        self.strict_tls = Some(enabled);
        self
    }

    pub fn with_buffer_duration(mut self, duration: Duration) -> Self {
        self.buffer_duration = Some(duration);
        self
    }

    pub fn with_buffer_size(mut self, bytes: u32) -> Self {
        self.buffer_size = Some(bytes);
        self
    }

    pub fn with_connection_speed_kbps(mut self, kbps: u64) -> Self {
        self.connection_speed_kbps = Some(kbps);
        self
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    #[cfg(feature = "backend-gstreamer")]
    pub(crate) fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    #[cfg(feature = "backend-gstreamer")]
    pub(crate) fn user_password(&self) -> Option<&str> {
        self.user_password.as_deref()
    }

    pub fn proxy(&self) -> Option<&str> {
        self.proxy.as_deref()
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub fn retry_count(&self) -> Option<u32> {
        self.retry_count
    }

    pub fn retry_backoff_factor(&self) -> Option<Duration> {
        self.retry_backoff_factor
    }

    pub fn retry_backoff_max(&self) -> Option<Duration> {
        self.retry_backoff_max
    }

    pub fn automatic_redirect(&self) -> Option<bool> {
        self.automatic_redirect
    }

    pub fn keep_alive(&self) -> Option<bool> {
        self.keep_alive
    }

    pub fn strict_tls(&self) -> Option<bool> {
        self.strict_tls
    }

    pub fn buffer_duration(&self) -> Option<Duration> {
        self.buffer_duration
    }

    pub fn buffer_size(&self) -> Option<u32> {
        self.buffer_size
    }

    pub fn connection_speed_kbps(&self) -> Option<u64> {
        self.connection_speed_kbps
    }
}

impl fmt::Debug for NetworkSourceOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkSourceOptions")
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field(
                "basic_auth_configured",
                &(self.user_id.is_some() || self.user_password.is_some()),
            )
            .field("user_agent", &self.user_agent)
            .field("proxy_configured", &self.proxy.is_some())
            .field("timeout", &self.timeout)
            .field("retry_count", &self.retry_count)
            .field("retry_backoff_factor", &self.retry_backoff_factor)
            .field("retry_backoff_max", &self.retry_backoff_max)
            .field("automatic_redirect", &self.automatic_redirect)
            .field("keep_alive", &self.keep_alive)
            .field("strict_tls", &self.strict_tls)
            .field("buffer_duration", &self.buffer_duration)
            .field("buffer_size", &self.buffer_size)
            .field("connection_speed_kbps", &self.connection_speed_kbps)
            .finish()
    }
}

impl MediaSource {
    /// Creates a source from an already encoded URI.
    pub fn from_uri(uri: impl Into<String>) -> Result<Self> {
        let uri = uri.into();
        let parsed = url::Url::parse(&uri).context("invalid media URI")?;
        let display_name = parsed
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| safe_uri_label(&parsed));

        Ok(Self {
            uri,
            display_name,
            network: NetworkSourceOptions::default(),
        })
    }

    /// Creates a file URI from a local path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let canonical = path
            .canonicalize()
            .with_context(|| format!("media file does not exist: {}", path.display()))?;
        if !canonical.is_file() {
            bail!("media source is not a file: {}", canonical.display());
        }

        let uri = url::Url::from_file_path(&canonical)
            .map_err(|_| {
                anyhow::anyhow!("cannot convert path to file URI: {}", canonical.display())
            })?
            .into();
        let display_name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("video")
            .to_owned();

        Ok(Self {
            uri,
            display_name,
            network: NetworkSourceOptions::default(),
        })
    }

    /// Treats inputs containing a URI scheme as URIs and all other inputs as
    /// local filesystem paths.
    pub fn parse(input: impl AsRef<str>) -> Result<Self> {
        let input = input.as_ref();
        match url::Url::parse(input) {
            Ok(url) if !url.scheme().is_empty() => Self::from_uri(url.to_string()),
            _ => Self::from_path(PathBuf::from(input)),
        }
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn with_network_options(mut self, options: NetworkSourceOptions) -> Self {
        self.network = options;
        self
    }

    pub fn network_options(&self) -> &NetworkSourceOptions {
        &self.network
    }

    #[cfg(any(feature = "backend-gstreamer", test))]
    pub(crate) fn redact_error_message(&self, message: &str) -> String {
        let mut redacted = message.replace(&self.uri, &redacted_uri(&self.uri));
        for secret in self.network.sensitive_values() {
            if !secret.is_empty() {
                redacted = redacted.replace(secret, "[REDACTED]");
            }
            if let Some(token) = secret.strip_prefix("Bearer ")
                && !token.is_empty()
            {
                redacted = redacted.replace(token, "[REDACTED]");
            }
        }
        redacted
    }
}

impl NetworkSourceOptions {
    #[cfg(any(feature = "backend-gstreamer", test))]
    fn sensitive_values(&self) -> impl Iterator<Item = &str> {
        self.headers
            .values()
            .map(String::as_str)
            .chain(self.user_id.iter().map(String::as_str))
            .chain(self.user_password.iter().map(String::as_str))
            .chain(self.proxy.iter().map(String::as_str))
    }
}

fn redacted_uri(uri: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(uri) else {
        return "[invalid media URI]".to_owned();
    };
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.into()
}

fn safe_uri_label(uri: &url::Url) -> String {
    match uri.host_str() {
        Some(host) => format!("{}://{host}", uri.scheme()),
        None => uri.scheme().to_owned(),
    }
}

fn validate_header(name: &str, value: &str) -> Result<()> {
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
    {
        bail!("invalid HTTP header name: {name:?}");
    }
    if !value
        .bytes()
        .all(|byte| byte == b'\t' || (byte >= b' ' && byte != 0x7f))
    {
        bail!("HTTP header value contains an invalid control character");
    }
    Ok(())
}

impl TryFrom<&Path> for MediaSource {
    type Error = anyhow::Error;

    fn try_from(path: &Path) -> Result<Self> {
        Self::from_path(path)
    }
}

impl TryFrom<PathBuf> for MediaSource {
    type Error = anyhow::Error;

    fn try_from(path: PathBuf) -> Result<Self> {
        Self::from_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaSource, NetworkSourceOptions};

    #[test]
    fn parses_remote_uri() {
        let source = MediaSource::parse("https://example.com/media/movie.mp4").unwrap();
        assert_eq!(source.uri(), "https://example.com/media/movie.mp4");
        assert_eq!(source.display_name(), "movie.mp4");
    }

    #[test]
    fn rejects_missing_local_file() {
        let error = MediaSource::parse("this-video-does-not-exist.mp4").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn network_headers_reject_invalid_names_and_newlines() {
        assert!(
            NetworkSourceOptions::default()
                .with_header("Bad Header", "value")
                .is_err()
        );
        assert!(
            NetworkSourceOptions::default()
                .with_header("Authorization", "one\r\ntwo")
                .is_err()
        );
    }

    #[test]
    fn network_header_names_are_replaced_case_insensitively() {
        let options = NetworkSourceOptions::default()
            .with_header("Authorization", "old")
            .unwrap()
            .with_header("authorization", "new")
            .unwrap();

        assert_eq!(options.headers().len(), 1);
        assert_eq!(options.headers().get("authorization").unwrap(), "new");
    }

    #[test]
    fn network_debug_output_redacts_credentials() {
        let options = NetworkSourceOptions::default()
            .with_header("Authorization", "Bearer secret")
            .unwrap()
            .with_basic_auth("private-user", "private-password")
            .with_proxy("https://user:password@proxy.example.com");
        let debug = format!("{options:?}");

        assert!(debug.contains("Authorization"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("private-user"));
        assert!(!debug.contains("private-password"));
        assert!(!debug.contains("password"));
    }

    #[test]
    fn media_source_retains_network_configuration() {
        let options = NetworkSourceOptions::default()
            .with_user_agent("gpui-video-test")
            .with_retry_count(5);
        let source = MediaSource::from_uri("https://example.com/video.mp4")
            .unwrap()
            .with_network_options(options.clone());

        assert_eq!(source.network_options(), &options);
    }

    #[test]
    fn debug_redacts_uri_and_network_credentials() {
        let source =
            MediaSource::from_uri("https://alice:secret@example.com/watch?token=signed-value")
                .unwrap()
                .with_network_options(
                    NetworkSourceOptions::default()
                        .with_bearer_token("bearer-secret")
                        .unwrap()
                        .with_basic_auth("alice", "basic-secret"),
                );

        let debug = format!("{source:?}");
        assert!(debug.contains("https://example.com/watch"));
        for secret in [
            "alice",
            "secret",
            "signed-value",
            "bearer-secret",
            "basic-secret",
        ] {
            assert!(!debug.contains(secret), "{secret:?} leaked in {debug}");
        }
    }

    #[test]
    fn backend_messages_are_redacted_with_the_source() {
        let source = MediaSource::from_uri("https://example.com/watch?token=signed-value")
            .unwrap()
            .with_network_options(
                NetworkSourceOptions::default()
                    .with_bearer_token("bearer-secret")
                    .unwrap(),
            );
        let message = format!(
            "request {} failed with Authorization: Bearer bearer-secret",
            source.uri()
        );

        let redacted = source.redact_error_message(&message);
        assert!(redacted.contains("https://example.com/watch"));
        assert!(!redacted.contains("signed-value"));
        assert!(!redacted.contains("bearer-secret"));
    }
}
