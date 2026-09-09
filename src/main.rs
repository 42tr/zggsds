mod auth;
mod db;
mod embed;
mod error;
mod handlers;
mod models;

#[cfg(test)]
mod tests;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use db::establish_connection;

#[tokio::main]
async fn main() {
    auth::validate_jwt_secret();
    let db = establish_connection().await;

    let app = Router::new()
        .route("/", get(embed::serve_index))
        .route("/:path", get(embed::serve_assets))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/logout", post(handlers::auth::logout))
        .route(
            "/auth/change-password",
            put(handlers::auth::change_password),
        )
        // 注册：匿名提交 + 查询状态
        .route("/auth/register", post(handlers::registration::register))
        .route(
            "/auth/register/status",
            get(handlers::registration::register_status),
        )
        // 注册审批：负责人/管理员
        .route(
            "/approvals/registrations/pending",
            get(handlers::registration_approval::list_pending_registrations),
        )
        .route(
            "/approvals/registrations/:id/approve",
            put(handlers::registration_approval::approve_registration),
        )
        .route(
            "/approvals/registrations/:id/reject",
            put(handlers::registration_approval::reject_registration),
        )
        // 部门列表（公开给注册页用）
        .route(
            "/public/departments/flat",
            get(handlers::department::list_departments_flat_public),
        )
        .route(
            "/users",
            post(handlers::user::create_user).get(handlers::user::list_users),
        )
        .route(
            "/users/:id",
            delete(handlers::user::delete_user).put(handlers::user::update_user),
        )
        .route("/users/options", get(handlers::user::list_user_options))
        // 项目管理：全员查看，timekeeper/admin 可增删改
        .route(
            "/projects",
            post(handlers::project::create_project).get(handlers::project::list_projects),
        )
        .route(
            "/projects/active",
            get(handlers::project::list_active_projects),
        )
        .route(
            "/projects/:id",
            put(handlers::project::update_project).delete(handlers::project::delete_project),
        )
        // 工时管理
        .route(
            "/time-entries/export",
            get(handlers::time_entry::export_time_entries),
        )
        .route(
            "/time-entries/pending",
            get(handlers::time_entry::list_pending_approvals),
        )
        .route(
            "/time-entries/batch",
            post(handlers::time_entry::create_time_entries_batch),
        )
        .route(
            "/time-entries",
            post(handlers::time_entry::create_time_entry)
                .get(handlers::time_entry::list_time_entries),
        )
        .route(
            "/time-entries/:id",
            put(handlers::time_entry::update_time_entry)
                .delete(handlers::time_entry::delete_time_entry),
        )
        // 一审（部门负责人）
        .route(
            "/time-entries/:id/approve",
            put(handlers::time_entry::approve_time_entry),
        )
        // 二审（项目负责人）
        .route(
            "/time-entries/:id/second-approve",
            put(handlers::time_entry::second_approve_time_entry),
        )
        // 员工申请修改
        .route(
            "/time-entries/:id/request-edit",
            put(handlers::time_entry::request_edit_time_entry),
        )
        // 部门负责人放开/拒绝修改权限
        .route(
            "/time-entries/:id/allow-edit",
            put(handlers::time_entry::allow_edit_time_entry),
        )
        .route(
            "/time-entries/:id/deny-edit",
            put(handlers::time_entry::deny_edit_time_entry),
        )
        // 部门管理
        .route(
            "/departments",
            post(handlers::department::create_department)
                .get(handlers::department::list_departments),
        )
        .route(
            "/departments/flat",
            get(handlers::department::list_departments_flat),
        )
        .route(
            "/departments/:id",
            put(handlers::department::update_department)
                .delete(handlers::department::delete_department),
        )
        .with_state(db);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("服务器启动在 http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
