//! Config entity for storing key-value configuration in the database.

use sea_orm::entity::prelude::*;

/// Database model for configuration key-value pairs.
/// The database is the source of truth; files are synced for rust-lightning compatibility.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
