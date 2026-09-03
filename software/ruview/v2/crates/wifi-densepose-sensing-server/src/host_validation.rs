//! Host-header allowlist for the sensing-server HTTP + WS surface.
//!
//! Defense against DNS rebinding: when the server is bound to loopback
//! (default `127.0.0.1`), a foreign page (e.g. `evil.com`) can lower its DNS
//! TTL and re-resolve to `127.0.0.1` after the browser has already accepted
//! the origin. From the browser's point of view the request is same-origin
//! against `evil.com`, so it reads the response — even though the bytes come
//! from the local sensing-server. Without `Host`-header validation the server
//! happily serves the request because every other axum layer treats it as a
//! normal connection.
//!
//! For RuView this means any website the user visits can stream live pose,
//! breathing rate, and heart-rate data out of the sensing-server (`/ws/sensing`,
//! `/api/v1/pose/current`, `/api/v1/vital-signs`, …), and trigger state-mutating
//! POSTs (`/api/v1/recording/start`, `/api/v1/models/load`, …) when bearer-auth
//! is not configured (the default LAN-only deployment posture from #443).
//!
//! The middleware here rejects any request whose `Host` header is not in the
//! configured allowlist with `421 Misdirected Request`. Defaults cover the
//! common local-only deployment (`localhost`, `127.0.0.1`, `[::1]` with or
//! without `:PORT`). Operators who bind to a routable address (`--bind-addr
//! 0.0.0.0` or a LAN IP) extend the allowlist with `--allowed-host` flags or
//! the `SENSING_ALLOWED_HOSTS` env var.
//!
//! Host names are deliberately not reused as browser Origins: a host-only
//! entry allows every port, while browser Origins are security principals and
//! must match their scheme, host, and port exactly. The browser-origin policy
//! is implemented by [`BrowserOriginAllowlist`].

use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{
        header::{HOST, ORIGIN},
        HeaderName, Method, StatusCode, Uri,
    },
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Environment variable that supplies additional allowed hosts
/// (comma-separated). Whitespace around each entry is trimmed; empty entries
/// are ignored.
pub const ALLOWED_HOSTS_ENV: &str = "SENSING_ALLOWED_HOSTS";

/// Environment variable that supplies explicit browser Origins
/// (comma-separated). Unlike [`ALLOWED_HOSTS_ENV`], every value must include a
/// scheme and an exact port (or the scheme's default port).
pub const ALLOWED_ORIGINS_ENV: &str = "SENSING_ALLOWED_ORIGINS";

/// Built-in allowlist entries. Each entry is also accepted with an optional
/// trailing `:PORT` (any port).
const DEFAULT_LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// Cheap, cloneable handle to the configured Host allowlist.
#[derive(Debug, Clone, Default)]
pub struct HostAllowlist {
    /// Lower-cased exact-match hostnames (with or without `:PORT` already
    /// baked in). Empty set ⇒ middleware accepts everything and is a no-op,
    /// matching the historical behaviour for callers that want to opt out.
    entries: Arc<HashSet<String>>,
}

impl HostAllowlist {
    /// Build an allowlist with only the default loopback names (bare and
    /// with any `:PORT`). Use this when the server is bound to loopback and
    /// no operator overrides have been supplied.
    pub fn loopback_only() -> Self {
        let mut entries: HashSet<String> = HashSet::new();
        for h in DEFAULT_LOOPBACK_HOSTS {
            entries.insert((*h).to_string());
        }
        HostAllowlist {
            entries: Arc::new(entries),
        }
    }

    /// Build an allowlist from an iterator of additional hostnames (each may
    /// optionally include a `:PORT` suffix). The default loopback set is
    /// always included so `--bind-addr 0.0.0.0` deployments do not lock out
    /// local browsers on `http://localhost:8080/…`.
    pub fn with_extra<I, S>(extras: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut entries: HashSet<String> = HashSet::new();
        for h in DEFAULT_LOOPBACK_HOSTS {
            entries.insert((*h).to_string());
        }
        for h in extras {
            let h = h.as_ref().trim();
            if !h.is_empty() {
                entries.insert(h.to_lowercase());
            }
        }
        HostAllowlist {
            entries: Arc::new(entries),
        }
    }

