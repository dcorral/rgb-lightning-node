use futures::executor::block_on;
use rln_entity::{DbMnemonic, DbMnemonicActMod, MnemonicEntity};
use sea_orm::{ActiveValue, DatabaseConnection, EntityTrait};

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
}
