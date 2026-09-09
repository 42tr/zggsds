use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "registration_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub username: String,
    pub email: String,
    #[sea_orm(column_type = "Text")]
    pub password_hash: String,
    pub department_id: i32,
    pub status: String,

    pub assigned_approver_role: Option<String>,
    pub assigned_approver_id: Option<i32>,

    pub approved_by: Option<i32>,
    pub approved_at: Option<DateTime>,

    pub rejected_by: Option<i32>,
    pub rejected_at: Option<DateTime>,

    #[sea_orm(column_type = "Text")]
    pub reject_reason: Option<String>,

    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
