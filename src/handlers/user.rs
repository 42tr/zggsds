use crate::auth::{hash_password, AuthUser};
use crate::error::ApiError;
use crate::models::user::{self, Entity as User};
use axum::http::StatusCode;
use axum::{extract::State, Json};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: String,
    pub department_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateUser {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub role: Option<String>,
    pub department_id: Option<Option<i32>>,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub role: String,
    pub department_id: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct UserOption {
    pub id: i32,
    pub username: String,
}

// Business pages only need display names, not emails or account administration data.
pub async fn list_user_options(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<UserOption>>, ApiError> {
    let mut query = User::find().filter(user::Column::DeletedAt.is_null());
    if !matches!(
        auth.role.as_str(),
        "admin" | "timekeeper" | "dept_manager" | "project_manager"
    ) {
        query = query.filter(user::Column::Id.eq(auth.user_id));
    }
    let users = query
        .all(&db)
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "查询用户失败".into()))?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserOption {
                id: u.id,
                username: u.username,
            })
            .collect(),
    ))
}

pub async fn create_user(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<UserResponse>, ApiError> {
    if auth.role != "admin" {
        return Err(ApiError(StatusCode::FORBIDDEN, "无权限".into()));
    }
    let password_hash = hash_password(&payload.password);

    let new_user = user::ActiveModel {
        username: Set(payload.username),
        email: Set(payload.email),
        password_hash: Set(password_hash),
        role: Set(payload.role),
        department_id: Set(payload.department_id),
        ..Default::default()
    };

    let result = new_user.insert(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(UserResponse {
        id: result.id,
        username: result.username,
        email: result.email,
        role: result.role,
        department_id: result.department_id,
    }))
}

pub async fn list_users(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<UserResponse>>, ApiError> {
    if auth.role != "admin" && auth.role != "timekeeper" && auth.role != "dept_manager" {
        return Err(ApiError(StatusCode::FORBIDDEN, "无权限".into()));
    }
    let users = User::find()
        .filter(user::Column::DeletedAt.is_null())
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    let response: Vec<UserResponse> = users
        .into_iter()
        .map(|u| UserResponse {
            id: u.id,
            username: u.username,
            email: u.email,
            role: u.role,
            department_id: u.department_id,
        })
        .collect();

    Ok(Json(response))
}

pub async fn delete_user(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, ApiError> {
    if auth.role != "admin" {
        return Err(ApiError(StatusCode::FORBIDDEN, "无权限".into()));
    }
    let user = User::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("User not found")?;

    let mut user: user::ActiveModel = user.into();
    user.deleted_at = Set(Some(Utc::now().naive_utc()));
    user.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("User deleted successfully".to_string()))
}

pub async fn update_user(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateUser>,
) -> Result<Json<UserResponse>, ApiError> {
    if auth.role != "admin" {
        return Err(ApiError(StatusCode::FORBIDDEN, "无权限".into()));
    }
    let user = User::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("User not found")?;

    let mut user: user::ActiveModel = user.into();

    if let Some(username) = payload.username {
        user.username = Set(username);
    }
    if let Some(email) = payload.email {
        user.email = Set(email);
    }
    if let Some(password) = payload.password {
        user.password_hash = Set(hash_password(&password));
    }
    if let Some(role) = payload.role {
        user.role = Set(role);
    }
    if let Some(department_id) = payload.department_id {
        user.department_id = Set(department_id);
    }

    user.updated_at = Set(Utc::now().naive_utc());
    let result = user.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(UserResponse {
        id: result.id,
        username: result.username,
        email: result.email,
        role: result.role,
        department_id: result.department_id,
    }))
}
