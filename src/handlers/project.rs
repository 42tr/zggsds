use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::models::project::{self, Entity as Project};
use crate::models::time_entry::{self, Entity as TimeEntry};

#[derive(Deserialize)]
pub struct CreateProject {
    pub project_no: Option<String>,
    pub name: String,
    pub code: Option<String>,
    pub project_type: String,
    pub status: Option<String>,
    pub cycle: Option<String>,
    pub owner: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub project_no: Option<String>,
    pub name: Option<String>,
    pub code: Option<String>,
    pub project_type: Option<String>,
    pub status: Option<String>,
    pub cycle: Option<String>,
    pub owner: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: i32,
    pub project_no: Option<String>,
    pub name: String,
    pub code: Option<String>,
    pub project_type: String,
    pub status: String,
    pub cycle: Option<String>,
    pub owner: Option<String>,
    pub description: Option<String>,
}

fn is_project_manager(role: &str) -> bool {
    role == "admin" || role == "timekeeper"
}

pub async fn create_project(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<CreateProject>,
) -> Result<Json<ProjectResponse>, String> {
    if !is_project_manager(&auth.role) {
        return Err("无权限：仅工时管理员或系统管理员可创建项目".to_string());
    }

    let now = Utc::now().naive_utc();
    let new_project = project::ActiveModel {
        project_no: Set(payload.project_no),
        name: Set(payload.name),
        code: Set(payload.code),
        project_type: Set(payload.project_type),
        status: Set(payload.status.unwrap_or_else(|| "进行中".to_string())),
        cycle: Set(payload.cycle),
        owner: Set(payload.owner),
        description: Set(payload.description),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = new_project.insert(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(to_response(result)))
}

pub async fn list_projects(
    _auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<ProjectResponse>>, String> {
    let projects = Project::find().all(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(projects.into_iter().map(to_response).collect()))
}

pub async fn list_active_projects(
    _auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<ProjectResponse>>, String> {
    let projects = Project::find()
        .filter(project::Column::Status.eq("进行中"))
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(projects.into_iter().map(to_response).collect()))
}

pub async fn update_project(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateProject>,
) -> Result<Json<ProjectResponse>, String> {
    if !is_project_manager(&auth.role) {
        return Err("无权限：仅工时管理员或系统管理员可修改项目".to_string());
    }

    let project = Project::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("项目不存在")?;

    let mut active: project::ActiveModel = project.into();
    if let Some(v) = payload.project_no {
        active.project_no = Set(Some(v));
    }
    if let Some(v) = payload.name {
        active.name = Set(v);
    }
    if let Some(v) = payload.code {
        active.code = Set(Some(v));
    }
    if let Some(v) = payload.project_type {
        active.project_type = Set(v);
    }
    if let Some(v) = payload.status {
        active.status = Set(v);
    }
    if let Some(v) = payload.cycle {
        active.cycle = Set(Some(v));
    }
    if let Some(v) = payload.owner {
        active.owner = Set(Some(v));
    }
    if let Some(v) = payload.description {
        active.description = Set(Some(v));
    }
    active.updated_at = Set(Utc::now().naive_utc());

    let result = active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(to_response(result)))
}

pub async fn delete_project(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Path(id): Path<i32>,
) -> Result<Json<String>, String> {
    if !is_project_manager(&auth.role) {
        return Err("无权限：仅工时管理员或系统管理员可删除项目".to_string());
    }

    // 被工时引用则不允许删除（避免外键失败）
    let ref_count = TimeEntry::find()
        .filter(time_entry::Column::ProjectId.eq(Some(id)))
        .count(&db)
        .await
        .map_err(|e| e.to_string())?;
    if ref_count > 0 {
        return Err(format!(
            "该项目已被 {} 条工时记录引用，无法删除。请先删除/修改相关工时记录后再试",
            ref_count
        ));
    }

    let project = Project::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("项目不存在")?;

    let active: project::ActiveModel = project.into();
    active.delete(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("项目已删除".to_string()))
}

fn to_response(p: project::Model) -> ProjectResponse {
    ProjectResponse {
        id: p.id,
        project_no: p.project_no,
        name: p.name,
        code: p.code,
        project_type: p.project_type,
        status: p.status,
        cycle: p.cycle,
        owner: p.owner,
        description: p.description,
    }
}
