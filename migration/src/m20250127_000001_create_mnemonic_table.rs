use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Mnemonic::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Mnemonic::Id)
                            .integer()
                            .not_null()
                            .primary_key()
                            .default(1),
                    )
                    .col(string(Mnemonic::EncryptedMnemonic))
                    .col(big_unsigned(Mnemonic::CreatedAt))
                    .col(big_unsigned(Mnemonic::UpdatedAt))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Mnemonic::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Mnemonic {
    Table,
    Id,
    EncryptedMnemonic,
    CreatedAt,
    UpdatedAt,
}
