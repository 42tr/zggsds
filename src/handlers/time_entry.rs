use crate::auth::AuthUser;
use crate::models::project::{self, Entity as Project};
use crate::models::time_entry::{self, Entity as TimeEntry};
use crate::models::user::{self, Entity as User};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use chrono::{Datelike, NaiveDate, Utc, Weekday};
use rust_xlsxwriter::{Format, FormatAlign, Workbook};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct CreateTimeEntry {
    pub project_id: Option<i32>,
    pub work_date: NaiveDate,
    pub hours: f64,
    pub description: String,
}

#[derive(Deserialize)]
pub struct BatchTimeEntryItem {
    pub project_id: Option<i32>,
    pub work_date: NaiveDate,
    pub hours: f64,
    pub description: String,
}

#[derive(Deserialize)]
pub struct CreateTimeEntriesBatch {
    pub entries: Vec<BatchTimeEntryItem>,
}

#[derive(Deserialize)]
pub struct UpdateStatus {
    pub status: String,
}

#[derive(Deserialize)]
pub struct UpdateTimeEntry {
    pub project_id: Option<i32>,
    pub work_date: NaiveDate,
    pub hours: f64,
    pub description: String,
}

#[derive(Deserialize)]
pub struct TimeEntryQuery {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub user_id: Option<i32>,
    pub project_id: Option<i32>,
    pub month: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Serialize)]
pub struct TimeEntryResponse {
    pub id: i32,
    pub user_id: i32,
    pub project_id: Option<i32>,
    pub work_date: NaiveDate,
    pub hours: f64,
    pub description: String,
    pub work_type: String,
    pub status: String,
    pub approved_by: Option<i32>,
    pub second_status: Option<String>,
    pub second_approved_by: Option<i32>,
    pub edit_allowed: i32,
    pub edit_requested: i32,
    pub modification_log: Option<String>,
}

#[derive(Serialize)]
pub struct TimeEntryListResponse {
    pub entries: Vec<TimeEntryResponse>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
    pub total_pages: u64,
}

#[derive(Serialize)]
pub struct BatchCreateTimeEntryResponse {
    pub created: Vec<TimeEntryResponse>,
    pub created_count: usize,
}

fn is_workday(date: &NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn needs_second_approval(project_type: &str) -> bool {
    matches!(project_type, "交付" | "研发" | "售前")
}

fn has_project_owner(owner: Option<&str>) -> bool {
    owner.map(|v| !v.trim().is_empty()).unwrap_or(false)
}

fn is_project_owner(owner: Option<&str>, username: &str) -> bool {
    owner
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v == username)
        .unwrap_or(false)
}

fn project_requires_second_approval(project: &project::Model) -> bool {
    needs_second_approval(&project.project_type) && has_project_owner(project.owner.as_deref())
}

fn validate_hours(hours: f64) -> Result<(), String> {
    if !hours.is_finite() || hours <= 0.0 {
        return Err("工时必须大于0".to_string());
    }
    Ok(())
}

async fn find_active_project(
    db: &DatabaseConnection,
    project_id: Option<i32>,
) -> Result<Option<project::Model>, String> {
    if let Some(pid) = project_id {
        let p = Project::find_by_id(pid)
            .one(db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("项目不存在")?;
        if p.status != "进行中" {
            return Err("只能为进行中的项目填写工时".to_string());
        }
        Ok(Some(p))
    } else {
        Ok(None)
    }
}

async fn current_username_if_dept_manager(
    db: &DatabaseConnection,
    auth: &AuthUser,
) -> Result<Option<String>, String> {
    if auth.role != "dept_manager" {
        return Ok(None);
    }
    let me = User::find_by_id(auth.user_id)
        .one(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("用户不存在")?;
    Ok(Some(me.username))
}

fn resolve_initial_status(
    auth: &AuthUser,
    project: Option<&project::Model>,
    username: Option<&str>,
) -> &'static str {
    if auth.role != "dept_manager" {
        return "pending";
    }
    if let Some(p) = project {
        if project_requires_second_approval(p) {
            if let Some(uname) = username {
                if is_project_owner(p.owner.as_deref(), uname) {
                    "approved"
                } else {
                    "dept_approved"
                }
            } else {
                "dept_approved"
            }
        } else {
            "approved"
        }
    } else {
        "approved"
    }
}

async fn day_total_hours(
    db: &DatabaseConnection,
    user_id: i32,
    work_date: NaiveDate,
    exclude_entry_id: Option<i32>,
) -> Result<f64, String> {
    let mut query = TimeEntry::find()
        .filter(time_entry::Column::UserId.eq(user_id))
        .filter(time_entry::Column::WorkDate.eq(work_date))
        .filter(time_entry::Column::Status.ne("rejected"));
    if let Some(id) = exclude_entry_id {
        query = query.filter(time_entry::Column::Id.ne(id));
    }
    let entries = query.all(db).await.map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(|e| e.hours).sum())
}

