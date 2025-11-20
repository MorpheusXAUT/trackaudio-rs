use crate::TrackAudioError;
use std::time::Duration;
use url::{Host, Url};

/// Represents the configuration for a [`TrackAudioClient`](crate::TrackAudioClient).
///
/// This struct is used to configure the URL of the TrackAudio instance to connect to as well
/// as some internal channel capacities and the client ping interval.
#[derive(Debug, Clone)]
pub struct TrackAudioConfig {
    /// The URL of the TrackAudio instance to connect to. Default: `ws://127.0.0.1:49080/ws`
    pub url: String,
    /// The capacity of the internal command channel. Default: 256
    pub command_channel_capacity: usize,
    /// The capacity of the internal event channel. Default: 256
    pub event_channel_capacity: usize,
    /// The interval the client sends ping packages to TrackAudio to verify the connection is still healthy. Default: 15 seconds
    pub ping_interval: Duration,
}

impl TrackAudioConfig {
    /// Creates a new configuration for the [`TrackAudioClient`](crate::TrackAudioClient).
    ///
    /// The `url` parameter is flexible and supports the following values:
    ///
    /// - Full WebSocket URL: `ws://127.0.0.1:49080/ws` or `ws://192.168.1.69/ws`
    /// - Host only: `127.0.0.1` or `localhost` (uses default port 49080 and `/ws` path)
    /// - Host and port: `127.0.0.1:12345` (uses `/ws` path)
    ///
    /// Note that TrackAudio currently only supports IPv4 connections and does not bind to any IPv6 addresses.
    /// Furthermore, only the `ws://` scheme is allowed (no TLS encryption or `wss://` support).
    ///
    /// # Example
    /// ```rust
    /// use trackaudio::TrackAudioConfig;
    /// let config = TrackAudioConfig::new("192.168.1.69");
    /// assert!(config.is_ok());
    /// assert_eq!(config.unwrap().url, "ws://192.168.1.69:49080/ws");
    /// ```
    pub fn new(url: impl AsRef<str>) -> crate::Result<Self> {
        Ok(Self {
            url: Self::normalize_url(url.as_ref())?,
            command_channel_capacity: 256,
            event_channel_capacity: 256,
            ping_interval: Duration::from_secs(15),
        })
    }

    /// Overrides the default capacity of the internal command and event channels.
    /// This can be useful to reduce the memory footprint of the client.
    ///
    /// # Defaults
    /// - Command channel capacity: 256
    /// - Event channel capacity: 256
    ///
    /// # Example
    /// ```rust
    /// use trackaudio::TrackAudioConfig;
    /// let config = TrackAudioConfig::default()
    ///     .with_capacity(100, 50);
    /// assert_eq!(config.command_channel_capacity, 100);
    /// assert_eq!(config.event_channel_capacity, 50);
    /// ```
    pub fn with_capacity(
        mut self,
        command_channel_capacity: usize,
        event_channel_capacity: usize,
    ) -> Self {
        self.command_channel_capacity = command_channel_capacity;
        self.event_channel_capacity = event_channel_capacity;
        self
    }

    /// Overrides the default interval the [`TrackAudioClient`](crate::TrackAudioClient) sends ping packages to TrackAudio
    /// to verify the connection is still healthy.
    /// This can be useful to reduce traffic load or adapt to network instabilities.
    ///
    /// # Defaults
    /// - Ping interval: 15 seconds
    ///
    /// # Example
    /// ```rust
    /// use std::time::Duration;
    /// use trackaudio::TrackAudioConfig;
    /// let config = TrackAudioConfig::default()
    ///     .with_ping_interval(Duration::from_secs(10));
    /// assert_eq!(config.ping_interval, Duration::from_secs(10));
    /// ```
    pub fn with_ping_interval(mut self, ping_interval: Duration) -> Self {
        self.ping_interval = ping_interval;
        self
    }

