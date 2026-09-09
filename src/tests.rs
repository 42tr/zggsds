use crate::{
    auth::AuthUser,
    handlers::{auth, time_entry, user},
    models,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, IntoActiveModel, Schema,
};

async fn database() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    db.execute(backend.build(&schema.create_table_from_entity(models::user::Entity)))
        .await
        .unwrap();
    db.execute(backend.build(&schema.create_table_from_entity(models::time_entry::Entity)))
        .await
        .unwrap();
    for (id, role, deleted) in [
        (1, "admin", false),
        (2, "employee", false),
        (3, "employee", true),
        (4, "project_manager", false),
    ] {
        let now = Utc::now().naive_utc();
        models::user::Model {
            id,
            username: format!("user{id}"),
            email: format!("user{id}@test.invalid"),
            password_hash: bcrypt::hash("correct-password", 4).unwrap(),
            role: role.into(),
            department_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
        }
        .into_active_model()
        .insert(&db)
        .await
        .unwrap();
        models::time_entry::Model {
            id,
            user_id: id,
            project_id: None,
            work_date: now.date(),
            hours: 2.0,
            description: format!("entry{id}"),
            work_type: "development".into(),
            status: "pending".into(),
            approved_by: None,
            approved_at: None,
            second_approved_by: None,
            second_approved_at: None,
            second_status: None,
            edit_allowed: 0,
            edit_requested: 0,
            modification_log: None,
            created_at: now,
            updated_at: now,
        }
        .into_active_model()
        .insert(&db)
        .await
        .unwrap();
    }
    db
}

fn employee() -> AuthUser {
    AuthUser {
        user_id: 2,
        role: "employee".into(),
    }
}

#[tokio::test]
async fn employee_can_resolve_own_name_and_only_read_own_time_entries() {
    let db = database().await;
    let Json(options) = user::list_user_options(employee(), State(db.clone()))
        .await
        .unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].id, 2);
    let json = serde_json::to_value(&options[0]).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 2);
    assert!(json.get("email").is_none());
    let params = serde_json::from_value(serde_json::json!({"user_id":1})).unwrap();
    let Json(entries) = time_entry::list_time_entries(employee(), State(db.clone()), Query(params))
        .await
        .unwrap();
    assert_eq!(entries.entries.len(), 1);
    assert_eq!(entries.entries[0].user_id, 2);
    assert_eq!(
        user::list_users(employee(), State(db.clone()))
            .await
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        user::delete_user(employee(), State(db), axum::extract::Path(1))
            .await
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn project_manager_can_resolve_approval_names_without_private_accounts() {
    let db = database().await;
    let manager = AuthUser {
        user_id: 4,
        role: "project_manager".into(),
    };
    let Json(options) = user::list_user_options(manager, State(db.clone()))
        .await
        .unwrap();
    let mut ids: Vec<_> = options.iter().map(|u| u.id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 4]);
    let Json(accounts) = user::list_users(
        AuthUser {
            user_id: 1,
            role: "admin".into(),
        },
        State(db),
    )
    .await
    .unwrap();
    assert_eq!(accounts.len(), 3);
}

#[tokio::test]
async fn authentication_errors_have_failure_status_codes() {
    let db = database().await;
    for username in ["user2", "unknown", "user3"] {
        let result = auth::login(
            State(db.clone()),
            Json(auth::LoginRequest {
                username: username.into(),
                password: "wrong-password".into(),
            }),
        )
        .await;
        assert_eq!(result.into_response().status(), StatusCode::UNAUTHORIZED);
    }
    let result = auth::change_password(
        employee(),
        State(db),
        Json(auth::ChangePasswordRequest {
            old_password: "wrong-password".into(),
            new_password: "new-password".into(),
        }),
    )
    .await;
    assert_eq!(result.into_response().status(), StatusCode::BAD_REQUEST);
}