async fn ensure_workday_daily_total(
    db: &DatabaseConnection,
    user_id: i32,
    work_date: NaiveDate,
    hours_to_apply: f64,
    exclude_entry_id: Option<i32>,
) -> Result<(), String> {
    if !is_workday(&work_date) {
        return Ok(());
    }
    let existing = day_total_hours(db, user_id, work_date, exclude_entry_id).await?;
    let total = existing + hours_to_apply;
    if total + 1e-9 < 8.0 {
        return Err(format!(
            "{} 工作日总工时不能低于8小时（当前合计 {:.1}）",
            work_date, total
        ));
    }
    Ok(())
}

fn to_response(e: time_entry::Model) -> TimeEntryResponse {
    TimeEntryResponse {
        id: e.id,
        user_id: e.user_id,
        project_id: e.project_id,
        work_date: e.work_date,
        hours: e.hours,
        description: e.description,
        work_type: e.work_type,
        status: e.status,
        approved_by: e.approved_by,
        second_status: e.second_status,
        second_approved_by: e.second_approved_by,
        edit_allowed: e.edit_allowed,
        edit_requested: e.edit_requested,
        modification_log: e.modification_log,
    }
}

pub async fn create_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<CreateTimeEntry>,
) -> Result<Json<TimeEntryResponse>, String> {
    validate_hours(payload.hours)?;
    let project = find_active_project(&db, payload.project_id).await?;
    let username = current_username_if_dept_manager(&db, &auth).await?;
    let initial_status = resolve_initial_status(&auth, project.as_ref(), username.as_deref());

    let now = Utc::now().naive_utc();
    let new_entry = time_entry::ActiveModel {
        user_id: Set(auth.user_id),
        project_id: Set(payload.project_id),
        work_date: Set(payload.work_date),
        hours: Set(payload.hours),
        description: Set(payload.description.trim().to_string()),
        work_type: Set("development".to_string()),
        status: Set(initial_status.to_string()),
        edit_allowed: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = new_entry.insert(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(to_response(result)))
}

pub async fn create_time_entries_batch(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Json(payload): Json<CreateTimeEntriesBatch>,
) -> Result<Json<BatchCreateTimeEntryResponse>, String> {
    if payload.entries.is_empty() {
        return Err("批量提交不能为空".to_string());
    }
    if payload.entries.len() > 500 {
        return Err("单次最多提交500条工时".to_string());
    }

    let username = current_username_if_dept_manager(&db, &auth).await?;
    let mut project_cache: HashMap<i32, project::Model> = HashMap::new();
    let mut incoming_day_total: HashMap<NaiveDate, f64> = HashMap::new();

    for (idx, entry) in payload.entries.iter().enumerate() {
        let row = idx + 1;
        validate_hours(entry.hours).map_err(|e| format!("第{}条: {}", row, e))?;
        if entry.description.trim().is_empty() {
            return Err(format!("第{}条: 描述不能为空", row));
        }

        if let Some(pid) = entry.project_id {
            if !project_cache.contains_key(&pid) {
                let p = find_active_project(&db, Some(pid))
                    .await
                    .map_err(|e| format!("第{}条: {}", row, e))?;
                if let Some(project) = p {
                    project_cache.insert(pid, project);
                }
            }
        }

        *incoming_day_total.entry(entry.work_date).or_insert(0.0) += entry.hours;
    }

    for (date, incoming_total) in &incoming_day_total {
        if !is_workday(date) {
            continue;
        }
        let existing_total = day_total_hours(&db, auth.user_id, *date, None).await?;
        let final_total = existing_total + *incoming_total;
        if final_total + 1e-9 < 8.0 {
            return Err(format!(
                "{} 工作日总工时不能低于8小时（当前合计 {:.1}）",
                date, final_total
            ));
        }
    }

    let txn = db.begin().await.map_err(|e| e.to_string())?;
    let now = Utc::now().naive_utc();
    let mut created = Vec::with_capacity(payload.entries.len());

    for entry in payload.entries {
        let project = entry.project_id.and_then(|pid| project_cache.get(&pid));
        let initial_status = resolve_initial_status(&auth, project, username.as_deref());
        let new_entry = time_entry::ActiveModel {
            user_id: Set(auth.user_id),
            project_id: Set(entry.project_id),
            work_date: Set(entry.work_date),
            hours: Set(entry.hours),
            description: Set(entry.description.trim().to_string()),
            work_type: Set("development".to_string()),
            status: Set(initial_status.to_string()),
            edit_allowed: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = new_entry.insert(&txn).await.map_err(|e| e.to_string())?;
        created.push(to_response(result));
    }

    txn.commit().await.map_err(|e| e.to_string())?;
    Ok(Json(BatchCreateTimeEntryResponse {
        created_count: created.len(),
        created,
    }))
}

pub async fn list_time_entries(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Query(params): Query<TimeEntryQuery>,
) -> Result<Json<TimeEntryListResponse>, String> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(10).clamp(1, 100);

    let mut query = build_base_query(&auth, &db).await?;

    if let Some(start) = params.start_date {
        query = query.filter(time_entry::Column::WorkDate.gte(start));
    }
    if let Some(end) = params.end_date {
        query = query.filter(time_entry::Column::WorkDate.lte(end));
    }
    if let Some(month_str) = params.month {
        // 格式 "2026-02"
        if let Ok(d) = NaiveDate::parse_from_str(&format!("{}-01", month_str), "%Y-%m-%d") {
            let end = last_day_of_month(d);
            query = query.filter(time_entry::Column::WorkDate.gte(d));
            query = query.filter(time_entry::Column::WorkDate.lte(end));
        }
    }
    if let Some(user_id) = params.user_id {
        if auth.role == "admin" || auth.role == "timekeeper" || auth.role == "dept_manager" {
            query = query.filter(time_entry::Column::UserId.eq(user_id));
        }
    }
    if let Some(project_id) = params.project_id {
        query = query.filter(time_entry::Column::ProjectId.eq(project_id));
    }

    let total = query.clone().count(&db).await.map_err(|e| e.to_string())?;
    let total_pages = (total + page_size - 1) / page_size;

    let entries = query
        .order_by_desc(time_entry::Column::WorkDate)
        .paginate(&db, page_size)
        .fetch_page(page - 1)
        .await
        .map_err(|e| e.to_string())?;

    Ok(Json(TimeEntryListResponse {
        entries: entries.into_iter().map(to_response).collect(),
        total,
        page,
        page_size,
        total_pages,
    }))
}

// 一审：部门负责人审批（dept_manager 或 admin）
pub async fn approve_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateStatus>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "dept_manager" {
        return Err("无权限：仅部门负责人或管理员可进行一审".to_string());
    }

    if payload.status != "approved" && payload.status != "rejected" {
        return Err("非法审批状态".to_string());
    }

    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.status != "pending" {
        return Err("该工时记录已审批".to_string());
    }

    // dept_manager 只能审批本部门员工，且不能审批自己
    if auth.role == "dept_manager" {
        if entry.user_id == auth.user_id {
            return Err("不能审批自己的工时".to_string());
        }
        let current_user = User::find_by_id(auth.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let manager_dept_id = current_user.department_id.ok_or("部门负责人必须归属部门")?;
        let entry_user = User::find_by_id(entry.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("工时归属用户不存在")?;
        if entry_user.department_id != Some(manager_dept_id) {
            return Err("只能审批本部门员工的工时".to_string());
        }
    }

    let now = Utc::now().naive_utc();

    // 判断是否需要二次审批
    // 如果部门负责人同时也是该项目的负责人（project.owner == 审批人用户名），则跳过二审
    let requires_second = if let Some(pid) = entry.project_id {
        if let Ok(Some(p)) = Project::find_by_id(pid).one(&db).await {
            if project_requires_second_approval(&p) {
                // 检查审批人是否同时是项目负责人
                let approver = User::find_by_id(auth.user_id)
                    .one(&db)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("用户不存在")?;
                let approver_is_project_owner =
                    is_project_owner(p.owner.as_deref(), &approver.username);
                !approver_is_project_owner
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let mut active: time_entry::ActiveModel = entry.into();

    if payload.status == "approved" && requires_second {
        // 一审通过，进入等待二审状态
        active.status = Set("dept_approved".to_string());
    } else {
        active.status = Set(payload.status.clone());
    }
    active.approved_by = Set(Some(auth.user_id));
    active.approved_at = Set(Some(now));
    active.updated_at = Set(now);
    active.update(&db).await.map_err(|e| e.to_string())?;

    if payload.status == "approved" && requires_second {
        Ok(Json("一审通过，等待项目负责人二次审批".to_string()))
    } else {
        Ok(Json("审批完成".to_string()))
    }
}

// 二审：项目负责人审批（project_manager、dept_manager 兼任项目负责人，或 admin）
pub async fn second_approve_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateStatus>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "project_manager" && auth.role != "dept_manager" {
        return Err("无权限：仅项目负责人或管理员可进行二审".to_string());
    }

    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.status != "dept_approved" {
        return Err("该工时记录尚未通过部门负责人审批".to_string());
    }

    // dept_manager 做二审时，必须是该项目的 owner
    if auth.role == "dept_manager" {
        let approver = User::find_by_id(auth.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let is_project_owner = if let Some(pid) = entry.project_id {
            if let Ok(Some(p)) = Project::find_by_id(pid).one(&db).await {
                is_project_owner(p.owner.as_deref(), &approver.username)
            } else {
                false
            }
        } else {
            false
        };
        if !is_project_owner {
            return Err("无权限：只能对自己负责的项目进行二审".to_string());
        }
    }

    let now = Utc::now().naive_utc();
    let final_status = if payload.status == "approved" {
        "approved"
    } else {
        "rejected"
    };

    let mut active: time_entry::ActiveModel = entry.into();
    active.status = Set(final_status.to_string());
    active.second_approved_by = Set(Some(auth.user_id));
    active.second_approved_at = Set(Some(now));
    active.second_status = Set(Some(payload.status));
    active.updated_at = Set(now);
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("二审完成".to_string()))
}

// 员工申请修改工时
pub async fn request_edit_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.user_id != auth.user_id {
        return Err("只能申请修改自己的工时".to_string());
    }

    if entry.status == "pending" {
        return Err("待审批的工时可直接修改，无需申请".to_string());
    }
    if entry.status == "rejected" {
        return Err("已拒绝的工时可直接修改，无需申请".to_string());
    }

    // 只允许申请修改当月工时
    let today = Utc::now().date_naive();
    if entry.work_date.year() != today.year() || entry.work_date.month() != today.month() {
        return Err("只能申请修改当月工时".to_string());
    }

    let mut active: time_entry::ActiveModel = entry.into();
    active.edit_requested = Set(1);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("已提交修改申请，等待部门负责人审批".to_string()))
}

// 部门负责人放开修改权限（仅限当月）
pub async fn allow_edit_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "dept_manager" {
        return Err("无权限：仅部门负责人或管理员可放开修改权限".to_string());
    }

    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    // 仅限当月工时
    let now = Utc::now().date_naive();
    if entry.work_date.year() != now.year() || entry.work_date.month() != now.month() {
        return Err("只能放开当月工时的修改权限".to_string());
    }

    // dept_manager 只能放开本部门员工的工时，不能操作自己的
    if auth.role == "dept_manager" {
        if entry.user_id == auth.user_id {
            return Err("不能操作自己的工时".to_string());
        }
        let current_user = User::find_by_id(auth.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let manager_dept_id = current_user.department_id.ok_or("部门负责人必须归属部门")?;
        let entry_user = User::find_by_id(entry.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("工时归属用户不存在")?;
        if entry_user.department_id != Some(manager_dept_id) {
            return Err("只能操作本部门员工的工时".to_string());
        }
    }

    let mut active: time_entry::ActiveModel = entry.into();
    active.edit_allowed = Set(1);
    active.edit_requested = Set(0);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("已放开修改权限".to_string()))
}

// 部门负责人拒绝修改申请
pub async fn deny_edit_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    if auth.role != "admin" && auth.role != "dept_manager" {
        return Err("无权限：仅部门负责人或管理员可操作".to_string());
    }

    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.edit_requested != 1 {
        return Err("该工时记录未申请修改".to_string());
    }

    // dept_manager 只能操作本部门员工，不能操作自己
    if auth.role == "dept_manager" {
        if entry.user_id == auth.user_id {
            return Err("不能操作自己的工时".to_string());
        }
        let current_user = User::find_by_id(auth.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("用户不存在")?;
        let manager_dept_id = current_user.department_id.ok_or("部门负责人必须归属部门")?;
        let entry_user = User::find_by_id(entry.user_id)
            .one(&db)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("工时归属用户不存在")?;
        if entry_user.department_id != Some(manager_dept_id) {
            return Err("只能操作本部门员工的工时".to_string());
        }
    }

    let mut active: time_entry::ActiveModel = entry.into();
    active.edit_requested = Set(0);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("已拒绝修改申请".to_string()))
}

pub async fn update_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
    Json(payload): Json<UpdateTimeEntry>,
) -> Result<Json<TimeEntryResponse>, String> {
    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.user_id != auth.user_id && auth.role != "admin" {
        return Err("只能修改自己的工时".to_string());
    }

    // pending/rejected 状态可直接修改；dept_manager 可直接修改自己的工时；其他状态需 edit_allowed=1
    let is_own_dept_manager = auth.role == "dept_manager" && entry.user_id == auth.user_id;
    let can_direct_edit = entry.status == "pending" || entry.status == "rejected";
    if !can_direct_edit && entry.edit_allowed != 1 && !is_own_dept_manager {
        return Err("该工时已审批，如需修改请向部门负责人申请".to_string());
    }

    validate_hours(payload.hours)?;
    find_active_project(&db, payload.project_id).await?;

    if entry.work_date != payload.work_date && is_workday(&entry.work_date) {
        let original_day_total =
            day_total_hours(&db, auth.user_id, entry.work_date, Some(entry.id)).await?;
        if original_day_total + 1e-9 < 8.0 {
            return Err(format!(
                "修改后 {} 工作日总工时不能低于8小时（当前合计 {:.1}）",
                entry.work_date, original_day_total
            ));
        }
    }
    ensure_workday_daily_total(
        &db,
        auth.user_id,
        payload.work_date,
        payload.hours,
        Some(entry.id),
    )
    .await?;

    // 记录修改日志
    let log_entry = json!({
        "modified_at": Utc::now().to_rfc3339(),
        "modified_by": auth.user_id,
        "before": {
            "project_id": entry.project_id,
            "work_date": entry.work_date.to_string(),
            "hours": entry.hours,
            "description": entry.description,
        },
        "after": {
            "project_id": payload.project_id,
            "work_date": payload.work_date.to_string(),
            "hours": payload.hours,
            "description": payload.description,
        }
    });

    let existing_log = entry
        .modification_log
        .clone()
        .unwrap_or_else(|| "[]".to_string());
    let mut logs: Vec<serde_json::Value> = serde_json::from_str(&existing_log).unwrap_or_default();
    logs.push(log_entry);
    let new_log = serde_json::to_string(&logs).unwrap_or_else(|_| "[]".to_string());

    let now = Utc::now().naive_utc();
    let mut active: time_entry::ActiveModel = entry.into();
    active.project_id = Set(payload.project_id);
    active.work_date = Set(payload.work_date);
    active.hours = Set(payload.hours);
    active.description = Set(payload.description);
    active.edit_allowed = Set(0); // 修改后重置
    active.modification_log = Set(Some(new_log));
    active.updated_at = Set(now);
    let result = active.update(&db).await.map_err(|e| e.to_string())?;

    Ok(Json(to_response(result)))
}