    const REQUIRED_TRACKAUDIO_SCHEME: &'static str = "ws";
    const DEFAULT_TRACKAUDIO_PORT: u16 = 49080;
    const REQUIRED_TRACKAUDIO_PATH: &'static str = "/ws";
    #[cfg_attr(feature = "tracing", tracing::instrument(err))]
    fn normalize_url(raw: &str) -> crate::Result<String> {
        let raw = raw.trim();

        if raw.is_empty() {
            return Err(TrackAudioError::InvalidUrl("empty URL".to_string()));
        }

        let has_scheme = raw.starts_with("ws://") || raw.starts_with("wss://");
        let raw = if has_scheme {
            raw.to_string()
        } else {
            format!("ws://{raw}")
        };

        let mut url =
            Url::parse(&raw).map_err(|err| TrackAudioError::InvalidUrl(err.to_string()))?;

        if url.scheme() != Self::REQUIRED_TRACKAUDIO_SCHEME {
            #[cfg(feature = "tracing")]
            tracing::trace!(?url, "Overriding URL scheme");
            url.set_scheme(Self::REQUIRED_TRACKAUDIO_SCHEME)
                .expect("default TrackAudio scheme should be valid");
        }

        if url.host().is_some_and(|h| matches!(h, Host::Ipv6(_))) {
            return Err(TrackAudioError::InvalidUrl(
                "IPv6 not supported".to_string(),
            ));
        }

        if url.port().is_none() {
            #[cfg(feature = "tracing")]
            tracing::trace!(?url, "Setting default port");
            url.set_port(Some(Self::DEFAULT_TRACKAUDIO_PORT))
                .expect("default TrackAudio port should be valid");
        }

        if url.path() != Self::REQUIRED_TRACKAUDIO_PATH {
            #[cfg(feature = "tracing")]
            tracing::trace!(?url, "Overriding path");
            url.set_path(Self::REQUIRED_TRACKAUDIO_PATH);
        }

        Ok(url.to_string())
    }
}

impl Default for TrackAudioConfig {
    /// Returns a default configuration for the [`TrackAudioClient`](crate::TrackAudioClient) that connects to `ws://127.0.0.1:49080/ws`.
    fn default() -> Self {
        Self::new("ws://127.0.0.1:49080/ws").expect("Invalid default TrackAudio URL")
    }
}

#[cfg(test)]
mod tests {
    use crate::{TrackAudioConfig, TrackAudioError};
    use assert_matches::assert_matches;
    use test_log::test;

    #[test]
    fn default_url() {
        let config = TrackAudioConfig::default();
        assert_eq!(config.url, "ws://127.0.0.1:49080/ws");
    }

    #[test]
    fn full_url() {
        let config =
            TrackAudioConfig::new("ws://192.168.1.69:12345/ws").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:12345/ws");
    }

    #[test]
    fn host_only() {
        let config = TrackAudioConfig::new("192.168.1.69").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:49080/ws");
    }

    #[test]
    fn host_and_port() {
        let config = TrackAudioConfig::new("192.168.1.69:12345").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:12345/ws");
    }

    #[test]
    fn wss_scheme() {
        let config = TrackAudioConfig::new("wss://192.168.1.69").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:49080/ws");
    }

    #[test]
    fn path_override() {
        let config =
            TrackAudioConfig::new("ws://192.168.1.69:49080/wss").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:49080/ws");
    }

    #[test]
    fn host_and_path_override() {
        let config = TrackAudioConfig::new("192.168.1.69/wss").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:49080/ws");
    }

    #[test]
    fn trim_whitespace() {
        let config = TrackAudioConfig::new(" 192.168.1.69  ").expect("config should be valid");
        assert_eq!(config.url, "ws://192.168.1.69:49080/ws");
    }

    #[test]
    fn empty_string() {
        let err = TrackAudioConfig::new("  ").expect_err("config should be invalid");
        assert_matches!(err, TrackAudioError::InvalidUrl(err) if err == "empty URL");
    }

    #[test]
    fn scheme_without_host() {
        let err = TrackAudioConfig::new("ws://").expect_err("config should be invalid");
        assert_matches!(err, TrackAudioError::InvalidUrl(err) if err == "empty host");
    }

    #[test]
    fn ipv6_host() {
        let err = TrackAudioConfig::new("[::1]").expect_err("config should be invalid");
        assert_matches!(err, TrackAudioError::InvalidUrl(err) if err == "IPv6 not supported");
    }
}
