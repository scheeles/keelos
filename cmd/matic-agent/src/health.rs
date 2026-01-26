//! Health check endpoints for matic-agent
//!
//! Provides HTTP endpoints for:
//! - /healthz - Liveness check
//! - /readyz - Readiness check
//! - /metrics - Prometheus metrics

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::telemetry::SystemMetrics;

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

/// Readiness check response with component status
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub checks: ReadinessChecks,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadinessChecks {
    pub grpc_server: String,
    pub filesystem: String,
}

/// Shared state for health endpoints
pub struct HealthState {
    pub metrics: Arc<RwLock<SystemMetrics>>,
}

/// Liveness check handler
///
/// Returns 200 OK if the agent is alive
async fn healthz() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// Readiness check handler
///
/// Returns 200 OK if the agent is ready to serve traffic
async fn readyz() -> impl IntoResponse {
    // TODO: Implement actual readiness checks
    // - Check if gRPC server is running
    // - Check if required filesystems are mounted
    // - Check if agent can communicate with init process

    let checks = ReadinessChecks {
        grpc_server: "ready".to_string(),
        filesystem: "ready".to_string(),
    };

    Json(ReadinessResponse {
        status: "ready".to_string(),
        checks,
    })
}

/// Metrics endpoint handler
///
/// Returns Prometheus-compatible metrics
async fn metrics(State(state): State<Arc<HealthState>>) -> impl IntoResponse {
    let mut metrics = state.metrics.write().await;
    metrics.update();

    match metrics.export_metrics() {
        Ok(text) => (StatusCode::OK, text).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export metrics: {}", e),
        )
            .into_response(),
    }
}

/// Create health check router
pub fn create_health_router(state: Arc<HealthState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_healthz() {
        let metrics = Arc::new(RwLock::new(SystemMetrics::default()));
        let state = Arc::new(HealthState { metrics });
        let app = create_health_router(state);

        let response = app
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readyz() {
        let metrics = Arc::new(RwLock::new(SystemMetrics::default()));
        let state = Arc::new(HealthState { metrics });
        let app = create_health_router(state);

        let response = app
            .oneshot(Request::builder().uri("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
