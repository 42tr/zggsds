use axum::{extract::State, Json};
use sea_orm::{DatabaseConnection, EntityTrait, PaginatorTrait, QueryOrder};

use crate::auth::AuthUser;
use crate::models::department::{self, Entity as Department};
use crate::models::registration_request::{self, Entity as RegistrationRequest};
use serde::Serialize;

#[derive(Serialize)]
pub struct DepartmentResponse {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
}

// 公开：注册页使用（不需要登录）
pub async fn list_departments_flat_public(
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<DepartmentResponse>>, String> {
    let depts = Department::find()
        .order_by_asc(department::Column::Id)
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(
        depts
            .into_iter()
            .map(|d| DepartmentResponse {
                id: d.id,
                name: d.name,
                parent_id: d.parent_id,
            })
            .collect(),
    ))
}

// 下面这些是原有文件内容，保留现有 API 行为
use crate::models::user::{self, Entity as User};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, QueryFilter, Set};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateDepartment {
    pub name: String,
    pub parent_id: Option<i32>,
}

#[derive(Serialize)]
pub struct DepartmentNode {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
    pub children: Vec<DepartmentNode>,
}

#[derive(Serialize)]
pub struct DepartmentResponseAuth {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
}

pub async fn create_department(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<CreateDepartment>,
) -> Result<Json<DepartmentResponseAuth>, String> {
    if auth.role != "admin" && auth.role != "timekeeper" {
        return Err("无权限".to_string());
    }
    let new_dept = department::ActiveModel {
        name: Set(payload.name),
        parent_id: Set(payload.parent_id),
        ..Default::default()
    };

    let result = new_dept.insert(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(DepartmentResponseAuth {
        id: result.id,
        name: result.name,
        parent_id: result.parent_id,
    }))
}

pub async fn list_departments(
    _auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<DepartmentNode>>, String> {
    let depts = Department::find()
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    let tree = build_department_tree(&depts, None);
    Ok(Json(tree))
}

pub async fn list_departments_flat(
    _auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<DepartmentResponseAuth>>, String> {
    let depts = Department::find()
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    let response: Vec<DepartmentResponseAuth> = depts
        .into_iter()
        .map(|d| DepartmentResponseAuth {
            id: d.id,
            name: d.name,
            parent_id: d.parent_id,
        })
        .collect();

    Ok(Json(response))
}

fn build_department_tree(
    depts: &[department::Model],
    parent_id: Option<i32>,
) -> Vec<DepartmentNode> {
    depts
        .iter()
        .filter(|d| d.parent_id == parent_id)
        .map(|dept| DepartmentNode {
            id: dept.id,
            name: dept.name.clone(),
            parent_id: dept.parent_id,
            children: build_department_tree(depts, Some(dept.id)),
        })
        .collect()
}

pub async fn update_department(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<CreateDepartment>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "timekeeper" {
        return Err("无权限".to_string());
    }
    let dept = Department::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Department not found")?;

    let mut dept: department::ActiveModel = dept.into();
    dept.name = Set(payload.name);
    dept.parent_id = Set(payload.parent_id);
    dept.updated_at = Set(Utc::now().naive_utc());
    dept.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("Department updated".to_string()))
}

pub async fn delete_department(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "timekeeper" {
        return Err("无权限".to_string());
    }
    let dept = Department::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Department not found")?;

    // 如果有待审批注册申请引用该部门，不允许删除（避免外键失败）
    let pending_req_count = RegistrationRequest::find()
        .filter(registration_request::Column::DepartmentId.eq(id))
        .filter(registration_request::Column::Status.eq("pending"))
        .count(&db)
        .await
        .map_err(|e| e.to_string())?;
    if pending_req_count > 0 {
        return Err(format!(
            "该部门存在 {} 条待审批注册申请，无法删除。请先处理这些申请后再试",
            pending_req_count
        ));
    }

    let child_departments = Department::find()
        .filter(department::Column::ParentId.eq(Some(id)))
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    for child in child_departments {
        let mut child_model: department::ActiveModel = child.into();
        child_model.parent_id = Set(None);
        child_model.update(&db).await.map_err(|e| e.to_string())?;
    }

    let users_with_dept = User::find()
        .filter(user::Column::DepartmentId.eq(Some(id)))
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    for user in users_with_dept {
        let mut user_model: user::ActiveModel = user.into();
        user_model.department_id = Set(None);
        user_model.update(&db).await.map_err(|e| e.to_string())?;
    }

    let dept: department::ActiveModel = dept.into();
    dept.delete(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("Department deleted successfully".to_string()))
}
