//! Explicit HTTP contract for unavailable browser-triggered training.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};

pub(crate) fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/api/v1/train/status", get(train_status))
        .route("/api/v1/train/start", post(train_unavailable))
        .route("/api/v1/train/stop", post(train_unavailable))
        .route("/api/v1/train/pretrain", post(train_unavailable))
        .route("/api/v1/train/lora", post(train_unavailable))
        .route("/ws/train/progress", get(train_unavailable))
}

pub(crate) fn training_status_payload() -> serde_json::Value {
    serde_json::json!({
        "active": false,
        "status": "unavailable",
        "phase": "unavailable",
        "capabilities": {
            "start": false,
            "pretrain": false,
            "lora": false,
            "progress_websocket": false,
        },
        "message": "HTTP training is unavailable: the retained API implementation does not support the active raw-csi-v1 recording contract.",
    })
}

async fn train_status() -> Json<serde_json::Value> {
    Json(training_status_payload())
}

async fn train_unavailable() -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(training_status_payload()))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    use super::*;

    async fn response(method: &str, path: &str) -> axum::response::Response {
        routes::<()>()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("training contract request"),
            )
            .await
            .expect("training contract response")
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("training contract body");
        serde_json::from_slice(&bytes).expect("training contract JSON")
    }

    #[tokio::test]
    async fn status_cannot_claim_an_active_run() {
        let response = response("GET", "/api/v1/train/status").await;
        assert_eq!(response.status(), StatusCode::OK);
        let status = json_body(response).await;
        assert_eq!(status["active"], false);
        assert_eq!(status["status"], "unavailable");
        assert_eq!(status["capabilities"]["start"], false);
        assert_eq!(status["capabilities"]["pretrain"], false);
        assert_eq!(status["capabilities"]["lora"], false);
        assert_eq!(status["capabilities"]["progress_websocket"], false);
    }

    #[tokio::test]
    async fn every_training_action_is_explicitly_unavailable() {
        for (method, path) in [
            ("POST", "/api/v1/train/start"),
            ("POST", "/api/v1/train/stop"),
            ("POST", "/api/v1/train/pretrain"),
            ("POST", "/api/v1/train/lora"),
            ("GET", "/ws/train/progress"),
        ] {
            let response = response(method, path).await;
            assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
            let status = json_body(response).await;
            assert_eq!(status["active"], false, "{path}");
            assert_eq!(status["status"], "unavailable", "{path}");
        }
    }
}
