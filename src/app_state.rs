use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use eframe::egui;
use tracing::{debug, error, info, warn};

use crate::domain::destination;
use crate::eve::auth::{self, AuthenticatedCharacter, LoginResult, SsoConfig};
use crate::eve::waypoints::{self, WaypointOptions};
use crate::storage::config::{AppConfig, CharacterConfig};
use crate::storage::tokens::{KeyringTokenStore, TokenStore};

const ACCESS_TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(60);

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
        info!(debug_mode, "Initializing Set Desto");

        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let (config, status_message) = match AppConfig::load() {
            Ok(config) => {
                info!(
                    character_count = config.characters.len(),
                    "Loaded application config"
                );
                (config, "Ready".to_string())
            }
            Err(err) => {
                error!(error = ?err, "Failed to load application config");
                (
                    AppConfig::default(),
                    format!("Failed to load config: {err}"),
                )
            }
        };
        let token_store = KeyringTokenStore;
        let characters = config
            .characters
            .iter()
            .cloned()
            .map(|character| CharacterState::from_config(character, &token_store))
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
            token_store,
        }
    }

    pub fn login_in_progress(&self) -> bool {
        self.login_receiver.is_some()
    }

    pub fn start_character_login(&mut self) {
        if self.login_in_progress() {
            debug!("Ignoring Add Character click because login is already in progress");
            return;
        }

        let config = match SsoConfig::from_env() {
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

    pub fn set_destination(&mut self) {
        let destination = self.destination.trim().to_owned();

        if destination.is_empty() {
            warn!("Set Destination blocked because destination is empty");
            self.status_message = "Destination required".to_string();
            return;
        }

        if self.characters.is_empty() {
            warn!("Set Destination requested with no characters added");
            self.status_message = "Add at least one character first".to_string();
            return;
        }

        let selected_character_count = self.selected_character_count();
        if selected_character_count == 0 {
            warn!("Set Destination requested with no selected characters");
            self.status_message = "Select at least one character".to_string();
            return;
        }

        let resolved_destination = match destination::resolve(&destination) {
            Ok(destination) => destination,
            Err(err) => {
                warn!(destination = %destination, error = ?err, "Destination resolution failed");
                self.status_message = err.to_string();
                return;
            }
        };
        let destination_id = resolved_destination.id;

        let selected_characters_with_access_tokens = self
            .characters
            .iter()
            .filter(|character| character.selected && character.has_access_token())
            .count();
        info!(
            destination = %destination,
            mode = ?self.mode,
            pinned = self.pin_destination,
            destination_id,
            destination_name = %resolved_destination.name,
            destination_kind = resolved_destination.kind.label(),
            character_count = self.characters.len(),
            selected_character_count,
            selected_characters_with_access_tokens,
            "Set Destination requested"
        );

        let options = WaypointOptions {
            add_to_beginning: self.pin_destination,
            clear_other_waypoints: !self.pin_destination,
        };
        let mut successes = 0_usize;
        let mut failures = Vec::new();

        for index in 0..self.characters.len() {
            if !self.characters[index].selected {
                continue;
            }

            let character_id = self.characters[index].character_id;
            let character_name = self.characters[index].character_name.clone();
            let result = self
                .access_token_for_character(index)
                .and_then(|access_token| {
                    waypoints::set_waypoint(&access_token, destination_id, options)
                });

            match result {
                Ok(()) => {
                    successes += 1;
                    info!(
                        character_id,
                        character_name, destination_id, "Set waypoint for character"
                    );
                }
                Err(err) => {
                    error!(
                        character_id,
                        character_name,
                        destination_id,
                        error = ?err,
                        "Failed to set waypoint for character"
                    );
                    failures.push(format!("{character_name}: {err}"));
                }
            }
        }

        if failures.is_empty() {
            self.status_message = format!(
                "Set {} ({destination_id}) for {successes} characters",
                resolved_destination.name
            );
        } else {
            self.status_message = format!(
                "Set destination for {successes}/{} characters; first error: {}",
                selected_character_count, failures[0]
            );
        }
    }

    pub fn clear_destination_form(&mut self) {
        self.destination.clear();
        self.notes.clear();
        self.pin_destination = false;
        self.status_message = "Cleared".to_string();
    }

    pub fn selected_character_count(&self) -> usize {
        self.characters
            .iter()
            .filter(|character| character.selected)
            .count()
    }

    pub fn set_character_selected(&mut self, character_id: u64, selected: bool) {
        let Some(character) = self
            .characters
            .iter_mut()
            .find(|character| character.character_id == character_id)
        else {
            warn!(character_id, "Cannot select unknown character");
            return;
        };

        if character.selected == selected {
            return;
        }

        character.selected = selected;
        info!(
            character_id,
            character_name = %character.character_name,
            selected,
            "Updated character selection"
        );
        self.save_character_selection();
    }

    pub fn set_all_characters_selected(&mut self, selected: bool) {
        for character in &mut self.characters {
            character.selected = selected;
        }
        info!(
            selected,
            character_count = self.characters.len(),
            "Updated all character selections"
        );
        self.save_character_selection();
    }

    pub fn invert_character_selection(&mut self) {
        for character in &mut self.characters {
            character.selected = !character.selected;
        }
        info!(
            selected_character_count = self.selected_character_count(),
            character_count = self.characters.len(),
            "Inverted character selection"
        );
        self.save_character_selection();
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

        Ok(())
    }

    fn save_character_selection(&mut self) {
        for character in &self.characters {
            if let Some(config_character) =
                self.config.characters.iter_mut().find(|config_character| {
                    config_character.character_id == character.character_id
                })
            {
                config_character.selected = character.selected;
            }
        }

        if let Err(err) = self.config.save() {
            error!(error = ?err, "Failed to save character selection");
            self.status_message = format!("Failed to save character selection: {err}");
        }
    }

    fn access_token_for_character(&mut self, index: usize) -> Result<String> {
        let character = self
            .characters
            .get(index)
            .ok_or_else(|| anyhow!("Character index {index} was out of range"))?;

        if character.access_token_is_fresh() {
            debug!(
                character_id = character.character_id,
                character_name = %character.character_name,
                "Using cached EVE SSO access token"
            );
            return character
                .access_token
                .clone()
                .ok_or_else(|| anyhow!("Character access token was unexpectedly missing"));
        }

        let character_id = character.character_id;
        let character_name = character.character_name.clone();
        info!(
            character_id,
            character_name, "Refreshing access token before ESI request"
        );

        let refresh_token = self.token_store.load_refresh_token(character_id)?;
        let config = SsoConfig::from_env()?;
        let refreshed = auth::refresh_access_token(&config, &refresh_token)?;

        if refreshed.character_id != character_id {
            bail!(
                "Refreshed token character mismatch: expected {character_id}, got {}",
                refreshed.character_id
            );
        }

        if let Some(refresh_token) = &refreshed.refresh_token {
            self.token_store
                .save_refresh_token(character_id, refresh_token)?;
        }

        let expires_at = expires_at_from_now(refreshed.expires_in);
        self.token_store
            .save_access_token(character_id, &refreshed.access_token, expires_at)?;

        let access_token = refreshed.access_token.clone();
        self.characters[index].update_access_token(
            refreshed.access_token,
            expires_at,
            refreshed.scopes,
        );

        Ok(access_token)
    }
}

#[derive(Debug)]
pub struct CharacterState {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    pub selected: bool,
    access_token: Option<String>,
    expires_at: Option<SystemTime>,
    refresh_token_saved: bool,
}

impl CharacterState {
    fn from_config(character: CharacterConfig, token_store: &impl TokenStore) -> Self {
        let cached_access_token = match token_store.load_access_token(character.character_id) {
            Ok(token) => token,
            Err(err) => {
                warn!(
                    character_id = character.character_id,
                    error = ?err,
                    "Failed to load cached access token"
                );
                None
            }
        };
        let (access_token, expires_at) = cached_access_token
            .map(|token| (Some(token.access_token), Some(token.expires_at)))
            .unwrap_or((None, None));

        Self {
            character_id: character.character_id,
            character_name: character.character_name,
            scopes: character.scopes,
            selected: character.selected,
            access_token,
            expires_at,
            refresh_token_saved: true,
        }
    }

    fn from_login(
        character: CharacterConfig,
        access_token: String,
        expires_at: SystemTime,
    ) -> Self {
        Self {
            character_id: character.character_id,
            character_name: character.character_name,
            scopes: character.scopes,
            selected: character.selected,
            access_token: Some(access_token),
            expires_at: Some(expires_at),
            refresh_token_saved: true,
        }
    }

    pub fn has_access_token(&self) -> bool {
        self.access_token.is_some()
    }

    fn access_token_is_fresh(&self) -> bool {
        self.access_token.is_some()
            && self.expires_at.is_some_and(|expires_at| {
                expires_at > SystemTime::now() + ACCESS_TOKEN_REFRESH_BUFFER
            })
    }

    fn update_access_token(
        &mut self,
        access_token: String,
        expires_at: SystemTime,
        scopes: Vec<String>,
    ) {
        self.access_token = Some(access_token);
        self.expires_at = Some(expires_at);
        self.scopes = scopes;
    }

    pub fn token_summary(&self) -> String {
        let access_status = match (&self.access_token, self.expires_at) {
            (Some(_), Some(expires_at)) => match expires_at.duration_since(SystemTime::now()) {
                Ok(remaining) => {
                    let remaining = remaining.as_secs();
                    format!("access token loaded, expires in {remaining}s")
                }
                Err(_) => "access token expired".to_string(),
            },
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

fn expires_at_from_now(expires_in: u64) -> SystemTime {
    SystemTime::now() + Duration::from_secs(expires_in)
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