pub async fn delete_time_entry(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<Json<String>, String> {
    let entry = TimeEntry::find_by_id(id)
        .one(&db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("工时记录不存在")?;

    if entry.user_id != auth.user_id && auth.role != "admin" {
        return Err("只能删除自己的工时".to_string());
    }

    if entry.status != "pending" {
        return Err("只能撤回待审批的工时记录".to_string());
    }

    let active: time_entry::ActiveModel = entry.into();
    active.delete(&db).await.map_err(|e| e.to_string())?;

    Ok(Json("工时已撤回".to_string()))
}

pub async fn list_pending_approvals(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<TimeEntryResponse>>, String> {
    let entries = match auth.role.as_str() {
        "admin" => {
            // admin 看所有 pending、dept_approved、edit_requested
            TimeEntry::find()
                .filter(
                    Condition::any()
                        .add(time_entry::Column::Status.is_in(vec!["pending", "dept_approved"]))
                        .add(time_entry::Column::EditRequested.eq(1)),
                )
                .all(&db)
                .await
                .map_err(|e| e.to_string())?
        }
        "dept_manager" => {
            // 部门负责人看本部门 pending 和 edit_requested（排除自己提交的）
            let current_user = User::find_by_id(auth.user_id)
                .one(&db)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("用户不存在")?;
            let department_id = current_user.department_id.ok_or("部门负责人必须归属部门")?;
            let users_in_dept: Vec<i32> = User::find()
                .filter(user::Column::DepartmentId.eq(department_id))
                .all(&db)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|u| u.id)
                .filter(|&id| id != auth.user_id) // 排除自己
                .collect();

            // 同时找出该用户作为项目负责人（project.owner == username）的项目 id
            let owned_project_ids: Vec<i32> = Project::find()
                .filter(project::Column::Owner.eq(current_user.username.as_str()))
                .all(&db)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|p| p.id)
                .collect();

            // 查询：本部门 pending/edit_requested，或者自己负责项目的 dept_approved
            let mut cond = Condition::any().add(
                Condition::all()
                    .add(time_entry::Column::UserId.is_in(users_in_dept))
                    .add(
                        Condition::any()
                            .add(time_entry::Column::Status.eq("pending"))
                            .add(time_entry::Column::EditRequested.eq(1)),
                    ),
            );
            if !owned_project_ids.is_empty() {
                cond = cond.add(
                    Condition::all()
                        .add(time_entry::Column::ProjectId.is_in(owned_project_ids))
                        .add(time_entry::Column::Status.eq("dept_approved")),
                );
            }

            TimeEntry::find()
                .filter(cond)
                .all(&db)
                .await
                .map_err(|e| e.to_string())?
        }
        "project_manager" => {
            // 项目负责人看 dept_approved 的工时
            TimeEntry::find()
                .filter(time_entry::Column::Status.eq("dept_approved"))
                .all(&db)
                .await
                .map_err(|e| e.to_string())?
        }
        _ => return Err("无权限查看审批列表".to_string()),
    };

    Ok(Json(entries.into_iter().map(to_response).collect()))
}

pub async fn export_time_entries(
    auth: AuthUser,
    State(db): State<DatabaseConnection>,
    Query(params): Query<TimeEntryQuery>,
) -> Result<Response, String> {
    let mut query = build_base_query(&auth, &db).await?;

    if let Some(start) = params.start_date {
        query = query.filter(time_entry::Column::WorkDate.gte(start));
    }
    if let Some(end) = params.end_date {
        query = query.filter(time_entry::Column::WorkDate.lte(end));
    }
    if let Some(month_str) = params.month {
        if let Ok(d) = NaiveDate::parse_from_str(&format!("{}-01", month_str), "%Y-%m-%d") {
            let end = last_day_of_month(d);
            query = query.filter(time_entry::Column::WorkDate.gte(d));
            query = query.filter(time_entry::Column::WorkDate.lte(end));
        }
    }
    if let Some(user_id) = params.user_id {
        if auth.role == "admin" || auth.role == "timekeeper" || auth.role == "dept_manager" {
            query = query.filter(time_entry::Column::UserId.eq(user_id));
        }
    }
    if let Some(project_id) = params.project_id {
        query = query.filter(time_entry::Column::ProjectId.eq(project_id));
    }

    let entries = query
        .order_by_desc(time_entry::Column::WorkDate)
        .all(&db)
        .await
        .map_err(|e| e.to_string())?;

    let users: Vec<_> = User::find().all(&db).await.map_err(|e| e.to_string())?;
    let projects: Vec<_> = Project::find().all(&db).await.map_err(|e| e.to_string())?;

    let user_map: std::collections::HashMap<i32, String> =
        users.iter().map(|u| (u.id, u.username.clone())).collect();

    let status_map: std::collections::HashMap<&str, &str> = [
        ("pending", "待审批"),
        ("dept_approved", "部门已审批"),
        ("approved", "已通过"),
        ("rejected", "已拒绝"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    let header_format = Format::new()
        .set_bold()
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_background_color("D9E1F2");

    let border_format = Format::new();

    let headers = [
        "ID",
        "用户",
        "项目编号",
        "项目名称",
        "日期",
        "工时",
        "描述",
        "状态",
    ];
    for (col, header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col as u16, *header, &header_format)
            .unwrap();
        worksheet.set_column_width(col as u16, 20).unwrap();
    }

    for (row, entry) in entries.iter().enumerate() {
        let row = row as u32 + 1;
        let status_label = status_map
            .get(entry.status.as_str())
            .copied()
            .unwrap_or(&entry.status);
        worksheet
            .write_string_with_format(row, 0, &entry.id.to_string(), &border_format)
            .unwrap();
        worksheet
            .write_string_with_format(
                row,
                1,
                user_map
                    .get(&entry.user_id)
                    .map(|s| s.as_str())
                    .unwrap_or("-"),
                &border_format,
            )
            .unwrap();
        let (project_no, project_name) = entry
            .project_id
            .and_then(|id| projects.iter().find(|p| p.id == id))
            .map(|p| {
                (
                    p.project_no
                        .clone()
                        .or(p.code.clone())
                        .unwrap_or_else(|| "-".to_string()),
                    p.name.clone(),
                )
            })
            .unwrap_or_else(|| ("-".to_string(), "-".to_string()));

        worksheet
            .write_string_with_format(row, 2, &project_no, &border_format)
            .unwrap();
        worksheet
            .write_string_with_format(row, 3, &project_name, &border_format)
            .unwrap();
        worksheet
            .write_string_with_format(row, 4, &entry.work_date.to_string(), &border_format)
            .unwrap();
        worksheet
            .write_number_with_format(row, 5, entry.hours, &border_format)
            .unwrap();
        worksheet
            .write_string_with_format(row, 6, &entry.description, &border_format)
            .unwrap();
        worksheet
            .write_string_with_format(row, 7, status_label, &border_format)
            .unwrap();
    }

    let filename = format!(
        "工时记录_{}.xlsx",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );

    let buffer = workbook.save_to_buffer().map_err(|e| e.to_string())?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename={}",
                percent_encoding::utf8_percent_encode(
                    &filename,
                    percent_encoding::NON_ALPHANUMERIC
                )
            ),
        )
        .body(axum::body::Body::from(buffer))
        .unwrap())
}

