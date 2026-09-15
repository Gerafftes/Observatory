//! Route composition for the sensing-server binary.

use axum::{routing::get, Router};

use super::{
    api_info, calibration_routes, health_metrics, health_ready, mmwave_routes, model_routes,
    observatory_routes, recording_routes, sensing_routes, system_routes, training_routes,
    SharedState,
};

/// Browser-facing routes extracted from the binary's top-level router.
pub(crate) fn api_routes() -> Router<SharedState> {
    Router::new()
        .merge(api_observability_routes())
        .merge(calibration_routes::routes())
        .merge(mmwave_routes::routes())
        .merge(model_routes::routes())
        .merge(observatory_routes::routes())
        .merge(recording_routes::routes())
        .merge(sensing_routes::routes())
        .merge(system_routes::routes())
        .merge(training_routes::routes())
}

/// Read-only API routes used for service discovery and health monitoring.
pub(crate) fn api_observability_routes() -> Router<SharedState> {
    Router::new()
        .route("/api/v1/info", get(api_info))
        .route("/api/v1/status", get(health_ready))
        .route("/api/v1/metrics", get(health_metrics))
}
