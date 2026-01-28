pub use sea_orm_migration::prelude::*;

mod m20250127_000001_create_mnemonic_table;
mod m20250128_000001_create_kv_store_table;
mod m20250128_000002_create_config_table;
mod m20250128_000003_create_revoked_token_table;
mod m20250128_000004_create_channel_peer_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250127_000001_create_mnemonic_table::Migration),
            Box::new(m20250128_000001_create_kv_store_table::Migration),
            Box::new(m20250128_000002_create_config_table::Migration),
            Box::new(m20250128_000003_create_revoked_token_table::Migration),
            Box::new(m20250128_000004_create_channel_peer_table::Migration),
        ]
    }
}
