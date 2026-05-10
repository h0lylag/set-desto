use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};

const SERVICE_NAME: &str = "set-desto";
const USERNAME_PREFIX: &str = "eve-character";

#[allow(dead_code)]
pub trait TokenStore {
    fn save_refresh_token(&self, character_id: u64, refresh_token: &str) -> Result<()>;
    fn load_refresh_token(&self, character_id: u64) -> Result<String>;
    fn delete_refresh_token(&self, character_id: u64) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn save_refresh_token(&self, character_id: u64, refresh_token: &str) -> Result<()> {
        entry_for_character(character_id)?
            .set_password(refresh_token)
            .with_context(|| format!("Failed to save refresh token for character {character_id}"))
    }

    fn load_refresh_token(&self, character_id: u64) -> Result<String> {
        entry_for_character(character_id)?
            .get_password()
            .with_context(|| format!("Failed to load refresh token for character {character_id}"))
    }

    fn delete_refresh_token(&self, character_id: u64) -> Result<()> {
        match entry_for_character(character_id)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(err).with_context(|| {
                format!("Failed to delete refresh token for character {character_id}")
            }),
        }
    }
}

fn entry_for_character(character_id: u64) -> Result<Entry> {
    Entry::new(SERVICE_NAME, &format!("{USERNAME_PREFIX}-{character_id}"))
        .with_context(|| format!("Failed to open keyring entry for character {character_id}"))
}
