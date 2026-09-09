use axum::{extract::State, Json};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::models::registration_request::{self, Entity as RegistrationRequest};
use crate::models::user::{self, Entity as User};

#[derive(Serialize)]
pub struct RegistrationApprovalItem {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub department_id: i32,
    pub status: String,
    pub assigned_approver_role: Option<String>,
    pub assigned_approver_id: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
}

pub async fn list_pending_registrations(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<RegistrationApprovalItem>>, String> {
    // 可审批角色：admin/timekeeper/dept_manager
    if auth.role != "admin" && auth.role != "timekeeper" && auth.role != "dept_manager" {
        return Err("无权限".to_string());
    }

    // dept_manager 只能看自己部门申请；admin/timekeeper 全部可看
    let mut query =
        RegistrationRequest::find().filter(registration_request::Column::Status.eq("pending"));

    if auth.role == "dept_manager" {
        let me = User::find_by_id(auth.user_id)
            .filter(user::Column::DeletedAt.is_null())
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;

        let dept_id = me.department_id.ok_or("部门负责人必须归属部门")?;

        query = query.filter(registration_request::Column::DepartmentId.eq(dept_id));
    }

    // 兜底处理：如果申请单 assigned_approver_id 指向自己（admin/timekeeper）也应该能看到
    // 对 admin/timekeeper 其实不需要额外过滤；保留此注释说明规则

    let list = query
        .order_by_desc(registration_request::Column::CreatedAt)
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(
        list.into_iter()
            .map(|r| RegistrationApprovalItem {
                id: r.id,
                username: r.username,
                email: r.email,
                department_id: r.department_id,
                status: r.status,
                assigned_approver_role: r.assigned_approver_role,
                assigned_approver_id: r.assigned_approver_id,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct RejectPayload {
    pub reason: String,
}

pub async fn approve_registration(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "timekeeper" && auth.role != "dept_manager" {
        return Err("无权限".to_string());
    }

    let req = RegistrationRequest::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("申请不存在")?;

    if req.status != "pending" {
        return Err("该申请已处理".to_string());
    }

    // dept_manager 只能审批本部门
    if auth.role == "dept_manager" {
        let me = User::find_by_id(auth.user_id)
            .filter(user::Column::DeletedAt.is_null())
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let dept_id = me.department_id.ok_or("部门负责人必须归属部门")?;
        if req.department_id != dept_id {
            return Err("只能审批本部门的注册申请".to_string());
        }
    }

    // 再次确保 username/email 未被抢占
    let exists_user = User::find()
        .filter(user::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(user::Column::Username.eq(&req.username))
                .add(user::Column::Email.eq(&req.email)),
        )
        .one(&db)
        .await
        .map_err(|e| e.to_string())?;
    if exists_user.is_some() {
        return Err("用户名或邮箱已存在，无法通过".to_string());
    }

    let now = Utc::now().naive_utc();

    // 1) 创建真实用户（默认员工）
    let new_user = user::ActiveModel {
        username: Set(req.username.clone()),
        email: Set(req.email.clone()),
        password_hash: Set(req.password_hash.clone()),
        role: Set("employee".to_string()),
        department_id: Set(Some(req.department_id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    new_user.insert(&db).await.map_err(|e| e.to_string())?;

    // 2) 更新申请单
    let mut active: registration_request::ActiveModel = req.into();
    active.status = Set("approved".to_string());
    active.approved_by = Set(Some(auth.user_id));
    active.approved_at = Set(Some(now));
    active.updated_at = Set(now);
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("ok".to_string()))
}

pub async fn reject_registration(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<RejectPayload>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "timekeeper" && auth.role != "dept_manager" {
        return Err("无权限".to_string());
    }

    let req = RegistrationRequest::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("申请不存在")?;

    if req.status != "pending" {
        return Err("该申请已处理".to_string());
    }

    if auth.role == "dept_manager" {
        let me = User::find_by_id(auth.user_id)
            .filter(user::Column::DeletedAt.is_null())
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let dept_id = me.department_id.ok_or("部门负责人必须归属部门")?;
        if req.department_id != dept_id {
            return Err("只能审批本部门的注册申请".to_string());
        }
    }

    let now = Utc::now().naive_utc();

    let mut active: registration_request::ActiveModel = req.into();
    active.status = Set("rejected".to_string());
    active.rejected_by = Set(Some(auth.user_id));
    active.rejected_at = Set(Some(now));
    active.reject_reason = Set(Some(payload.reason));
    active.updated_at = Set(now);
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("ok".to_string()))
}
