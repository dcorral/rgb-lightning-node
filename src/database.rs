use futures::executor::block_on;
use rln_entity::{ConfigActMod, ConfigEntity, DbMnemonic, DbMnemonicActMod, MnemonicEntity};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::error::APIError;

pub struct RlnDatabase {
    connection: DatabaseConnection,
}

impl RlnDatabase {
    /// Create an RlnDatabase wrapper from an existing connection.
    /// Does NOT run migrations (assumes they were already run).
    pub fn from_connection(connection: DatabaseConnection) -> Self {
        Self { connection }
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

    /// Get a config value by key.
    pub fn get_config(&self, key: &str) -> Result<Option<String>, APIError> {
        let result = block_on(
            ConfigEntity::find()
                .filter(rln_entity::ConfigColumn::Key.eq(key))
                .one(self.get_connection()),
        )
        .map_err(|e| std::io::Error::other(format!("Database query failed: {e}")))?;

        Ok(result.map(|r| r.value))
    }

    /// Set a config value. Uses UPSERT for atomic insert/update.
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), APIError> {
        let now = crate::utils::get_current_timestamp() as i64;

        let config = ConfigActMod {
            key: ActiveValue::Set(key.to_string()),
            value: ActiveValue::Set(value.to_string()),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
        };

        block_on(
            ConfigEntity::insert(config)
                .on_conflict(
                    OnConflict::column(rln_entity::ConfigColumn::Key)
                        .update_columns([
                            rln_entity::ConfigColumn::Value,
                            rln_entity::ConfigColumn::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(self.get_connection()),
        )
        .map_err(|e| std::io::Error::other(format!("Database write failed: {e}")))?;

        Ok(())
    }
}
