use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub String);

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }
}

impl From<&str> for ApiError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}
