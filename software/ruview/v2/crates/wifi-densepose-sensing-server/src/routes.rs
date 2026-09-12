//! Route composition for the sensing-server binary.

use axum::{routing::get, Router};

use super::{api_info, health_metrics, health_ready, SharedState};

/// Read-only API routes used for service discovery and health monitoring.
pub(crate) fn api_observability_routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/info", get(api_info))
        .route("/api/v1/status", get(health_ready))
        .route("/api/v1/metrics", get(health_metrics))
}