    /// Build an allowlist by joining (a) the default loopback set, (b) any
    /// CLI-supplied extras, and (c) the comma-separated `SENSING_ALLOWED_HOSTS`
    /// env var. Order of precedence does not matter — the result is a set.
    pub fn from_cli_and_env<I, S>(cli_extras: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let env_extras: Vec<String> = std::env::var(ALLOWED_HOSTS_ENV)
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let cli_vec: Vec<String> = cli_extras
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        HostAllowlist::with_extra(cli_vec.into_iter().chain(env_extras))
    }

    /// Disable host-header validation entirely. Provided as an explicit escape
    /// hatch for operators who deploy the server behind a reverse proxy that
    /// already canonicalises `Host`, or for unit tests that need to bypass
    /// the layer.
    pub fn disabled() -> Self {
        HostAllowlist::default()
    }

    /// True if the middleware will enforce host validation. `false` ⇒ no-op.
    pub fn is_enabled(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Test-only accessor returning a sorted, lower-cased copy of the
    /// configured allowlist. Exposed via the `pub(crate)` boundary so we can
    /// unit-test the env-var parsing without reaching into the `Arc`.
    pub fn entries_for_test(&self) -> Vec<String> {
        let mut v: Vec<String> = self.entries.iter().cloned().collect();
        v.sort();
        v
    }

    /// Check whether `host` (the raw `Host` header value, e.g.
    /// `127.0.0.1:8080` or `[::1]`) is permitted. Comparison is case-insensitive
    /// on the host part; ports are matched verbatim if the allowlist entry
    /// pins one, otherwise the port is ignored.
    pub fn is_allowed(&self, host: &str) -> bool {
        if self.entries.is_empty() {
            return true;
        }
        let host = host.trim().to_lowercase();
        if host.is_empty() {
            return false;
        }

        // Exact match (e.g. allowlist contains `127.0.0.1:8080` and request
        // sent `Host: 127.0.0.1:8080`).
        if self.entries.contains(&host) {
            return true;
        }

        // Match on host-only when the allowlist entry has no port and the
        // request includes a port. Handles `Host: 127.0.0.1:8080` against
        // `127.0.0.1` in the allowlist, and `Host: [::1]:8080` against
        // `[::1]`.
        let host_only = strip_port(&host);
        if self.entries.contains(host_only) {
            return true;
        }

        false
    }
}

/// Exact browser-Origin allowlist for live WebSockets and state-changing API
/// requests.
///
/// This is intentionally separate from [`HostAllowlist`]. Host validation
/// accepts a host-only entry with any port for DNS-rebinding compatibility;
/// doing that for an HTTP `Origin` would allow a page served from an unrelated
/// local port to reach the unauthenticated loopback WebSocket/API surface.
#[derive(Debug, Clone, Default)]
pub struct BrowserOriginAllowlist {
    entries: Arc<HashSet<String>>,
}

impl BrowserOriginAllowlist {
    /// Build the policy from repeated CLI values and the comma-separated
    /// [`ALLOWED_ORIGINS_ENV`] variable. Explicit values replace the derived
    /// local UI defaults. Invalid values are rejected instead of ignored.
    pub fn from_cli_and_env<I, S>(cli_origins: I, http_port: u16) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let env_origins: Vec<String> = std::env::var(ALLOWED_ORIGINS_ENV)
            .ok()
            .map(|raw| raw.split(',').map(str::to_owned).collect())
            .unwrap_or_default();
        let explicit: Vec<String> = cli_origins
            .into_iter()
            .map(|origin| origin.as_ref().to_owned())
            .chain(env_origins)
            .map(|origin| origin.trim().to_owned())
            .filter(|origin| !origin.is_empty())
            .collect();

        if explicit.is_empty() {
            Ok(Self::local_ui(http_port))
        } else {
            Self::from_explicit(explicit)
        }
    }

    /// Build a policy from already separated explicit Origin values.
    pub fn from_explicit<I, S>(origins: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut entries = HashSet::new();
        for raw in origins {
            let raw = raw.as_ref().trim();
            if raw.is_empty() {
                continue;
            }
            let normalized = normalize_browser_origin(raw).ok_or_else(|| {
                format!(
                    "invalid browser Origin {raw:?}; expected http(s)://HOST[:PORT] without a resource path, query, credentials, or wildcard"
                )
            })?;
            entries.insert(normalized);
        }
        Ok(Self {
            entries: Arc::new(entries),
        })
    }

