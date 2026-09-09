use crate::error::ApiError;
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::{allow_auth_attempt, create_token, hash_password, verify_password, AuthUser};
use crate::models::user::{self, Entity as User};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user_id: i32,
    pub username: String,
    pub role: String,
}

pub async fn login(
    State(db): State<DatabaseConnection>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !allow_auth_attempt(&format!("login:{}", payload.username.trim())) {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "登录尝试过于频繁，请稍后再试".into(),
        ));
    }
    let user = User::find()
        .filter(crate::models::user::Column::Username.eq(&payload.username))
        .filter(crate::models::user::Column::DeletedAt.is_null())
        .one(&db)
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "登录失败".into()))?
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "用户名或密码错误".into()))?;

    if !verify_password(&payload.password, &user.password_hash) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "用户名或密码错误".into(),
        ));
    }

    let token = create_token(user.id, &user.username, &user.role);

    Ok((
        [(
            header::SET_COOKIE,
            HeaderValue::from_str(&format!(
                "access_token={token}; HttpOnly; SameSite=Lax; Path=/"
            ))
            .unwrap(),
        )],
        Json(LoginResponse {
            user_id: user.id,
            username: user.username,
            role: user.role,
        }),
    ))
}

pub async fn logout() -> impl IntoResponse {
    (
        [(
            header::SET_COOKIE,
            "access_token=; Max-Age=0; HttpOnly; SameSite=Lax; Path=/",
        )],
        Json("ok"),
    )
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

pub async fn change_password(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<String>, ApiError> {
    if payload.new_password.trim().len() < 6 {
        return Err("新密码至少 6 位".into());
    }

    let user_model = User::find_by_id(auth.user_id)
        .filter(user::Column::DeletedAt.is_null())
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("User not found")?;

    if !verify_password(&payload.old_password, &user_model.password_hash) {
        return Err("旧密码不正确".into());
    }

    // 避免新旧密码相同（可选，但挺合理）
    if verify_password(&payload.new_password, &user_model.password_hash) {
        return Err("新密码不能与旧密码相同".into());
    }

    let mut active: user::ActiveModel = user_model.into();
    active.password_hash = Set(hash_password(&payload.new_password));
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("密码修改成功，请重新登录".to_string()))
}
