use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use anyhow::Result;
use eframe::egui;
use tracing::{error, info};

use crate::eve::sso::{LoginResult, SsoConfig};
use crate::eve::waypoints::WaypointRouteMode;
use crate::sde::{RouteGraph, SdeCacheEvent};
use crate::storage::config::AppConfig;
use crate::storage::tokens::KeyringTokenStore;

mod characters;
mod destinations;
mod favorites;
mod imports;
mod login;
mod models;
mod sde_cache;
mod settings;
mod waypoint_batches;
mod waypoint_worker;

pub use models::{
    AppTab, CharacterSendResult, CharacterSort, CharacterSortColumn, CharacterState,
    FavoriteDestination, FobImportSystem, WaypointBatchSummary,
};

use models::{
    ResolvedDestinationDisplay, WaypointSendEvent, WaypointSendProgress, WaypointSendRequest,
};

pub struct SetDestoApp {
    pub debug_mode: bool,
    pub active_tab: AppTab,
    pub characters: Vec<CharacterState>,
    pub character_sort: CharacterSort,
    pub destination: String,
    pub favorite_destination_input: String,
    pub favorites: Vec<FavoriteDestination>,
    pub favorite_nickname_edits: BTreeMap<i64, String>,
    pub waypoint_route_mode: WaypointRouteMode,
    pub status_message: String,
    pub sde_cache_status: String,
    pub esi_client_id: String,
    pub fob_import_open: bool,
    pub fob_import_text: String,
    pub fob_import_rows: Vec<FobImportSystem>,
    pub fob_import_messages: Vec<String>,
    pub sde_route_graph: Option<Arc<RouteGraph>>,
    last_resolved_destination: Option<ResolvedDestinationDisplay>,
    pub pending_remove_character_id: Option<u64>,
    pub pending_remove_favorite_destination_id: Option<i64>,
    config: AppConfig,
    login_receiver: Option<Receiver<LoginResult>>,
    sde_cache_receiver: Option<Receiver<SdeCacheEvent>>,
    waypoint_send_receiver: Option<Receiver<WaypointSendEvent>>,
    waypoint_send_progress: Option<WaypointSendProgress>,
    last_waypoint_batch: Option<WaypointBatchSummary>,
    last_waypoint_request: Option<WaypointSendRequest>,
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
        let (sde_route_graph, sde_cache_status) = match crate::sde::load_cached_graph() {
            Ok(graph) => (
                Some(graph),
                "Loaded cached SDE route graph; checking for updates...".to_string(),
            ),
            Err(_) => (
                None,
                "Downloading SDE route graph in the background...".to_string(),
            ),
        };
        let sde_cache_receiver = Some(crate::sde::start_cache_refresh(
            sde_route_graph.as_ref().map(|graph| graph.build_number()),
        ));
        let esi_client_id = config.esi.client_id.clone();
        let favorites: Vec<FavoriteDestination> = config
            .favorites
            .iter()
            .cloned()
            .map(FavoriteDestination::from_config)
            .collect();
        let favorite_nickname_edits = favorite_nickname_edits(&favorites);
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
            character_sort: CharacterSort::default(),
            destination: String::new(),
            favorite_destination_input: String::new(),
            favorites,
            favorite_nickname_edits,
            waypoint_route_mode: WaypointRouteMode::default(),
            status_message,
            sde_cache_status,
            esi_client_id,
            fob_import_open: false,
            fob_import_text: String::new(),
            fob_import_rows: Vec::new(),
            fob_import_messages: Vec::new(),
            sde_route_graph,
            last_resolved_destination: None,
            pending_remove_character_id: None,
            pending_remove_favorite_destination_id: None,
            config,
            login_receiver: None,
            sde_cache_receiver,
            waypoint_send_receiver: None,
            waypoint_send_progress: None,
            last_waypoint_batch: None,
            last_waypoint_request: None,
            token_store,
        }
    }

    pub fn login_in_progress(&self) -> bool {
        self.login_receiver.is_some()
    }

    pub fn waypoint_send_in_progress(&self) -> bool {
        self.waypoint_send_receiver.is_some()
    }

    pub fn waypoint_batch_summary(&self) -> Option<WaypointBatchSummary> {
        if let Some(progress) = &self.waypoint_send_progress {
            return Some(progress.summary(true));
        }

        self.last_waypoint_batch.clone()
    }

    pub fn resolved_destination_summary(&self) -> Option<String> {
        self.last_resolved_destination
            .as_ref()
            .map(ResolvedDestinationDisplay::summary)
    }

    pub fn clear_resolved_destination(&mut self) {
        self.last_resolved_destination = None;
    }

    pub fn favorite_send_enabled(&self) -> bool {
        !self.waypoint_send_in_progress()
            && !self.characters.is_empty()
            && self.selected_character_count() > 0
    }

    pub(super) fn sso_config(&self) -> Result<SsoConfig> {
        SsoConfig::from_client_id(&self.esi_client_id)
    }

    pub(super) fn character_mut(&mut self, character_id: u64) -> Option<&mut CharacterState> {
        self.characters
            .iter_mut()
            .find(|character| character.character_id == character_id)
    }
}

pub(super) fn favorite_nickname_edits(favorites: &[FavoriteDestination]) -> BTreeMap<i64, String> {
    favorites
        .iter()
        .map(|favorite| (favorite.destination_id, favorite.nickname.clone()))
        .collect()
}

pub(super) fn upsert_character(characters: &mut Vec<CharacterState>, character: CharacterState) {
    if let Some(existing) = characters
        .iter_mut()
        .find(|existing| existing.character_id == character.character_id)
    {
        *existing = character;
    } else {
        characters.push(character);
    }
}
