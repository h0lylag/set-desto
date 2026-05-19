use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::warn;

use crate::domain::destination;
use crate::eve::sso::SsoConfig;
use crate::eve::waypoints::WaypointOptions;
use crate::sde::{RouteDestination, RouteGraph};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterSortColumn {
    Selected,
    Name,
    AddedAt,
}

impl CharacterSortColumn {
    pub fn default_ascending(self) -> bool {
        match self {
            Self::Selected => false,
            Self::Name => true,
            Self::AddedAt => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterSort {
    pub column: CharacterSortColumn,
    pub ascending: bool,
}

impl CharacterSort {
    pub fn for_column(column: CharacterSortColumn) -> Self {
        Self {
            column,
            ascending: column.default_ascending(),
        }
    }

    pub fn toggle_column(&mut self, column: CharacterSortColumn) {
        if self.column == column {
            self.ascending = !self.ascending;
            return;
        }

        *self = Self::for_column(column);
    }

    pub fn marker(self, column: CharacterSortColumn) -> &'static str {
        if self.column != column {
            return "";
        }

        if self.ascending { "^" } else { "v" }
    }
}

impl Default for CharacterSort {
    fn default() -> Self {
        Self::for_column(CharacterSortColumn::Name)
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
            } => pending_summary("Pending", destination_name, *destination_id),
            Self::Sent {
                destination_name,
                destination_id,
            } => pending_summary("Sent", destination_name, *destination_id),
            Self::Failed {
                destination_name,
                destination_id,
                error,
            } => format!(
                "{} - {error}",
                pending_summary("Failed", destination_name, *destination_id)
            ),
            Self::Skipped { reason } => format!("Skipped: {reason}"),
        }
    }
}