// 构建基础查询（按角色过滤）
async fn build_base_query(
    auth: &AuthUser,
    db: &DatabaseConnection,
) -> Result<sea_orm::Select<time_entry::Entity>, String> {
    let query = match auth.role.as_str() {
        "admin" | "timekeeper" => TimeEntry::find(),
        "dept_manager" => {
            let current_user = User::find_by_id(auth.user_id)
                .one(db)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("用户不存在")?;
            let department_id = current_user.department_id.ok_or("部门负责人必须归属部门")?;
            let users_in_dept: Vec<i32> = User::find()
                .filter(user::Column::DepartmentId.eq(department_id))
                .all(db)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|u| u.id)
                .collect();
            TimeEntry::find().filter(time_entry::Column::UserId.is_in(users_in_dept))
        }
        "project_manager" => {
            // 项目负责人可查看所有 dept_approved 及之后状态的工时
            TimeEntry::find().filter(time_entry::Column::Status.is_in(vec![
                "dept_approved",
                "approved",
                "rejected",
            ]))
        }
        _ => TimeEntry::find().filter(time_entry::Column::UserId.eq(auth.user_id)),
    };
    Ok(query)
}

fn last_day_of_month(d: NaiveDate) -> NaiveDate {
    let month = d.month();
    let year = d.year();
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .pred_opt()
        .unwrap()
}