    /// Default browser Origins for the UI served by the sensing server.
    /// Port zero means an ephemeral listener, so no browser Origin can be
    /// derived safely and the allowlist stays empty.
    pub fn local_ui(http_port: u16) -> Self {
        if http_port == 0 {
            return Self::default();
        }
        let origins = DEFAULT_LOOPBACK_HOSTS
            .iter()
            .map(|host| format!("http://{host}:{http_port}"));
        Self::from_explicit(origins).expect("built-in loopback browser Origins are valid")
    }

    /// True when the normalized Origin is explicitly allowlisted.
    pub fn is_allowed(&self, origin: &str) -> bool {
        normalize_browser_origin(origin)
            .is_some_and(|normalized| self.entries.contains(&normalized))
    }

    /// Test-only accessor returning normalized entries in stable order.
    pub fn entries_for_test(&self) -> Vec<String> {
        let mut entries: Vec<String> = self.entries.iter().cloned().collect();
        entries.sort();
        entries
    }
}

/// Normalize an HTTP Origin to `scheme://host:port`.
///
/// Default HTTP/HTTPS ports are materialized so an omitted default port and
/// its explicit spelling represent the same origin. All other ports remain
/// exact. A path, query, fragment, userinfo, wildcard, or non-HTTP scheme is
/// rejected rather than being silently discarded.
fn normalize_browser_origin(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("null")
        || value.contains('#')
        || value.contains('*')
    {
        return None;
    }

    // An origin has no resource path. Keep the distinction between an
    // authority-only value (`http://localhost:8080`) and an explicitly
    // supplied root path (`http://localhost:8080/`) before `http::Uri`
    // normalizes both forms to the same path representation.
    if value.ends_with('/') {
        return None;
    }

    let uri: Uri = value.parse().ok()?;
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    // `http::Uri` represents an authority-only absolute URI as either an
    // empty path or `/`; both are the same origin serialization. Any other
    // path is a real resource path and must not be accepted as configuration.
    if !matches!(uri.path(), "" | "/") || uri.query().is_some() {
        return None;
    }

    let authority = uri.authority()?;
    if authority.as_str().contains('@') {
        return None;
    }
    let host = uri.host()?;
    if host.is_empty() || host.contains('%') || host.contains('*') {
        return None;
    }
    let port = match authority.as_str().strip_prefix(host)? {
        "" => default_port,
        port_suffix => {
            let port = port_suffix.strip_prefix(':')?;
            if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            port.parse::<u16>().ok()?
        }
    };
    let normalized_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host.to_ascii_lowercase())
    } else {
        host.to_ascii_lowercase()
    };
    if port == 0 {
        return None;
    }

    Some(format!("{scheme}://{normalized_host}:{port}"))
}

/// Strip a `:PORT` suffix from `host`, leaving the host portion. IPv6 literals
/// are wrapped in brackets (`[::1]:PORT`) so the last `:` is the port
/// separator; bracketed IPv6 without a port stays intact.
fn strip_port(host: &str) -> &str {
    if let Some(close) = host.strip_prefix('[').and_then(|_| host.find(']')) {
        // Bracketed IPv6: `[::1]` or `[::1]:8080`.
        if let Some(after) = host.get(close + 1..) {
            if after.starts_with(':') {
                return &host[..=close];
            }
        }
        return host;
    }
    match host.rfind(':') {
        Some(idx) => &host[..idx],
        None => host,
    }
}

/// Axum middleware: rejects any request whose `Host` header is not in the
/// configured allowlist. Use with [`axum::middleware::from_fn_with_state`].
///
/// Behaviour:
/// * No `Host` header → `400 Bad Request` (HTTP/1.1 requires one; HTTP/2
///   synthesises it from `:authority`, so a missing value is a real protocol
///   violation, not a rebinding signal).
/// * `Host` header present but not in the allowlist → `421 Misdirected Request`.
/// * Empty allowlist → no-op (the operator explicitly opted out).
pub async fn require_allowed_host(
    State(allowlist): State<HostAllowlist>,
    request: Request,
    next: Next,
) -> Response {
    if !allowlist.is_enabled() {
        return next.run(request).await;
    }
    let host_header = request
        .headers()
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let host_header = match host_header {
        Some(h) => h,
        None => {
            return (StatusCode::BAD_REQUEST, "missing Host header\n").into_response();
        }
    };
    if allowlist.is_allowed(&host_header) {
        next.run(request).await
    } else {
        (
            StatusCode::MISDIRECTED_REQUEST,
            "Host header not in allowlist (DNS-rebinding defense). \
             Set --allowed-host <name[:port]> or SENSING_ALLOWED_HOSTS=<comma-list> \
             to permit this hostname.\n",
        )
            .into_response()
    }
}

