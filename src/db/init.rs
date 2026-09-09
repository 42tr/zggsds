use crate::auth::hash_password;
use crate::models::user::{self, Entity as User};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, Statement,
};

pub async fn init_database(db: &sea_orm::DatabaseConnection) {
    let sql = r#"
        CREATE TABLE IF NOT EXISTS departments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            parent_id INTEGER,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (parent_id) REFERENCES departments(id)
        );

        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'employee',
            department_id INTEGER,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            deleted_at DATETIME,
            FOREIGN KEY (department_id) REFERENCES departments(id)
        );

        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_no TEXT,
            name TEXT NOT NULL UNIQUE,
            code TEXT,
            project_type TEXT NOT NULL DEFAULT '事务',
            status TEXT NOT NULL DEFAULT '进行中',
            cycle TEXT,
            owner TEXT,
            description TEXT,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS time_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            project_id INTEGER,
            work_date DATE NOT NULL,
            hours REAL NOT NULL,
            description TEXT NOT NULL,
            work_type TEXT NOT NULL DEFAULT 'development',
            status TEXT NOT NULL DEFAULT 'pending',
            approved_by INTEGER,
            approved_at DATETIME,
            second_approved_by INTEGER,
            second_approved_at DATETIME,
            second_status TEXT,
            edit_allowed INTEGER NOT NULL DEFAULT 0,
            edit_requested INTEGER NOT NULL DEFAULT 0,
            modification_log TEXT,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (project_id) REFERENCES projects(id),
            FOREIGN KEY (approved_by) REFERENCES users(id),
            FOREIGN KEY (second_approved_by) REFERENCES users(id)
        );

        -- 注册申请单：匿名用户提交，部门负责人/管理员审批后才创建真实用户
        CREATE TABLE IF NOT EXISTS registration_requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL,
            email TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            department_id INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            -- 记录系统把该申请分配给谁处理（无部门负责人时分配给 admin/timekeeper）
            assigned_approver_role TEXT,
            assigned_approver_id INTEGER,
            approved_by INTEGER,
            approved_at DATETIME,
            rejected_by INTEGER,
            rejected_at DATETIME,
            reject_reason TEXT,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (department_id) REFERENCES departments(id),
            FOREIGN KEY (assigned_approver_id) REFERENCES users(id),
            FOREIGN KEY (approved_by) REFERENCES users(id),
            FOREIGN KEY (rejected_by) REFERENCES users(id)
        );
    "#;

    db.execute(Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        sql.to_string(),
    ))
    .await
    .expect("Failed to initialize database");

    // 迁移：为已存在的 projects 表添加新字段（忽略已存在的错误）
    let migrations = vec![
        "ALTER TABLE projects ADD COLUMN project_no TEXT",
        "ALTER TABLE projects ADD COLUMN project_type TEXT NOT NULL DEFAULT '事务'",
        "ALTER TABLE projects ADD COLUMN status TEXT NOT NULL DEFAULT '进行中'",
        "ALTER TABLE projects ADD COLUMN cycle TEXT",
        "ALTER TABLE projects ADD COLUMN owner TEXT",
        "ALTER TABLE time_entries ADD COLUMN approved_by INTEGER",
        "ALTER TABLE time_entries ADD COLUMN approved_at DATETIME",
        "ALTER TABLE time_entries ADD COLUMN second_approved_by INTEGER",
        "ALTER TABLE time_entries ADD COLUMN second_approved_at DATETIME",
        "ALTER TABLE time_entries ADD COLUMN second_status TEXT",
        "ALTER TABLE time_entries ADD COLUMN edit_allowed INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE time_entries ADD COLUMN edit_requested INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE time_entries ADD COLUMN modification_log TEXT",
        // registration_requests 的增量字段（老库里没有则补；有则忽略错误）
        "ALTER TABLE registration_requests ADD COLUMN assigned_approver_role TEXT",
        "ALTER TABLE registration_requests ADD COLUMN assigned_approver_id INTEGER",
        "ALTER TABLE registration_requests ADD COLUMN approved_by INTEGER",
        "ALTER TABLE registration_requests ADD COLUMN approved_at DATETIME",
        "ALTER TABLE registration_requests ADD COLUMN rejected_by INTEGER",
        "ALTER TABLE registration_requests ADD COLUMN rejected_at DATETIME",
        "ALTER TABLE registration_requests ADD COLUMN reject_reason TEXT",
    ];

    for migration in migrations {
        let _ = db
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                migration.to_string(),
            ))
            .await;
    }

    // 初始化 admin
    let existing_admin = User::find()
        .filter(user::Column::Username.eq("admin"))
        .one(db)
        .await
        .unwrap_or(None);

    if existing_admin.is_none() {
        let admin_password = std::env::var("ADMIN_PASSWORD")
            .expect("ADMIN_PASSWORD must be set when creating the initial admin user");
        if admin_password.trim().len() < 12 {
            panic!("ADMIN_PASSWORD must be at least 12 characters");
        }
        let password_hash = hash_password(&admin_password);
        user::ActiveModel {
            username: Set("admin".to_string()),
            email: Set("admin@zdht.com".to_string()),
            password_hash: Set(password_hash),
            role: Set("admin".to_string()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("Failed to create admin user");
        println!("默认管理员账号已创建: 用户名=admin");
    }
}
