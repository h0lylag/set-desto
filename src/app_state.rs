use std::sync::mpsc::Receiver;

use anyhow::{Context, Result};
use eframe::egui;
use tracing::debug;

use crate::eve::auth::{self, AuthenticatedCharacter, LoginResult, SsoConfig};
use crate::storage::config::{AppConfig, CharacterConfig};
use crate::storage::tokens::{KeyringTokenStore, TokenStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestoMode {
    Manual,
    Clipboard,
}

impl DestoMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::Clipboard => "Clipboard",
        }
    }
}

pub struct SetDestoApp {
    pub debug_mode: bool,
    pub characters: Vec<CharacterState>,
    pub destination: String,
    pub notes: String,
    pub pin_destination: bool,
    pub mode: DestoMode,
    pub status_message: String,
    config: AppConfig,
    login_receiver: Option<Receiver<LoginResult>>,
    token_store: KeyringTokenStore,
}

impl SetDestoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, debug_mode: bool) -> Self {
        debug!("Initializing Set Desto (debug_mode={})", debug_mode);

        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let (config, status_message) = match AppConfig::load() {
            Ok(config) => (config, "Ready".to_string()),
            Err(err) => (
                AppConfig::default(),
                format!("Failed to load config: {err}"),
            ),
        };
        let characters = config
            .characters
            .iter()
            .cloned()
            .map(CharacterState::from_config)
            .collect();

        Self {
            debug_mode,
            characters,
            destination: String::new(),
            notes: String::new(),
            pin_destination: false,
            mode: DestoMode::Manual,
            status_message,
            config,
            login_receiver: None,
            token_store: KeyringTokenStore,
        }
    }

    pub fn login_in_progress(&self) -> bool {
        self.login_receiver.is_some()
    }

    pub fn start_character_login(&mut self) {
        if self.login_in_progress() {
            return;
        }

        let config = match SsoConfig::from_env() {
            Ok(config) => config,
            Err(err) => {
                self.status_message = err.to_string();
                return;
            }
        };

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
                        self.status_message = format!("Added {character_name}");
                    }
                    Err(err) => {
                        self.status_message = format!("Failed to save {character_name}: {err}");
                    }
                }
                self.login_receiver = None;
            }
            Ok(Err(error)) => {
                self.status_message = format!("Login failed: {error}");
                self.login_receiver = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.status_message = "Login failed: SSO worker stopped".to_string();
                self.login_receiver = None;
            }
        }
    }

    pub fn set_destination(&mut self) {
        let destination = self.destination.trim().to_owned();

        if destination.is_empty() {
            self.status_message = "Destination required".to_string();
            return;
        }

        self.status_message = format!("Destination queued: {destination}");
        debug!(
            destination,
            mode = ?self.mode,
            pinned = self.pin_destination,
            "Destination queued"
        );
    }

    pub fn clear_destination_form(&mut self) {
        self.destination.clear();
        self.notes.clear();
        self.pin_destination = false;
        self.status_message = "Cleared".to_string();
    }

    fn save_logged_in_character(&mut self, character: AuthenticatedCharacter) -> Result<()> {
        let character_config = CharacterConfig {
            character_id: character.character_id,
            character_name: character.character_name.clone(),
            scopes: character.scopes.clone(),
        };

        self.token_store
            .save_refresh_token(character.character_id, &character.refresh_token)?;

        self.config.upsert_character(character_config.clone());
        self.config
            .save()
            .context("Failed to save character metadata")?;

        upsert_character(
            &mut self.characters,
            CharacterState::from_login(
                character_config,
                character.access_token,
                character.expires_in,
            ),
        );

        Ok(())
    }
}

#[derive(Debug)]
pub struct CharacterState {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token_saved: bool,
}

impl CharacterState {
    fn from_config(character: CharacterConfig) -> Self {
        Self {
            character_id: character.character_id,
            character_name: character.character_name,
            scopes: character.scopes,
            access_token: None,
            expires_in: None,
            refresh_token_saved: true,
        }
    }

    fn from_login(character: CharacterConfig, access_token: String, expires_in: u64) -> Self {
        Self {
            character_id: character.character_id,
            character_name: character.character_name,
            scopes: character.scopes,
            access_token: Some(access_token),
            expires_in: Some(expires_in),
            refresh_token_saved: true,
        }
    }

    pub fn token_summary(&self) -> String {
        let access_status = match (&self.access_token, self.expires_in) {
            (Some(_), Some(expires_in)) => format!("access token loaded, expires in {expires_in}s"),
            (Some(_), None) => "access token loaded".to_string(),
            (None, _) => "access token needs refresh".to_string(),
        };
        let refresh_status = if self.refresh_token_saved {
            "refresh token saved"
        } else {
            "no refresh token"
        };

        format!(
            "{} scopes, {access_status}, {refresh_status}",
            self.scopes.len()
        )
    }
}

fn upsert_character(characters: &mut Vec<CharacterState>, character: CharacterState) {
    if let Some(existing) = characters
        .iter_mut()
        .find(|existing| existing.character_id == character.character_id)
    {
        *existing = character;
    } else {
        characters.push(character);
    }
}
