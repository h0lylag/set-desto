use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const SERVICE_NAME: &str = "set-desto";
const USERNAME_PREFIX: &str = "eve-character";

#[allow(dead_code)]
pub trait TokenStore {
    fn save_refresh_token(&self, character_id: u64, refresh_token: &str) -> Result<()>;
    fn load_refresh_token(&self, character_id: u64) -> Result<String>;
    fn delete_refresh_token(&self, character_id: u64) -> Result<()>;

    fn save_access_token(
        &self,
        character_id: u64,
        access_token: &str,
        expires_at: SystemTime,
    ) -> Result<()>;
    fn load_access_token(&self, character_id: u64) -> Result<Option<CachedAccessToken>>;
    fn delete_access_token(&self, character_id: u64) -> Result<()>;
}

#[derive(Clone)]
pub struct CachedAccessToken {
    pub access_token: String,
    pub expires_at: SystemTime,
}

impl std::fmt::Debug for CachedAccessToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CachedAccessToken")
            .field("access_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeyringTokenStore;

impl TokenStore for KeyringTokenStore {
    fn save_refresh_token(&self, character_id: u64, refresh_token: &str) -> Result<()> {
        refresh_entry_for_character(character_id)?
            .set_password(refresh_token)
            .with_context(|| {
                format!("Failed to save refresh token for character {character_id}")
            })?;
        info!(character_id, "Saved refresh token to OS keyring");
        Ok(())
    }

    fn load_refresh_token(&self, character_id: u64) -> Result<String> {
        debug!(character_id, "Loading refresh token from OS keyring");
        refresh_entry_for_character(character_id)?
            .get_password()
            .with_context(|| format!("Failed to load refresh token for character {character_id}"))
    }

    fn delete_refresh_token(&self, character_id: u64) -> Result<()> {
        match refresh_entry_for_character(character_id)?.delete_credential() {
            Ok(()) => {
                info!(character_id, "Deleted refresh token from OS keyring");
                Ok(())
            }
            Err(KeyringError::NoEntry) => {
                debug!(character_id, "No refresh token existed in OS keyring");
                Ok(())
            }
            Err(err) => Err(err).with_context(|| {
                format!("Failed to delete refresh token for character {character_id}")
            }),
        }
    }

    fn save_access_token(
        &self,
        character_id: u64,
        access_token: &str,
        expires_at: SystemTime,
    ) -> Result<()> {
        let expires_at_unix_seconds = expires_at
            .duration_since(UNIX_EPOCH)
            .context("Access token expiry was before the Unix epoch")?
            .as_secs();
        let payload = CachedAccessTokenPayload {
            access_token: access_token.to_string(),
            expires_at_unix_seconds,
        };
        let payload =
            serde_json::to_string(&payload).context("Failed to serialize cached access token")?;

        access_entry_for_character(character_id)?
            .set_password(&payload)
            .with_context(|| format!("Failed to save access token for character {character_id}"))?;

        info!(
            character_id,
            expires_at_unix_seconds, "Saved access token to OS keyring"
        );
        Ok(())
    }

    fn load_access_token(&self, character_id: u64) -> Result<Option<CachedAccessToken>> {
        debug!(character_id, "Loading cached access token from OS keyring");
        let payload = match access_entry_for_character(character_id)?.get_password() {
            Ok(payload) => payload,
            Err(KeyringError::NoEntry) => {
                debug!(character_id, "No cached access token existed in OS keyring");
                return Ok(None);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("Failed to load access token for character {character_id}")
                });
            }
        };

        let payload: CachedAccessTokenPayload =
            serde_json::from_str(&payload).with_context(|| {
                format!("Failed to parse access token for character {character_id}")
            })?;
        let expires_at = UNIX_EPOCH + Duration::from_secs(payload.expires_at_unix_seconds);

        Ok(Some(CachedAccessToken {
            access_token: payload.access_token,
            expires_at,
        }))
    }

    fn delete_access_token(&self, character_id: u64) -> Result<()> {
        match access_entry_for_character(character_id)?.delete_credential() {
            Ok(()) => {
                info!(character_id, "Deleted access token from OS keyring");
                Ok(())
            }
            Err(KeyringError::NoEntry) => {
                debug!(character_id, "No access token existed in OS keyring");
                Ok(())
            }
            Err(err) => Err(err).with_context(|| {
                format!("Failed to delete access token for character {character_id}")
            }),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CachedAccessTokenPayload {
    access_token: String,
    expires_at_unix_seconds: u64,
}

fn refresh_entry_for_character(character_id: u64) -> Result<Entry> {
    Entry::new(SERVICE_NAME, &format!("{USERNAME_PREFIX}-{character_id}")).with_context(|| {
        format!("Failed to open refresh keyring entry for character {character_id}")
    })
}

fn access_entry_for_character(character_id: u64) -> Result<Entry> {
    Entry::new(
        SERVICE_NAME,
        &format!("{USERNAME_PREFIX}-{character_id}-access"),
    )
    .with_context(|| format!("Failed to open access keyring entry for character {character_id}"))
}