fn pending_summary(prefix: &str, destination_name: &str, destination_id: i64) -> String {
    if destination_id > 0 {
        format!("{prefix}: {destination_name} ({destination_id})")
    } else {
        format!("{prefix}: {destination_name}")
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
    pub nickname: String,
}

#[derive(Clone, Debug)]
pub struct FobImportSystem {
    pub import_index: usize,
    pub input_system: String,
    pub resolved_system_id: Option<i64>,
    pub resolved_system_name: Option<String>,
    pub region: String,
    pub last_seen_utc: String,
    pub claimed_by: String,
    pub selected: bool,
    pub error: Option<String>,
}

impl FobImportSystem {
    pub fn display_system(&self) -> &str {
        self.resolved_system_name
            .as_deref()
            .unwrap_or(&self.input_system)
    }

    pub fn valid(&self) -> bool {
        self.resolved_system_id.is_some() && self.error.is_none()
    }
}

impl FavoriteDestination {
    pub(super) fn from_config(config: FavoriteDestinationConfig) -> Self {
        Self {
            destination_id: config.destination_id,
            destination_name: config.destination_name,
            destination_kind: config.destination_kind,
            nickname: config.nickname,
        }
    }

    pub(super) fn into_config(self) -> FavoriteDestinationConfig {
        FavoriteDestinationConfig {
            destination_id: self.destination_id,
            destination_name: self.destination_name,
            destination_kind: self.destination_kind,
            nickname: self.nickname,
        }
    }

    pub(super) fn from_resolved(destination: &ResolvedDestinationDisplay) -> Self {
        Self {
            destination_id: destination.id,
            destination_name: destination.name.clone(),
            destination_kind: destination.kind_label.clone(),
            nickname: String::new(),
        }
    }

    pub fn display_name(&self) -> &str {
        let nickname = self.nickname.trim();
        if nickname.is_empty() {
            &self.destination_name
        } else {
            nickname
        }
    }
}

#[derive(Clone, Debug)]
pub struct WaypointBatchSummary {
    pub destination_name: String,
    pub destination_id: i64,
    pub total: usize,
    pub completed: usize,
    pub active: usize,
    pub successes: usize,
    pub failures: usize,
    pub skipped: usize,
    pub in_progress: bool,
    pub latest_error: Option<String>,
}

impl WaypointBatchSummary {
    pub fn summary_line(&self) -> String {
        let destination = if self.destination_id > 0 {
            format!("{} ({})", self.destination_name, self.destination_id)
        } else {
            self.destination_name.clone()
        };

        if self.in_progress {
            let active = if self.active > 0 {
                format!(", {} sending", self.active)
            } else {
                String::new()
            };
            return format!(
                "{destination} -> {}/{} complete{active}, {} sent, {} failed, {} skipped",
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
            return if self.in_progress { 0.0 } else { 1.0 };
        }

        let progress = self.completed as f32 / self.total as f32;
        if self.in_progress && self.completed >= self.total {
            return 0.99;
        }

        progress
    }

    pub fn progress_text(&self) -> String {
        if self.destination_id == 0 {
            return format!("{}/{} characters", self.completed, self.total);
        }

        if self.in_progress && self.active > 0 {
            return format!(
                "{}/{} done, {} sending",
                self.completed, self.total, self.active
            );
        }

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
    pub(super) active: usize,
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
            active: self.active,
            successes: self.successes,
            failures: self.failures,
            skipped: self.skipped,
            in_progress,
            latest_error: self.latest_error.clone(),
        }
    }

    pub(super) fn status_message(&self) -> String {
        let mut status = format!(
            "Sending {}: {}/{} complete, {} sending, {} sent, {} failed",
            self.destination_name,
            self.completed,
            self.total,
            self.active,
            self.successes,
            self.failures
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
    pub(super) kind: WaypointSendRequestKind,
}

impl WaypointSendRequest {
    pub(super) fn single(
        destination_name: String,
        destination_id: i64,
        options: WaypointOptions,
    ) -> Self {
        Self {
            destination_name,
            destination_id,
            kind: WaypointSendRequestKind::Single { options },
        }
    }

    pub(super) fn optimized_route(
        destinations: Vec<RouteDestination>,
        graph: Arc<RouteGraph>,
    ) -> Self {
        Self {
            destination_name: format!("Optimized FOB route ({} stops)", destinations.len()),
            destination_id: 0,
            kind: WaypointSendRequestKind::OptimizedRoute {
                destinations,
                graph,
            },
        }
    }

    pub(super) fn destination_summary(&self) -> String {
        if self.destination_id > 0 {
            format!("{} ({})", self.destination_name, self.destination_id)
        } else {
            self.destination_name.clone()
        }
    }

    pub(super) fn matches_send_result(&self, destination_name: &str, destination_id: i64) -> bool {
        self.destination_id == destination_id && self.destination_name == destination_name
    }
}

#[derive(Clone, Debug)]
pub(super) enum WaypointSendRequestKind {
    Single {
        options: WaypointOptions,
    },
    OptimizedRoute {
        destinations: Vec<RouteDestination>,
        graph: Arc<RouteGraph>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct WaypointSendJob {
    pub(super) character_id: u64,
    pub(super) character_name: String,
    pub(super) access_token: Option<String>,
    pub(super) expires_at: Option<SystemTime>,
    pub(super) destination_name: String,
    pub(super) destination_id: i64,
    pub(super) kind: WaypointSendRequestKind,
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
    pub added_at_unix_seconds: u64,
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
            added_at_unix_seconds: character.added_at_unix_seconds,
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
            added_at_unix_seconds: character.added_at_unix_seconds,
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

pub(super) fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn has_failed_send_result(character: &CharacterState) -> bool {
    matches!(
        character.last_send_result.as_ref(),
        Some(CharacterSendResult::Failed { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress_summary(total: usize, completed: usize, in_progress: bool) -> WaypointBatchSummary {
        WaypointBatchSummary {
            destination_name: "Jita".to_string(),
            destination_id: 30000142,
            total,
            completed,
            active: 0,
            successes: completed,
            failures: 0,
            skipped: 0,
            in_progress,
            latest_error: None,
        }
    }

    #[test]
    fn in_progress_empty_batch_starts_at_zero_progress() {
        assert_eq!(progress_summary(0, 0, true).progress_fraction(), 0.0);
    }

    #[test]
    fn in_progress_complete_batch_waits_for_finished_event_before_full_progress() {
        assert!(progress_summary(3, 3, true).progress_fraction() < 1.0);
    }

    #[test]
    fn finished_batch_can_show_full_progress() {
        assert_eq!(progress_summary(3, 3, false).progress_fraction(), 1.0);
    }
}
