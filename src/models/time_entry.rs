use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "time_entries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub project_id: Option<i32>,
    pub work_date: Date,
    pub hours: f64,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub work_type: String,
    pub status: String,
    pub approved_by: Option<i32>,
    pub approved_at: Option<DateTime>,
    pub second_approved_by: Option<i32>,
    pub second_approved_at: Option<DateTime>,
    pub second_status: Option<String>,
    pub edit_allowed: i32,
    pub edit_requested: i32,
    pub modification_log: Option<String>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
