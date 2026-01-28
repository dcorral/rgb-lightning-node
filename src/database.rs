use std::path::Path;
use std::time::Duration;

use futures::executor::block_on;
use rln_entity::{DbMnemonic, DbMnemonicActMod, MnemonicEntity};
use rln_migration::{Migrator, MigratorTrait};
use sea_orm::{ActiveValue, ConnectOptions, Database, DatabaseConnection, EntityTrait};

use crate::error::APIError;

pub struct RlnDatabase {
    connection: DatabaseConnection,
}

impl RlnDatabase {
    /// Initialize the database connection and run migrations.
    /// This function uses block_on internally so should NOT be called from an async context.
    /// Use `new_async` for async contexts or wrap this in `spawn_blocking`.
    pub fn new(db_path: &Path) -> Result<Self, APIError> {
        let connection_string = format!("sqlite:{}?mode=rwc", db_path.display());
        let mut opt = ConnectOptions::new(connection_string);
        // Use single connection to avoid deadlocks
        opt.max_connections(1)
            .min_connections(0)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .max_lifetime(Duration::from_secs(8));

        let connection = block_on(Database::connect(opt)).map_err(|e| {
            APIError::FailedKeysCreation(
                db_path.to_string_lossy().to_string(),
                format!("Database connection failed: {e}"),
            )
        })?;

        block_on(Migrator::up(&connection, None)).map_err(|e| {
            APIError::FailedKeysCreation(
                db_path.to_string_lossy().to_string(),
                format!("Migration failed: {e}"),
            )
        })?;

        Ok(Self { connection })
    }

    /// Initialize the database connection and run migrations asynchronously.
    pub async fn new_async(db_path: &Path) -> Result<Self, APIError> {
        let connection_string = format!("sqlite:{}?mode=rwc", db_path.display());
        let mut opt = ConnectOptions::new(connection_string);
        opt.max_connections(1)
            .min_connections(0)
            .connect_timeout(Duration::from_secs(8))
            .idle_timeout(Duration::from_secs(8))
            .max_lifetime(Duration::from_secs(8));

        let connection = Database::connect(opt).await.map_err(|e| {
            APIError::FailedKeysCreation(
                db_path.to_string_lossy().to_string(),
                format!("Database connection failed: {e}"),
            )
        })?;

        Migrator::up(&connection, None).await.map_err(|e| {
            APIError::FailedKeysCreation(
                db_path.to_string_lossy().to_string(),
                format!("Migration failed: {e}"),
            )
        })?;

        Ok(Self { connection })
    }

    pub fn get_connection(&self) -> &DatabaseConnection {
        &self.connection
    }

    pub fn mnemonic_exists(&self) -> Result<bool, APIError> {
        let result = block_on(MnemonicEntity::find_by_id(1).one(self.get_connection()))
            .map_err(|e| std::io::Error::other(format!("Database query failed: {e}")))?;
        Ok(result.is_some())
    }

    pub fn get_mnemonic(&self) -> Result<Option<DbMnemonic>, APIError> {
        block_on(MnemonicEntity::find_by_id(1).one(self.get_connection()))
            .map_err(|e| std::io::Error::other(format!("Database query failed: {e}")))
            .map_err(APIError::IO)
    }

    pub fn save_mnemonic(&self, encrypted_mnemonic: String) -> Result<(), APIError> {
        let now = crate::utils::get_current_timestamp() as i64;
        let existing = self.get_mnemonic()?;

        if let Some(_) = existing {
            let mnemonic = DbMnemonicActMod {
                id: ActiveValue::Set(1),
                encrypted_mnemonic: ActiveValue::Set(encrypted_mnemonic),
                created_at: ActiveValue::NotSet,
                updated_at: ActiveValue::Set(now),
            };
            block_on(MnemonicEntity::update(mnemonic).exec(self.get_connection()))
                .map_err(|e| std::io::Error::other(format!("Database update failed: {e}")))?;
        } else {
            let mnemonic = DbMnemonicActMod {
                id: ActiveValue::Set(1),
                encrypted_mnemonic: ActiveValue::Set(encrypted_mnemonic),
                created_at: ActiveValue::Set(now),
                updated_at: ActiveValue::Set(now),
            };
            block_on(MnemonicEntity::insert(mnemonic).exec(self.get_connection()))
                .map_err(|e| std::io::Error::other(format!("Database insert failed: {e}")))?;
        }
        Ok(())
    }
}
