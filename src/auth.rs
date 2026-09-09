use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Mutex, OnceLock}, time::{Duration as StdDuration, Instant}};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub user_id: i32,
    pub role: String,
    pub exp: i64,
}

pub struct AuthUser {
    pub user_id: i32,
    pub role: String,
}

static RATE_LIMIT: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();

pub fn allow_auth_attempt(key: &str) -> bool {
    let now = Instant::now();
    let cutoff = now - StdDuration::from_secs(60);
    let mut all = RATE_LIMIT.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    let attempts = all.entry(key.to_lowercase()).or_default();
    attempts.retain(|at| *at > cutoff);
    if attempts.len() >= 10 { return false; }
    attempts.push(now);
    true
}

fn jwt_secret() -> String {
    let secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set to a strong random value");
    if secret.trim().is_empty() {
        panic!("JWT_SECRET must not be empty");
    }
    secret
}

pub fn validate_jwt_secret() {
    let _ = jwt_secret();
}

pub fn create_token(user_id: i32, username: &str, role: &str) -> String {
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: username.to_string(),
        user_id,
        role: role.to_string(),
        exp: expiration,
    };

    let secret = jwt_secret();
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to create token")
}

pub fn verify_token(token: &str) -> Result<Claims, String> {
    let secret = jwt_secret();
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|e| e.to_string())?;

    Ok(token_data.claims)
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned)
            .or_else(|| parts.headers.get("Cookie").and_then(|h| h.to_str().ok()).and_then(|cookies| {
                cookies.split(';').find_map(|cookie| {
                    let (name, value) = cookie.trim().split_once('=')?;
                    (name == "access_token").then(|| format!("Bearer {value}"))
                })
            }))
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if !auth_header.starts_with("Bearer ") {
            return Err(StatusCode::UNAUTHORIZED);
        }

        let token = &auth_header[7..];
        let claims = verify_token(token).map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser {
            user_id: claims.user_id,
            role: claims.role,
        })
    }
}

pub fn hash_password(password: &str) -> String {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap_or_else(|_| "".to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}