/// Whether a request can expose or mutate the sensing surface from a browser.
/// WebSocket handshakes are always checked because browsers do not apply CORS
/// to WebSockets. State-changing API calls are checked for CSRF-style browser
/// requests while non-browser clients without an `Origin` remain compatible.
fn browser_sensitive_request(request: &Request) -> bool {
    let path = request.uri().path();
    let websocket = path.starts_with("/ws/") || path == "/api/v1/stream/pose";
    let api_v1 = path == "/api/v1" || path.starts_with("/api/v1/");
    let mutation = api_v1
        && matches!(
            request.method(),
            &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
        );
    websocket || mutation
}

/// Axum middleware: reject cross-origin browser access to live WebSockets and
/// state-changing REST endpoints. The check is deliberately separate from
/// `Host` validation: `Host` blocks DNS rebinding, while `Origin` prevents a
/// page on a foreign site from opening a browser WebSocket or submitting a
/// state-changing request to a local service.
pub async fn require_safe_browser_origin(
    State(allowlist): State<BrowserOriginAllowlist>,
    request: Request,
    next: Next,
) -> Response {
    if !browser_sensitive_request(&request) {
        return next.run(request).await;
    }

    let mut origin_values = request.headers().get_all(ORIGIN).iter();
    match (origin_values.next(), origin_values.next()) {
        (Some(origin), None) => {
            let allowed = origin
                .to_str()
                .ok()
                .is_some_and(|value| allowlist.is_allowed(value));
            if !allowed {
                return (
                    StatusCode::FORBIDDEN,
                    "cross-origin browser request rejected (Origin is not allowlisted)\n",
                )
                    .into_response();
            }
        }
        (Some(_), Some(_)) => {
            return (
                StatusCode::FORBIDDEN,
                "browser request rejected (multiple Origin headers)\n",
            )
                .into_response();
        }
        (None, Some(_)) => {
            return (
                StatusCode::FORBIDDEN,
                "browser request rejected (multiple Origin headers)\n",
            )
                .into_response();
        }
        (None, None) => {
            if request
                .headers()
                .get(HeaderName::from_static("sec-fetch-site"))
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
            {
                return (
                    StatusCode::FORBIDDEN,
                    "cross-site browser request rejected (missing Origin)\n",
                )
                    .into_response();
            }
        }
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
        Router,
    };
    use tower::ServiceExt;

    fn router(allowlist: HostAllowlist) -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/api/v1/pose/current", get(|| async { "ok" }))
            .route("/ws/sensing", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                allowlist,
                require_allowed_host,
            ))
    }

    async fn status(router: Router, path: &str, host: Option<&str>) -> StatusCode {
        let mut req = Request::builder().method("GET").uri(path);
        if let Some(h) = host {
            req = req.header(HOST, h);
        }
        let req = req.body(Body::empty()).unwrap();
        router.oneshot(req).await.unwrap().status()
    }

    fn origin_router(allowlist: BrowserOriginAllowlist) -> Router {
        Router::new()
            .route("/api/v1/mutate", post(|| async { "ok" }))
            .route("/ws/sensing", get(|| async { "ok" }))
            .route("/api/v1/stream/pose", get(|| async { "ok" }))
            .route("/api/v1/pose/current", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                allowlist,
                require_safe_browser_origin,
            ))
    }

    async fn origin_status(
        router: Router,
        method: &str,
        path: &str,
        origin: Option<&str>,
        fetch_site: Option<&str>,
    ) -> StatusCode {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(origin) = origin {
            req = req.header(ORIGIN, origin);
        }
        if let Some(fetch_site) = fetch_site {
            req = req.header("Sec-Fetch-Site", fetch_site);
        }
        router
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn loopback_only_allows_default_hosts_with_any_port() {
        let r = router(HostAllowlist::loopback_only());
        for h in [
            "localhost",
            "localhost:8080",
            "127.0.0.1",
            "127.0.0.1:8080",
            "127.0.0.1:65535",
            "[::1]",
            "[::1]:8080",
        ] {
            assert_eq!(
                status(r.clone(), "/api/v1/pose/current", Some(h)).await,
                StatusCode::OK,
                "host {h} should be allowed under loopback_only()"
            );
        }
    }

    #[tokio::test]
    async fn loopback_only_rejects_foreign_hosts() {
        let r = router(HostAllowlist::loopback_only());
        for h in [
            "evil.com",
            "evil.com:8080",
            "127.0.0.1.evil.com",
            "192.168.1.10",
            "192.168.1.10:8080",
            "sensing.local",
        ] {
            assert_eq!(
                status(r.clone(), "/api/v1/pose/current", Some(h)).await,
                StatusCode::MISDIRECTED_REQUEST,
                "host {h} should be rejected under loopback_only()"
            );
        }
    }

    #[tokio::test]
    async fn rejects_missing_host_header() {
        let r = router(HostAllowlist::loopback_only());
        assert_eq!(
            status(r, "/api/v1/pose/current", None).await,
            StatusCode::BAD_REQUEST,
        );
    }

    #[tokio::test]
    async fn rejects_empty_host_header() {
        let r = router(HostAllowlist::loopback_only());
        assert_eq!(
            status(r, "/api/v1/pose/current", Some("")).await,
            StatusCode::MISDIRECTED_REQUEST,
        );
    }

    #[tokio::test]
    async fn rejection_applies_to_health_and_ws_routes_too() {
        // The whole router is fronted by the middleware — there is no
        // bypass for `/health` or `/ws/*`, because rebinding doesn't care
        // which route it targets, it cares about what bytes flow back.
        let r = router(HostAllowlist::loopback_only());
        assert_eq!(
            status(r.clone(), "/health", Some("evil.com")).await,
            StatusCode::MISDIRECTED_REQUEST,
        );
        assert_eq!(
            status(r, "/ws/sensing", Some("evil.com")).await,
            StatusCode::MISDIRECTED_REQUEST,
        );
    }

    #[tokio::test]
    async fn extras_extend_loopback_set() {
        let r = router(HostAllowlist::with_extra(["sensing.local", "192.168.1.10"]));
        assert_eq!(
            status(r.clone(), "/api/v1/pose/current", Some("sensing.local")).await,
            StatusCode::OK,
        );
        assert_eq!(
            status(
                r.clone(),
                "/api/v1/pose/current",
                Some("sensing.local:8080")
            )
            .await,
            StatusCode::OK,
        );
        assert_eq!(
            status(r.clone(), "/api/v1/pose/current", Some("192.168.1.10:8080")).await,
            StatusCode::OK,
        );
        // Loopback defaults are still in:
        assert_eq!(
            status(r.clone(), "/api/v1/pose/current", Some("127.0.0.1")).await,
            StatusCode::OK,
        );
        // Foreign hosts still rejected:
        assert_eq!(
            status(r, "/api/v1/pose/current", Some("evil.com")).await,
            StatusCode::MISDIRECTED_REQUEST,
        );
    }

    /// REGRESSION (ADR-080 #1 — X-Forwarded-For / X-Forwarded-Host spoofing).
    ///
    /// The DNS-rebinding allowlist must decide purely on the real `Host` header
    /// and ignore any client-supplied forwarding headers. Otherwise an attacker
    /// could spoof `X-Forwarded-Host: localhost` (or `X-Forwarded-For`) to slip a
    /// foreign `Host` past the allowlist. This test sends a rejected `Host:
    /// evil.com` *with* allowlisted forwarding headers and asserts the request is
    /// still `421` — the forwarded headers must not bypass the control. It also
    /// confirms an allowed `Host` stays `200` regardless of a hostile XFF.
    #[tokio::test]
    async fn forwarded_headers_never_bypass_host_allowlist() {
        let r = router(HostAllowlist::loopback_only());
        async fn with_forwarded(router: Router, host: &str, xff: &str, xfh: &str) -> StatusCode {
            let req = Request::builder()
                .method("GET")
                .uri("/api/v1/pose/current")
                .header(HOST, host)
                .header("X-Forwarded-For", xff)
                .header("X-Forwarded-Host", xfh)
                .body(Body::empty())
                .unwrap();
            router.oneshot(req).await.unwrap().status()
        }
        // Foreign Host + spoofed allowlisted forwarding headers ⇒ still rejected.
        assert_eq!(
            with_forwarded(r.clone(), "evil.com", "127.0.0.1", "localhost").await,
            StatusCode::MISDIRECTED_REQUEST,
            "X-Forwarded-* must not let a foreign Host bypass the allowlist"
        );
        // Allowed Host + hostile forwarding headers ⇒ still allowed (forwarded
        // headers are simply not consulted).
        assert_eq!(
            with_forwarded(r, "127.0.0.1:8080", "evil.com", "evil.com").await,
            StatusCode::OK,
            "the real Host header is the only signal; XFF/XFH are ignored"
        );
    }

    #[tokio::test]
    async fn disabled_allowlist_is_no_op() {
        let r = router(HostAllowlist::disabled());
        assert_eq!(
            status(r.clone(), "/api/v1/pose/current", Some("evil.com")).await,
            StatusCode::OK,
        );
        assert_eq!(
            status(r, "/api/v1/pose/current", None).await,
            StatusCode::OK,
        );
    }

    #[tokio::test]
    async fn case_insensitive_host_match() {
        let r = router(HostAllowlist::loopback_only());
        for h in ["LOCALHOST", "LocalHost:8080", "127.0.0.1"] {
            assert_eq!(
                status(r.clone(), "/api/v1/pose/current", Some(h)).await,
                StatusCode::OK,
                "host {h} should be allowed (case-insensitive)"
            );
        }
        let r2 = router(HostAllowlist::with_extra(["Sensing.Local"]));
        assert_eq!(
            status(r2, "/api/v1/pose/current", Some("sensing.local:8080")).await,
            StatusCode::OK,
        );
    }

    #[test]
    fn strip_port_handles_ipv4_ipv6_and_bare_hostnames() {
        assert_eq!(strip_port("localhost"), "localhost");
        assert_eq!(strip_port("localhost:8080"), "localhost");
        assert_eq!(strip_port("127.0.0.1"), "127.0.0.1");
        assert_eq!(strip_port("127.0.0.1:8080"), "127.0.0.1");
        assert_eq!(strip_port("[::1]"), "[::1]");
        assert_eq!(strip_port("[::1]:8080"), "[::1]");
        // No `:` at all
        assert_eq!(strip_port("sensing.local"), "sensing.local");
    }

    #[test]
    fn with_extra_trims_whitespace_and_skips_empty() {
        let allowlist = HostAllowlist::with_extra(["  sensing.local  ", "", "192.168.1.10"]);
        let entries = allowlist.entries_for_test();
        assert!(entries.contains(&"sensing.local".to_string()));
        assert!(entries.contains(&"192.168.1.10".to_string()));
        assert!(!entries.iter().any(|s| s.is_empty()));
    }

    #[test]
    fn loopback_only_includes_all_three_defaults() {
        let entries = HostAllowlist::loopback_only().entries_for_test();
        assert!(entries.contains(&"localhost".to_string()));
        assert!(entries.contains(&"127.0.0.1".to_string()));
        assert!(entries.contains(&"[::1]".to_string()));
    }

    #[test]
    fn empty_input_to_with_extra_still_includes_loopback_defaults() {
        // Calling `with_extra` with no extras (e.g. operator passed no
        // `--allowed-host` flags) must keep the loopback defaults so a fresh
        // 127.0.0.1 deployment isn't bricked.
        let entries: Vec<String> = Vec::new();
        let allowlist = HostAllowlist::with_extra(entries);
        assert!(allowlist.is_allowed("127.0.0.1"));
        assert!(allowlist.is_allowed("127.0.0.1:8080"));
        assert!(allowlist.is_allowed("localhost"));
        assert!(!allowlist.is_allowed("evil.com"));
    }

    #[test]
    fn env_constants_are_stable() {
        assert_eq!(ALLOWED_HOSTS_ENV, "SENSING_ALLOWED_HOSTS");
        assert_eq!(ALLOWED_ORIGINS_ENV, "SENSING_ALLOWED_ORIGINS");
    }

    #[test]
    fn browser_origins_match_exact_scheme_host_and_port() {
        let allowlist = BrowserOriginAllowlist::local_ui(8080);
        assert!(allowlist.is_allowed("http://localhost:8080"));
        assert!(allowlist.is_allowed("http://127.0.0.1:8080"));
        assert!(allowlist.is_allowed("http://[::1]:8080"));
        assert!(!allowlist.is_allowed("http://localhost:3000"));
        assert!(!allowlist.is_allowed("https://localhost:8080"));
        assert!(!allowlist.is_allowed("http://evil.example:8080"));
    }

    #[test]
    fn browser_origin_parser_rejects_unsafe_forms() {
        for origin in [
            "null",
            "localhost:8080",
            "http://localhost:8080/",
            "http://localhost:8080/path",
            "http://localhost:8080?query=1",
            "http://user@localhost:8080",
            "http://localhost:*",
            "http://localhost:0",
            "http://localhost:65536",
            "http://localhost:abc",
            "http://localhost:",
            "ftp://localhost:8080",
        ] {
            assert!(
                BrowserOriginAllowlist::from_explicit([origin]).is_err(),
                "unsafe Origin should be rejected: {origin}"
            );
        }
    }

    #[test]
    fn explicit_origins_replace_local_defaults_and_keep_default_ports_exact() {
        let allowlist = BrowserOriginAllowlist::from_explicit(["http://localhost:3000"]).unwrap();
        assert!(allowlist.is_allowed("http://localhost:3000"));
        assert!(!allowlist.is_allowed("http://localhost:8080"));

        let default_http = BrowserOriginAllowlist::from_explicit(["http://localhost"]).unwrap();
        assert!(default_http.is_allowed("http://localhost:80"));
        assert!(!default_http.is_allowed("http://localhost:8080"));
    }

    #[tokio::test]
    async fn foreign_origin_cannot_open_live_websocket() {
        for path in ["/ws/sensing", "/api/v1/stream/pose"] {
            let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
            assert_eq!(
                origin_status(r, "GET", path, Some("https://evil.example"), None).await,
                StatusCode::FORBIDDEN,
                "foreign Origin must not reach {path}"
            );
        }
    }

    #[tokio::test]
    async fn foreign_origin_cannot_submit_state_changing_api_request() {
        let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
        assert_eq!(
            origin_status(
                r,
                "POST",
                "/api/v1/mutate",
                Some("https://evil.example"),
                None,
            )
            .await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn cross_site_fetch_without_origin_is_rejected() {
        let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
        assert_eq!(
            origin_status(r, "POST", "/api/v1/mutate", None, Some("cross-site")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn allowed_origin_and_non_browser_clients_remain_supported() {
        let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
        assert_eq!(
            origin_status(
                r.clone(),
                "POST",
                "/api/v1/mutate",
                Some("http://localhost:8080"),
                None,
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            origin_status(r, "POST", "/api/v1/mutate", None, None).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn wrong_local_port_cannot_open_websocket_or_mutate_api() {
        let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
        assert_eq!(
            origin_status(
                r.clone(),
                "GET",
                "/ws/sensing",
                Some("http://localhost:3000"),
                None,
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            origin_status(
                r,
                "POST",
                "/api/v1/mutate",
                Some("http://localhost:3000"),
                None,
            )
            .await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn explicitly_allowed_cross_port_ui_remains_supported() {
        let allowlist = BrowserOriginAllowlist::from_explicit(["http://localhost:3000"]).unwrap();
        let r = origin_router(allowlist);
        assert_eq!(
            origin_status(
                r.clone(),
                "GET",
                "/ws/sensing",
                Some("http://localhost:3000"),
                None,
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            origin_status(
                r,
                "POST",
                "/api/v1/mutate",
                Some("http://localhost:3000"),
                None,
            )
            .await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn read_only_api_get_is_not_blocked_by_origin_layer() {
        let r = origin_router(BrowserOriginAllowlist::local_ui(8080));
        assert_eq!(
            origin_status(
                r,
                "GET",
                "/api/v1/pose/current",
                Some("https://evil.example"),
                None,
            )
            .await,
            StatusCode::OK
        );
    }
}
