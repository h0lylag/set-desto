use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use crate::eve::auth::{self, AuthenticatedCharacter};
use crate::storage::config::CharacterConfig;
use crate::storage::tokens::TokenStore;

use super::models::{CharacterState, expires_at_from_now};
use super::{SetDestoApp, upsert_character};

impl SetDestoApp {
    pub fn start_character_login(&mut self) {
        if self.login_in_progress() {
            debug!("Ignoring Add Character click because login is already in progress");
            return;
        }

        if self.waypoint_send_in_progress() {
            debug!("Ignoring Add Character click because a waypoint send is in progress");
            self.status_message = "Wait for the current waypoint send to finish".to_string();
            return;
        }

        let config = match self.sso_config() {
            Ok(config) => config,
            Err(err) => {
                warn!(error = ?err, "Cannot start EVE SSO login");
                self.status_message = err.to_string();
                return;
            }
        };

        info!(redirect_uri = %config.redirect_uri, "Starting EVE SSO login");
        self.status_message = "Opening EVE SSO login...".to_string();
        self.login_receiver = Some(auth::start_login(config));
    }

    pub fn poll_character_login(&mut self) {
        let Some(receiver) = &self.login_receiver else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(character)) => {
                let character_name = character.character_name.clone();
                match self.save_logged_in_character(character) {
                    Ok(()) => {
                        info!(character_name, "Character login saved");
                        self.status_message = format!("Added {character_name}");
                    }
                    Err(err) => {
                        error!(character_name, error = ?err, "Failed to save character login");
                        self.status_message = format!("Failed to save {character_name}: {err}");
                    }
                }
                self.login_receiver = None;
            }
            Ok(Err(error)) => {
                error!(error, "EVE SSO login failed");
                self.status_message = format!("Login failed: {error}");
                self.login_receiver = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                error!("EVE SSO worker disconnected");
                self.status_message = "Login failed: SSO worker stopped".to_string();
                self.login_receiver = None;
            }
        }
    }

    fn save_logged_in_character(&mut self, character: AuthenticatedCharacter) -> Result<()> {
        let expires_at = expires_at_from_now(character.expires_in);
        let selected = self
            .characters
            .iter()
            .find(|existing| existing.character_id == character.character_id)
            .map(|existing| existing.selected)
            .unwrap_or(true);
        let character_config = CharacterConfig {
            character_id: character.character_id,
            character_name: character.character_name.clone(),
            scopes: character.scopes.clone(),
            selected,
        };

        info!(
            character_id = character.character_id,
            character_name = %character.character_name,
            scope_count = character.scopes.len(),
            expires_in = character.expires_in,
            "Saving authenticated character"
        );
        self.token_store
            .save_refresh_token(character.character_id, &character.refresh_token)?;
        self.token_store.save_access_token(
            character.character_id,
            &character.access_token,
            expires_at,
        )?;

        self.config.upsert_character(character_config.clone());
        self.config
            .save()
            .context("Failed to save character metadata")?;

        upsert_character(
            &mut self.characters,
            CharacterState::from_login(character_config, character.access_token, expires_at),
        );
        if !self.waypoint_send_in_progress() {
            self.clear_send_results();
        }

        Ok(())
    }
}
