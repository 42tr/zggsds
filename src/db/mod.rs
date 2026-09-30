use sea_orm::{Database, DbConn};

mod init;

pub async fn establish_connection() -> DbConn {
    std::fs::create_dir_all("./data").expect("Failed to create data directory");
    let db_url = "sqlite://./data/time_tracking.db?mode=rwc";
    let db = Database::connect(db_url)
        .await
        .expect("Failed to connect to database");
    init::init_database(&db).await;
    db
}
