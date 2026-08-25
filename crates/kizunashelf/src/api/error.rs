use crate::contract::ErrorResponse;
use aide::OperationOutput;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

pub type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    pub(crate) fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    pub(crate) fn forbidden(message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.to_string(),
        }
    }

    pub(crate) fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.to_string(),
        }
    }

    pub(crate) fn bad_gateway(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.to_string(),
        }
    }

    pub(crate) fn gateway_timeout(message: &str) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.to_string(),
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Every API failure in all three runtimes funnels through here, and the
        // client only ever sees `message` (verbatim, in a toast). So this is the
        // one place an operator can learn that a request failed at all — without
        // it, a 500 leaves no trace anywhere on the server.
        //
        // A 5xx is ours to fix; a 4xx is the caller's, so it stays a warning.
        // 404 is demoted to debug because it is routine traffic, not a fault: a
        // grid of entities with missing covers would otherwise log one warning
        // per image.
        if self.status.is_server_error() {
            tracing::error!(
                status = self.status.as_u16(),
                message = %self.message,
                "request failed",
            );
        } else if self.status == StatusCode::NOT_FOUND {
            tracing::debug!(message = %self.message, "request not found");
        } else {
            tracing::warn!(
                status = self.status.as_u16(),
                message = %self.message,
                "request rejected",
            );
        }
        let mut response = (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response();
        response.extensions_mut().insert(ApiErrorLogged);
        response
    }
}

/// Marks a response as one that already logged its own reason above. The
/// router's fallback logging ([`super::router::log_unlogged_failures`]) uses it
/// to tell those apart from a request axum rejected *before* any handler ran — a
/// malformed query, a method the route doesn't take — which never builds an
/// `ApiError` and would otherwise leave no trace at all.
#[derive(Clone, Copy)]
pub(super) struct ApiErrorLogged;

impl OperationOutput for ApiError {
    type Inner = ErrorResponse;

    fn operation_response(
        ctx: &mut aide::generate::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        <Json<ErrorResponse> as OperationOutput>::operation_response(ctx, operation)
    }
}
