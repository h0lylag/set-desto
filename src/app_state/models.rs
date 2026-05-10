use std::time::{Duration, SystemTime};

use tracing::warn;

use crate::domain::destination;
use crate::eve::sso::SsoConfig;
use crate::eve::waypoints::WaypointOptions;
use crate::storage::config::{CharacterConfig, FavoriteDestinationConfig};
use crate::storage::tokens::{KeyringTokenStore, TokenStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppTab {
    Destination,
    Characters,
    Favorites,
    Esi,
}

impl AppTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Destination => "Set Destination",
            Self::Characters => "Characters",
            Self::Favorites => "Favorites",
            Self::Esi => "ESI",
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

#[derive(Clone, Debug)]
pub(super) struct ResolvedDestinationDisplay {
    pub(super) name: String,
    pub(super) id: i64,
    pub(super) kind_label: String,
}

impl ResolvedDestinationDisplay {
    pub(super) fn from_resolved(destination: &destination::ResolvedDestination) -> Self {
        Self {
            name: destination.name.clone(),
            id: destination.id,
            kind_label: destination.kind.label().to_string(),
        }
    }

    pub(super) fn from_favorite(favorite: &FavoriteDestination) -> Self {
        Self {
            name: favorite.destination_name.clone(),
            id: favorite.destination_id,
            kind_label: favorite.destination_kind.clone(),
        }
    }

    pub(super) fn summary(&self) -> String {
        if self.name == self.id.to_string() {
            return format!("{}, {}", self.id, self.kind_label);
        }

        format!("{} ({}), {}", self.name, self.id, self.kind_label)
    }
}

#[derive(Clone, Debug)]
pub struct FavoriteDestination {
    pub destination_id: i64,
    pub destination_name: String,
    pub destination_kind: String,
}

impl FavoriteDestination {
    pub(super) fn from_config(config: FavoriteDestinationConfig) -> Self {
        Self {
            destination_id: config.destination_id,
            destination_name: config.destination_name,
            destination_kind: config.destination_kind,
        }
    }

    pub(super) fn into_config(self) -> FavoriteDestinationConfig {
        FavoriteDestinationConfig {
            destination_id: self.destination_id,
            destination_name: self.destination_name,
            destination_kind: self.destination_kind,
        }
    }

    pub(super) fn from_resolved(destination: &ResolvedDestinationDisplay) -> Self {
        Self {
            destination_id: destination.id,
            destination_name: destination.name.clone(),
            destination_kind: destination.kind_label.clone(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} ({}) - {}",
            self.destination_name, self.destination_id, self.destination_kind
        )
    }
}

#[derive(Clone, Debug)]
pub struct WaypointBatchSummary {
    pub destination_name: String,
    pub destination_id: i64,
    pub total: usize,
    pub completed: usize,
    pub successes: usize,
    pub failures: usize,
    pub skipped: usize,
    pub in_progress: bool,
    pub latest_error: Option<String>,
}

impl WaypointBatchSummary {
    pub fn summary_line(&self) -> String {
        let destination = format!("{} ({})", self.destination_name, self.destination_id);

        if self.in_progress {
            return format!(
                "{destination} -> {}/{} complete, {} sent, {} failed, {} skipped",
                self.completed, self.total, self.successes, self.failures, self.skipped
            );
        }

        format!(
            "{destination} -> {} sent, {} failed, {} skipped",
            self.successes, self.failures, self.skipped
        )
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }

        self.completed as f32 / self.total as f32
    }

    pub fn progress_text(&self) -> String {
        format!("{}/{}", self.completed, self.total)
    }

    pub(super) fn status_message(&self) -> String {
        let mut status = self.summary_line();

        if let Some(error) = &self.latest_error {
            status.push_str(&format!("; latest error: {error}"));
        }

        status
    }
}

#[derive(Debug)]
pub(super) struct WaypointSendProgress {
    pub(super) destination_name: String,
    pub(super) destination_id: i64,
    pub(super) total: usize,
    pub(super) completed: usize,
    pub(super) successes: usize,
    pub(super) failures: usize,
    pub(super) skipped: usize,
    pub(super) latest_error: Option<String>,
}

impl WaypointSendProgress {
    pub(super) fn summary(&self, in_progress: bool) -> WaypointBatchSummary {
        WaypointBatchSummary {
            destination_name: self.destination_name.clone(),
            destination_id: self.destination_id,
            total: self.total,
            completed: self.completed,
            successes: self.successes,
            failures: self.failures,
            skipped: self.skipped,
            in_progress,
            latest_error: self.latest_error.clone(),
        }
    }

    pub(super) fn status_message(&self) -> String {
        let mut status = format!(
            "Sending {}: {}/{} complete, {} sent, {} failed",
            self.destination_name, self.completed, self.total, self.successes, self.failures
        );

        if let Some(error) = &self.latest_error {
            status.push_str(&format!("; latest error: {error}"));
        }

        status
    }
}

#[derive(Clone, Debug)]
pub(super) struct WaypointSendRequest {
    pub(super) destination_name: String,
    pub(super) destination_id: i64,
    pub(super) options: WaypointOptions,
}

#[derive(Clone, Debug)]
pub(super) struct WaypointSendJob {
    pub(super) character_id: u64,
    pub(super) character_name: String,
    pub(super) access_token: Option<String>,
    pub(super) expires_at: Option<SystemTime>,
    pub(super) destination_name: String,
    pub(super) destination_id: i64,
    pub(super) options: WaypointOptions,
    pub(super) token_store: KeyringTokenStore,
    pub(super) sso_config: SsoConfig,
}

#[derive(Debug)]
pub(super) enum WaypointSendEvent {
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
pub(super) struct WaypointSendSuccess {
    pub(super) access_token_update: Option<AccessTokenUpdate>,
}

#[derive(Debug)]
pub(super) struct AccessTokenUpdate {
    pub(super) access_token: String,
    pub(super) expires_at: SystemTime,
    pub(super) scopes: Vec<String>,
}

#[derive(Debug)]
pub struct CharacterState {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    pub selected: bool,
    pub last_send_result: Option<CharacterSendResult>,
    pub(super) access_token: Option<String>,
    pub(super) expires_at: Option<SystemTime>,
    refresh_token_saved: bool,
}

impl CharacterState {
    pub(super) fn from_config(character: CharacterConfig, token_store: &impl TokenStore) -> Self {
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

    pub(super) fn from_login(
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

    pub(super) fn update_access_token(
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

pub(super) fn expires_at_from_now(expires_in: u64) -> SystemTime {
    SystemTime::now() + Duration::from_secs(expires_in)
}

pub(super) fn has_failed_send_result(character: &CharacterState) -> bool {
    matches!(
        character.last_send_result.as_ref(),
        Some(CharacterSendResult::Failed { .. })
    )
}
