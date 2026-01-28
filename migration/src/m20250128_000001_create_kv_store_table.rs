use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KvStore::Table)
                    .if_not_exists()
                    .col(string(KvStore::PrimaryNamespace))
                    .col(string(KvStore::SecondaryNamespace))
                    .col(string(KvStore::Key))
                    .col(blob(KvStore::Value))
                    .primary_key(
                        Index::create()
                            .col(KvStore::PrimaryNamespace)
                            .col(KvStore::SecondaryNamespace)
                            .col(KvStore::Key),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KvStore::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum KvStore {
    Table,
    PrimaryNamespace,
    SecondaryNamespace,
    Key,
    Value,
}
