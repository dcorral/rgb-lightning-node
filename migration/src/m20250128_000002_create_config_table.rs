use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Config::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Config::Key)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Config::Value).string().not_null())
                    .col(ColumnDef::new(Config::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(Config::UpdatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Config::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Config {
    Table,
    Key,
    Value,
    CreatedAt,
    UpdatedAt,
}
