use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/"]
struct Asset;

pub async fn serve_assets(Path(path): Path<String>) -> impl IntoResponse {
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };

    if path.starts_with("users")
        || path.starts_with("projects")
        || path.starts_with("time-entries")
        || path.starts_with("departments")
        || path.starts_with("auth")
    {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());

            (headers, content.data.into_response()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

pub async fn serve_index() -> impl IntoResponse {
    match Asset::get("index.html") {
        Some(content) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "text/html".parse().unwrap());
            (headers, content.data.into_response()).into_response()
        }
        None => (StatusCode::INTERNAL_SERVER_ERROR, "Index not found").into_response(),
    }
}
