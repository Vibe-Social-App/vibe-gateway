use std::time::Duration;

pub const DEFAULT_UPSTREAM_TIMEOUT_SECS: u64 = 10;
pub const DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS: u64 = 3;
/// Routes that legitimately take long (large uploads, HLS/streaming playback).
/// WebSocket upgrades are always exempt regardless of this list.
pub const DEFAULT_LONG_ROUTES: &[&str] = &["/api/uploads", "/wrtc/hls"];
/// Time allowed for a long route to produce response headers.
pub const LONG_ROUTE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct UpstreamSettings {
    /// Max time to wait for the upstream to start responding (headers). Body
    /// streaming after that is not limited, so SSE/HLS/downloads are unaffected.
    pub timeout: Duration,
    pub connect_timeout: Duration,
    /// Path prefixes that use `long_timeout` instead of `timeout`.
    pub long_routes: Vec<String>,
    pub long_timeout: Duration,
}

impl Default for UpstreamSettings {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_UPSTREAM_TIMEOUT_SECS),
            connect_timeout: Duration::from_secs(DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS),
            long_routes: DEFAULT_LONG_ROUTES.iter().map(|s| s.to_string()).collect(),
            long_timeout: LONG_ROUTE_TIMEOUT,
        }
    }
}

fn parse_secs(v: Option<&str>, default: u64) -> Duration {
    let secs = v
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(default);
    Duration::from_secs(secs)
}

fn parse_routes(v: Option<&str>) -> Vec<String> {
    match v {
        Some(s) => s
            .split(',')
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect(),
        None => DEFAULT_LONG_ROUTES.iter().map(|s| s.to_string()).collect(),
    }
}

impl UpstreamSettings {
    pub fn from_values(
        timeout: Option<&str>,
        connect: Option<&str>,
        long_routes: Option<&str>,
    ) -> Self {
        Self {
            timeout: parse_secs(timeout, DEFAULT_UPSTREAM_TIMEOUT_SECS),
            connect_timeout: parse_secs(connect, DEFAULT_UPSTREAM_CONNECT_TIMEOUT_SECS),
            long_routes: parse_routes(long_routes),
            long_timeout: LONG_ROUTE_TIMEOUT,
        }
    }

    pub fn from_env() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        Self::from_values(
            get("UPSTREAM_TIMEOUT_SECS").as_deref(),
            get("UPSTREAM_CONNECT_TIMEOUT_SECS").as_deref(),
            get("UPSTREAM_LONG_ROUTES").as_deref(),
        )
    }

    pub fn timeout_for(&self, path: &str) -> Duration {
        let long = self.long_routes.iter().any(|p| {
            path == p || path.starts_with(&format!("{}/", p.trim_end_matches('/')))
        });
        if long {
            self.long_timeout.max(self.timeout)
        } else {
            self.timeout
        }
    }
}

pub fn build_client(s: &UpstreamSettings) -> reqwest::Client {
    // No total `.timeout()` here: it would also cap response bodies and
    // upgraded WebSocket connections. Header wait is bounded in the proxy.
    reqwest::Client::builder()
        .connect_timeout(s.connect_timeout)
        .tcp_keepalive(Duration::from_secs(60))
        .tcp_nodelay(true)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("Failed to build reqwest client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = UpstreamSettings::from_values(None, None, None);
        assert_eq!(s.timeout, Duration::from_secs(10));
        assert_eq!(s.connect_timeout, Duration::from_secs(3));
        assert_eq!(s.long_routes, vec!["/api/uploads", "/wrtc/hls"]);
    }

    #[test]
    fn parses_and_falls_back() {
        let s = UpstreamSettings::from_values(Some("5"), Some("x"), Some(" /a, /b/ ,,"));
        assert_eq!(s.timeout, Duration::from_secs(5));
        assert_eq!(s.connect_timeout, Duration::from_secs(3));
        assert_eq!(s.long_routes, vec!["/a", "/b/"]);
        assert_eq!(
            UpstreamSettings::from_values(Some("0"), None, None).timeout,
            Duration::from_secs(10)
        );
    }

    #[test]
    fn long_route_matching() {
        let s = UpstreamSettings::default();
        assert_eq!(s.timeout_for("/api/users"), Duration::from_secs(10));
        assert_eq!(s.timeout_for("/api/uploads"), LONG_ROUTE_TIMEOUT);
        assert_eq!(s.timeout_for("/api/uploads/x"), LONG_ROUTE_TIMEOUT);
        assert_eq!(s.timeout_for("/api/uploadsx"), Duration::from_secs(10));
    }
}
