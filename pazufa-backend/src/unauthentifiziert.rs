use crate::LTZFServer;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use tracing::debug;
use utoipa::{IntoResponses, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<Arc<LTZFServer>> {
    OpenApiRouter::new()
        .routes(routes!(ping))
        .routes(routes!(status))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
struct PingQueryParams {
    /// time of ping as milliseconds since 1.1.1970
    #[serde(rename = "t")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<f64>,
}
#[allow(unused, reason = "It is used to build response schemata for utoipa")]
#[derive(IntoResponses)]
enum PingResponse {
    #[response(status = 200)]
    Pong,
    #[response(status = 400)]
    BadReq,
    #[response(status = 500)]
    Error,
}
impl IntoResponse for PingResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Pong => StatusCode::OK.into_response(),
            Self::BadReq => StatusCode::BAD_REQUEST.into_response(),
            Self::Error => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to calculate one-way-time from given parameters".to_owned(),
            )
                .into_response(),
        }
    }
}
/// Ping - GET /ping
/// Endpoint for doing a roundtrip with a minimal response to see if the server is alive
/// # Errors
/// Can fail if the optional time argument is not a valid unix timestamp or otherwise malformed.
#[tracing::instrument(skip_all, fields(t=?query_params.t))]
#[utoipa::path(
    get,
    path="/api/v2/ping",
    tag="Unauthorisiert",
    description = "Just does a round trip with minimal response",
    params((
            "t" = Option<f64>, 
            Query, description="Time of ping as milliseconds since epoch", exclusive_minimum=0)),
    responses(PingResponse)
)]
async fn ping(
    Query(query_params): Query<PingQueryParams>,
    State(_api_impl): State<Arc<LTZFServer>>,
) -> PingResponse {
    if let Some(t) = query_params.t
        && let Ok(current_time) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
    {
        debug!(
            "Ping with one-way time: {} s",
            current_time.as_secs_f64() - t
        );
        PingResponse::Pong
    } else {
        PingResponse::Error
    }
}

/// Status - GET /status
/// Returns a string with some interesting infos about the versions of software and spec this
/// specific instance of the backend is running on
/// # Errors
/// Can fail if the body is built incorrectly. In the future will fail with a better / more
/// meaningful error message if the server is in some kind of failure state.
#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path="/api/v2/status",
    tag="Unauthorisiert",
    description = "Retrieves the current status of the API",
    responses(
        (status = 200,
            description = "API is running",
            body = serde_json::Value, content_type = "application/json",
        ),
        (status = 500, description = "API is not running")
    )
)]
async fn status(State(api_impl): State<Arc<LTZFServer>>) -> Response {
    (StatusCode::OK, api_impl.statistics.output().await).into_response()
}
