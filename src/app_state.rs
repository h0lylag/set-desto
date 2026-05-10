use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppTab {
    Destination,
    Characters,
}

impl AppTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Destination => "Set Destination",
            Self::Characters => "Characters",
        }
    }
}

#[derive(Clone, Debug)]
pub enum CharacterSendResult {
    Pending {
        destination_name: String,
        destination_id: i64,
    },
    Sent {
        destination_name: String,
        destination_id: i64,
    },
    Failed {
        destination_name: String,
        destination_id: i64,
        error: String,
    },
    Skipped {
        reason: String,
    },
}

impl CharacterSendResult {
    pub fn summary(&self) -> String {
        match self {
            Self::Pending {
                destination_name,
                destination_id,
            } => format!("Pending: {destination_name} ({destination_id})"),
            Self::Sent {
                destination_name,
                destination_id,
            } => format!("Sent: {destination_name} ({destination_id})"),
            Self::Failed {
                destination_name,
                destination_id,
                error,
            } => format!("Failed: {destination_name} ({destination_id}) - {error}"),
            Self::Skipped { reason } => format!("Skipped: {reason}"),
        }
    }
}

#[derive(Debug)]
struct WaypointSendProgress {
    destination_name: String,
    destination_id: i64,
    total: usize,
    completed: usize,
    successes: usize,
    failures: usize,
    latest_error: Option<String>,
}

impl WaypointSendProgress {
    fn status_message(&self) -> String {
        if let Some(error) = &self.latest_error {
            return format!(
                "Processed {}/{} for {} ({} sent, {} failed); latest error: {error}",
                self.completed, self.total, self.destination_name, self.successes, self.failures
            );
        }

        format!(
            "Sending {} ({}) to {}/{} characters...",
            self.destination_name, self.destination_id, self.completed, self.total
        )
    }

    fn final_status_message(&self) -> String {
        if let Some(error) = &self.latest_error {
            return format!(
                "Set destination for {}/{} characters ({} failed); latest error: {error}",
                self.successes, self.total, self.failures
            );
        }

        format!(
            "Set {} ({}) for {} characters",
            self.destination_name, self.destination_id, self.successes
        )
    }
}

#[derive(Clone, Debug)]
struct WaypointSendJob {
    character_id: u64,
    character_name: String,
    access_token: Option<String>,
    expires_at: Option<SystemTime>,
    destination_name: String,
    destination_id: i64,
    options: WaypointOptions,
    token_store: KeyringTokenStore,
}

#[derive(Debug)]
enum WaypointSendEvent {
    Started {
        character_id: u64,
        character_name: String,
    },
    Finished {
        character_id: u64,
        character_name: String,
        destination_name: String,
        destination_id: i64,
        result: std::result::Result<WaypointSendSuccess, String>,
    },
    BatchFinished,
}

#[derive(Debug)]
struct WaypointSendSuccess {
    access_token_update: Option<AccessTokenUpdate>,
}

#[derive(Debug)]
struct AccessTokenUpdate {
    access_token: String,
    expires_at: SystemTime,
    scopes: Vec<String>,
}

