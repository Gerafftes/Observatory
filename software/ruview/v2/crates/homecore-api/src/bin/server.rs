//! `homecore-api-server` binary. Boots a HomeCore runtime and serves
//! the HA-compat REST + WS API.
//!
//! ## Auth (ADR-161, HC-WS-08)
//!
//! Token provisioning matches `homecore-server`: if `HOMECORE_TOKENS`
//! is set (comma-separated bearer tokens) the API enforces that
//! whitelist on both the REST and WS paths. If it is **unset**, the
//! token store is locked and every bearer is rejected until tokens are
//! provisioned.
//!
//! ## Bind address
//!
//! Defaults to `127.0.0.1` (loopback only) so a bare `cargo run` of
//! this dev binary is not network-exposed. Override with
//! `HOMECORE_BIND=0.0.0.0:8123` for a LAN deployment (and provision
//! `HOMECORE_TOKENS` when you do).
//!
//!     cargo run -p homecore-api --bin homecore-api-server
//!     HOMECORE_TOKENS=secret curl -H "Authorization: Bearer secret" \
//!         http://127.0.0.1:8123/api/

use std::net::SocketAddr;

use homecore::HomeCore;
use homecore_api::{router, LongLivedTokenStore, SharedState, DEFAULT_PORT};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug,homecore_api=debug".into()),
        )
        .init();

    let homecore = HomeCore::new();

    // Token provisioning (HC-WS-08). An unset/empty env leaves the store
    // locked; there is no wildcard development mode.
    let tokens = LongLivedTokenStore::from_env();
    let n = tokens.len().await;
    if n == 0 {
        tracing::warn!(
            "HOMECORE_TOKENS is unset or empty — the API is locked and rejects all bearer tokens"
        );
    } else {
        tracing::info!(
            "LongLivedTokenStore provisioned with {n} bearer token(s) from HOMECORE_TOKENS"
        );
    }

    let state = SharedState::with_tokens(homecore, "Home", env!("CARGO_PKG_VERSION"), tokens);
    let app = router(state);

    // Default to loopback so `cargo run` is not network-exposed; allow
    // an explicit HOMECORE_BIND override for LAN deployments.
    let addr: SocketAddr = match std::env::var("HOMECORE_BIND") {
        Ok(v) if !v.trim().is_empty() => v.parse()?,
        _ => SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
    };
    tracing::info!("HOMECORE-API listening on http://{addr}  (HA-compat /api + /api/websocket)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
