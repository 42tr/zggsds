use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::{allow_auth_attempt, hash_password};
use crate::models::department::Entity as Department;
use crate::models::registration_request::{self, Entity as RegistrationRequest};
use crate::models::user::{self, Entity as User};

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub department_id: i32,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub request_id: i32,
    pub status: String,
    pub assigned_approver_role: Option<String>,
    pub assigned_approver_id: Option<i32>,
}

fn normalize(s: &str) -> String {
    s.trim().to_string()
}

async fn pick_assignee(
    db: &DatabaseConnection,
    dept_id: i32,
) -> Result<(Option<String>, Option<i32>), String> {
    // 优先：该部门 dept_manager（按 id 最小的那个作为负责人）
    let mgrs = User::find()
        .filter(user::Column::Role.eq("dept_manager"))
        .filter(user::Column::DepartmentId.eq(Some(dept_id)))
        .filter(user::Column::DeletedAt.is_null())
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(mgr) = mgrs.first() {
        return Ok((Some("dept_manager".to_string()), Some(mgr.id)));
    }

    // 兜底：timekeeper
    let tks = User::find()
        .filter(user::Column::Role.eq("timekeeper"))
        .filter(user::Column::DeletedAt.is_null())
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(tk) = tks.first() {
        return Ok((Some("timekeeper".to_string()), Some(tk.id)));
    }

    // 再兜底：admin
    let adms = User::find()
        .filter(user::Column::Role.eq("admin"))
        .filter(user::Column::DeletedAt.is_null())
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(adm) = adms.first() {
        return Ok((Some("admin".to_string()), Some(adm.id)));
    }

    Ok((None, None))
}

pub async fn register(
    State(db): State<DatabaseConnection>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, String> {
    let username = normalize(&payload.username);
    let email = normalize(&payload.email);

    if !allow_auth_attempt(&format!("register:{}", email)) {
        return Err("注册请求过于频繁，请稍后再试".to_string());
    }

    if username.is_empty() {
        return Err("用户名不能为空".to_string());
    }
    if email.is_empty() {
        return Err("邮箱不能为空".to_string());
    }
    if payload.password.trim().len() < 6 {
        return Err("密码至少 6 位".to_string());
    }

    // 部门必须存在
    Department::find_by_id(payload.department_id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("部门不存在")?;

    // users 表中不可重复
    let exists_user = User::find()
        .filter(user::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(user::Column::Username.eq(&username))
                .add(user::Column::Email.eq(&email)),
        )
        .one(&db)
        .await
        .map_err(|e| e.to_string())?;
    if exists_user.is_some() {
        return Err("用户名或邮箱已存在".to_string());
    }

    // registration_requests 中不可有重复 pending
    let exists_req = RegistrationRequest::find()
        .filter(registration_request::Column::Status.eq("pending"))
        .filter(
            Condition::any()
                .add(registration_request::Column::Username.eq(&username))
                .add(registration_request::Column::Email.eq(&email)),
        )
        .one(&db)
        .await
        .map_err(|e| e.to_string())?;
    if exists_req.is_some() {
        return Err("已有待审批的注册申请，请勿重复提交".to_string());
    }

    let (assigned_role, assigned_id) = pick_assignee(&db, payload.department_id).await?;

    let now = Utc::now().naive_utc();
    let new_req = registration_request::ActiveModel {
        username: Set(username),
        email: Set(email),
        password_hash: Set(hash_password(payload.password.trim())),
        department_id: Set(payload.department_id),
        status: Set("pending".to_string()),
        assigned_approver_role: Set(assigned_role.clone()),
        assigned_approver_id: Set(assigned_id),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = new_req.insert(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(RegisterResponse {
        request_id: result.id,
        status: result.status,
        assigned_approver_role: result.assigned_approver_role,
        assigned_approver_id: result.assigned_approver_id,
    }))
}

#[derive(Deserialize)]
pub struct RegisterStatusQuery {
    pub request_id: Option<i32>,
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct RegisterStatusResponse {
    pub request_id: i32,
    pub status: String,
    pub reject_reason: Option<String>,
}

pub async fn register_status(
    State(db): State<DatabaseConnection>,
    Query(q): Query<RegisterStatusQuery>,
) -> Result<Json<RegisterStatusResponse>, String> {
    let req = if let (Some(id), Some(email)) = (q.request_id, q.email) {
        let email = normalize(&email);
        RegistrationRequest::find_by_id(id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .filter(|req| req.email == email)
    } else {
        None
    };

    let req = req.ok_or("未找到注册申请")?;

    Ok(Json(RegisterStatusResponse {
        request_id: req.id,
        status: req.status,
        reject_reason: req.reject_reason,
    }))
}