pub struct SetDestoApp {
    pub debug_mode: bool,
    pub active_tab: AppTab,
    pub characters: Vec<CharacterState>,
    pub destination: String,
    pub pin_destination: bool,
    pub mode: DestoMode,
    pub status_message: String,
    pub pending_remove_character_id: Option<u64>,
    config: AppConfig,
    login_receiver: Option<Receiver<LoginResult>>,
    waypoint_send_receiver: Option<Receiver<WaypointSendEvent>>,
    waypoint_send_progress: Option<WaypointSendProgress>,
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
            active_tab: AppTab::Destination,
            characters,
            destination: String::new(),
            pin_destination: false,
            mode: DestoMode::Manual,
            status_message,
            pending_remove_character_id: None,
            config,
            login_receiver: None,
            waypoint_send_receiver: None,
            waypoint_send_progress: None,
            token_store,
        }
    }

    pub fn login_in_progress(&self) -> bool {
        self.login_receiver.is_some()
    }

    pub fn waypoint_send_in_progress(&self) -> bool {
        self.waypoint_send_receiver.is_some()
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

    pub fn poll_waypoint_send(&mut self) {
        loop {
            let event = match self
                .waypoint_send_receiver
                .as_ref()
                .map(|receiver| receiver.try_recv())
            {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Empty)) | None => return,
                Some(Err(TryRecvError::Disconnected)) => {
                    error!("Waypoint send worker disconnected");
                    self.status_message = "Waypoint send failed: worker stopped".to_string();
                    self.waypoint_send_receiver = None;
                    self.waypoint_send_progress = None;
                    return;
                }
            };

            self.handle_waypoint_send_event(event);
        }
    }

    pub fn set_destination(&mut self) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring Set Destination because a waypoint send is already in progress");
            self.status_message = "Waypoint send already in progress".to_string();
            return;
        }

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
        self.prepare_send_results(&resolved_destination.name, destination_id);

        let jobs = self.waypoint_send_jobs(&resolved_destination.name, destination_id, options);
        let total = jobs.len();
        let (sender, receiver) = mpsc::channel();
        start_waypoint_send(jobs, sender);

        self.waypoint_send_receiver = Some(receiver);
        self.waypoint_send_progress = Some(WaypointSendProgress {
            destination_name: resolved_destination.name.clone(),
            destination_id,
            total,
            completed: 0,
            successes: 0,
            failures: 0,
            latest_error: None,
        });
        self.status_message = format!(
            "Sending {} ({destination_id}) to {total} characters...",
            resolved_destination.name
        );
    }

    pub fn clear_destination_form(&mut self) {
        self.destination.clear();
        self.pin_destination = false;
        self.status_message = "Cleared".to_string();
    }

    fn prepare_send_results(&mut self, destination_name: &str, destination_id: i64) {
        for character in &mut self.characters {
            character.last_send_result = Some(if character.selected {
                CharacterSendResult::Pending {
                    destination_name: destination_name.to_string(),
                    destination_id,
                }
            } else {
                CharacterSendResult::Skipped {
                    reason: "not selected".to_string(),
                }
            });
        }
    }

    fn waypoint_send_jobs(
        &self,
        destination_name: &str,
        destination_id: i64,
        options: WaypointOptions,
    ) -> Vec<WaypointSendJob> {
        self.characters
            .iter()
            .filter(|character| character.selected)
            .map(|character| WaypointSendJob {
                character_id: character.character_id,
                character_name: character.character_name.clone(),
                access_token: character.access_token.clone(),
                expires_at: character.expires_at,
                destination_name: destination_name.to_string(),
                destination_id,
                options,
                token_store: self.token_store,
            })
            .collect()
    }

    fn handle_waypoint_send_event(&mut self, event: WaypointSendEvent) {
        match event {
            WaypointSendEvent::Started {
                character_id,
                character_name,
            } => {
                debug!(character_id, character_name, "Waypoint send started");
                self.status_message = format!("Sending waypoint for {character_name}...");
            }
            WaypointSendEvent::Finished {
                character_id,
                character_name,
                destination_name,
                destination_id,
                result,
            } => {
                self.record_waypoint_send_result(
                    character_id,
                    character_name,
                    destination_name,
                    destination_id,
                    result,
                );
            }
            WaypointSendEvent::BatchFinished => {
                self.finish_waypoint_send();
            }
        }
    }

    fn record_waypoint_send_result(
        &mut self,
        character_id: u64,
        character_name: String,
        destination_name: String,
        destination_id: i64,
        result: std::result::Result<WaypointSendSuccess, String>,
    ) {
        let status_message = {
            let Some(progress) = &mut self.waypoint_send_progress else {
                return;
            };
            progress.completed += 1;

            match &result {
                Ok(_) => {
                    progress.successes += 1;
                }
                Err(error) => {
                    progress.failures += 1;
                    progress.latest_error = Some(format!("{character_name}: {error}"));
                }
            }

            progress.status_message()
        };

        match result {
            Ok(success) => {
                if let Some(update) = success.access_token_update {
                    self.apply_access_token_update(character_id, update);
                }
                if let Some(character) = self.character_mut(character_id) {
                    character.last_send_result = Some(CharacterSendResult::Sent {
                        destination_name: destination_name.clone(),
                        destination_id,
                    });
                }
                info!(
                    character_id,
                    character_name, destination_id, "Set waypoint for character"
                );
            }
            Err(error) => {
                if let Some(character) = self.character_mut(character_id) {
                    character.last_send_result = Some(CharacterSendResult::Failed {
                        destination_name: destination_name.clone(),
                        destination_id,
                        error: error.clone(),
                    });
                }
                error!(
                    character_id,
                    character_name, destination_id, error, "Failed to set waypoint for character"
                );
            }
        }

        self.status_message = status_message;
    }

    fn finish_waypoint_send(&mut self) {
        if let Some(progress) = &self.waypoint_send_progress {
            self.status_message = progress.final_status_message();
        }
        self.waypoint_send_receiver = None;
        self.waypoint_send_progress = None;
    }

    fn apply_access_token_update(&mut self, character_id: u64, update: AccessTokenUpdate) {
        if let Some(character) = self.character_mut(character_id) {
            character.update_access_token(update.access_token, update.expires_at, update.scopes);
        }
    }

    fn character_mut(&mut self, character_id: u64) -> Option<&mut CharacterState> {
        self.characters
            .iter_mut()
            .find(|character| character.character_id == character_id)
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

    pub fn request_remove_character(&mut self, character_id: u64) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring Remove Character because a waypoint send is in progress");
            self.status_message = "Wait for the current waypoint send to finish".to_string();
            return;
        }

        let Some(character) = self
            .characters
            .iter()
            .find(|character| character.character_id == character_id)
        else {
            warn!(character_id, "Cannot remove unknown character");
            return;
        };

        info!(
            character_id,
            character_name = %character.character_name,
            "Character removal confirmation requested"
        );
        self.pending_remove_character_id = Some(character_id);
    }

    pub fn cancel_remove_character(&mut self) {
        self.pending_remove_character_id = None;
    }

    pub fn confirm_remove_character(&mut self, character_id: u64) {
        match self.remove_character(character_id) {
            Ok(character_name) => {
                info!(character_id, character_name, "Removed character");
                self.status_message = format!("Removed {character_name}");
            }
            Err(err) => {
                error!(character_id, error = ?err, "Failed to remove character");
                self.status_message = format!("Failed to remove character: {err}");
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

        Ok(())
    }

    fn remove_character(&mut self, character_id: u64) -> Result<String> {
        let character_name = self
            .characters
            .iter()
            .find(|character| character.character_id == character_id)
            .map(|character| character.character_name.clone())
            .ok_or_else(|| anyhow!("Character {character_id} was not found"))?;

        self.token_store
            .delete_access_token(character_id)
            .with_context(|| format!("Failed to delete access token for {character_name}"))?;
        self.token_store
            .delete_refresh_token(character_id)
            .with_context(|| format!("Failed to delete refresh token for {character_name}"))?;

        let mut config = self.config.clone();
        config
            .remove_character(character_id)
            .ok_or_else(|| anyhow!("Character {character_id} was missing from config"))?;
        config
            .save()
            .context("Failed to save character metadata after removal")?;
        self.config = config;

        self.characters
            .retain(|character| character.character_id != character_id);
        if self.pending_remove_character_id == Some(character_id) {
            self.pending_remove_character_id = None;
        }

        Ok(character_name)
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
}

#[derive(Debug)]
pub struct CharacterState {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    pub selected: bool,
    pub last_send_result: Option<CharacterSendResult>,
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
            last_send_result: None,
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
            last_send_result: None,
            access_token: Some(access_token),
            expires_at: Some(expires_at),
            refresh_token_saved: true,
        }
    }

    pub fn has_access_token(&self) -> bool {
        self.access_token.is_some()
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

fn start_waypoint_send(jobs: Vec<WaypointSendJob>, sender: Sender<WaypointSendEvent>) {
    thread::spawn(move || {
        let mut handles = Vec::with_capacity(jobs.len());

        for job in jobs {
            let sender = sender.clone();
            handles.push(thread::spawn(move || {
                let _ = sender.send(WaypointSendEvent::Started {
                    character_id: job.character_id,
                    character_name: job.character_name.clone(),
                });

                let result = run_waypoint_send_job(&job).map_err(|err| err.to_string());
                let _ = sender.send(WaypointSendEvent::Finished {
                    character_id: job.character_id,
                    character_name: job.character_name,
                    destination_name: job.destination_name,
                    destination_id: job.destination_id,
                    result,
                });
            }));
        }

        for handle in handles {
            if handle.join().is_err() {
                error!("Waypoint send worker panicked");
            }
        }

        let _ = sender.send(WaypointSendEvent::BatchFinished);
    });
}

fn run_waypoint_send_job(job: &WaypointSendJob) -> Result<WaypointSendSuccess> {
    let (access_token, access_token_update) = access_token_for_job(job)?;
    waypoints::set_waypoint(&access_token, job.destination_id, job.options)?;

    Ok(WaypointSendSuccess {
        access_token_update,
    })
}

fn access_token_for_job(job: &WaypointSendJob) -> Result<(String, Option<AccessTokenUpdate>)> {
    if access_token_is_fresh(&job.access_token, job.expires_at) {
        debug!(
            character_id = job.character_id,
            character_name = %job.character_name,
            "Using cached EVE SSO access token"
        );
        let access_token = job
            .access_token
            .clone()
            .ok_or_else(|| anyhow!("Character access token was unexpectedly missing"))?;
        return Ok((access_token, None));
    }

    info!(
        character_id = job.character_id,
        character_name = %job.character_name,
        "Refreshing access token before ESI request"
    );

    let refresh_token = job.token_store.load_refresh_token(job.character_id)?;
    let config = SsoConfig::from_env()?;
    let refreshed = auth::refresh_access_token(&config, &refresh_token)?;

    if refreshed.character_id != job.character_id {
        bail!(
            "Refreshed token character mismatch: expected {}, got {}",
            job.character_id,
            refreshed.character_id
        );
    }

    if let Some(refresh_token) = &refreshed.refresh_token {
        job.token_store
            .save_refresh_token(job.character_id, refresh_token)?;
    }

    let expires_at = expires_at_from_now(refreshed.expires_in);
    job.token_store
        .save_access_token(job.character_id, &refreshed.access_token, expires_at)?;

    let access_token = refreshed.access_token.clone();
    let update = AccessTokenUpdate {
        access_token: refreshed.access_token,
        expires_at,
        scopes: refreshed.scopes,
    };

    Ok((access_token, Some(update)))
}

fn access_token_is_fresh(access_token: &Option<String>, expires_at: Option<SystemTime>) -> bool {
    access_token.is_some()
        && expires_at
            .is_some_and(|expires_at| expires_at > SystemTime::now() + ACCESS_TOKEN_REFRESH_BUFFER)
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
