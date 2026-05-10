use std::time::{Duration, SystemTime};

use anyhow::{Error, Result, bail};
use tracing::{debug, info, warn};

use crate::domain::destination;
use crate::eve::{auth, waypoints::WaypointOptions};
use crate::storage::tokens::TokenStore;

use super::SetDestoApp;
use super::models::{ResolvedDestinationDisplay, WaypointSendRequest, expires_at_from_now};

const STRUCTURE_TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(60);

impl SetDestoApp {
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

        let resolved_destination = match self.resolve_destination_input(&destination) {
            Ok(destination) => destination,
            Err(err) => {
                warn!(destination = %destination, error = ?err, "Destination resolution failed");
                self.status_message = err.to_string();
                return;
            }
        };
        self.send_resolved_destination(
            ResolvedDestinationDisplay::from_resolved(&resolved_destination),
            &destination,
        );
    }

    pub(super) fn resolve_destination_input(
        &mut self,
        input: &str,
    ) -> Result<destination::ResolvedDestination> {
        if destination::looks_like_player_structure_id(input) {
            return self.resolve_structure_with_selected_characters(input);
        }

        match destination::resolve(input) {
            Ok(destination) => Ok(destination),
            Err(public_error) => {
                match self.maybe_resolve_structure_with_selected_characters(input) {
                    Ok(Some(destination)) => Ok(destination),
                    Ok(None) => Err(public_error),
                    Err(structure_error) => {
                        let public_error = public_error.to_string();
                        bail!("{public_error}; player structure lookup failed: {structure_error}");
                    }
                }
            }
        }
    }

    pub(super) fn send_resolved_destination(
        &mut self,
        resolved_destination: ResolvedDestinationDisplay,
        destination: &str,
    ) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring Set Destination because a waypoint send is already in progress");
            self.status_message = "Waypoint send already in progress".to_string();
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

        self.last_resolved_destination = Some(resolved_destination.clone());
        let destination_id = resolved_destination.id;
        let selected_characters_with_access_tokens = self
            .characters
            .iter()
            .filter(|character| character.selected && character.has_access_token())
            .count();
        info!(
            destination = %destination,
            pinned = self.pin_destination,
            destination_id,
            destination_name = %resolved_destination.name,
            destination_kind = %resolved_destination.kind_label,
            character_count = self.characters.len(),
            selected_character_count,
            selected_characters_with_access_tokens,
            "Set Destination requested"
        );

        let options = WaypointOptions {
            add_to_beginning: self.pin_destination,
            clear_other_waypoints: !self.pin_destination,
        };
        let request = WaypointSendRequest {
            destination_name: resolved_destination.name.clone(),
            destination_id,
            options,
        };
        let target_ids: Vec<u64> = self
            .characters
            .iter()
            .filter(|character| character.selected)
            .map(|character| character.character_id)
            .collect();
        let skipped = self.characters.len().saturating_sub(target_ids.len());

        self.start_waypoint_send_batch(request, target_ids, skipped, true);
    }

    pub fn clear_destination_form(&mut self) {
        self.destination.clear();
        self.pin_destination = false;
        self.last_resolved_destination = None;
        self.status_message = "Cleared".to_string();
    }

    fn maybe_resolve_structure_with_selected_characters(
        &mut self,
        input: &str,
    ) -> Result<Option<destination::ResolvedDestination>> {
        if self.selected_character_count() == 0 {
            return Ok(None);
        }

        self.resolve_structure_with_selected_characters(input)
            .map(Some)
    }

    fn resolve_structure_with_selected_characters(
        &mut self,
        input: &str,
    ) -> Result<destination::ResolvedDestination> {
        let character_indices: Vec<usize> = self
            .characters
            .iter()
            .enumerate()
            .filter_map(|(index, character)| {
                (character.selected && missing_structure_scopes(&character.scopes).is_empty())
                    .then_some(index)
            })
            .collect();

        if character_indices.is_empty() {
            let Some(character) = self.characters.iter().find(|character| character.selected)
            else {
                bail!("Select at least one character to resolve player structures");
            };

            let missing = missing_structure_scopes(&character.scopes).join(", ");
            bail!(
                "{} needs to be re-authenticated with player structure scopes: {missing}",
                character.character_name
            );
        };

        let mut last_error: Option<Error> = None;
        for character_index in character_indices {
            let character_name = self.characters[character_index].character_name.clone();
            let context = match self.structure_resolution_context_for_index(character_index) {
                Ok(context) => context,
                Err(err) => {
                    warn!(
                        character_name = %character_name,
                        error = ?err,
                        "Failed to prepare selected character for player structure lookup"
                    );
                    last_error = Some(err);
                    continue;
                }
            };

            match destination::resolve_structure(input, &context) {
                Ok(destination) => return Ok(destination),
                Err(err) => {
                    warn!(
                        character_id = context.character_id,
                        character_name = %character_name,
                        error = ?err,
                        "Player structure lookup failed with selected character"
                    );
                    last_error = Some(err);
                }
            }
        }

        match last_error {
            Some(err) => Err(err),
            None => bail!("No selected character could resolve the player structure"),
        }
    }

    fn structure_resolution_context_for_index(
        &mut self,
        character_index: usize,
    ) -> Result<destination::StructureResolutionContext> {
        let missing = missing_structure_scopes(&self.characters[character_index].scopes);
        if !missing.is_empty() {
            bail!(
                "{} needs to be re-authenticated with player structure scopes: {}",
                self.characters[character_index].character_name,
                missing.join(", ")
            );
        }

        let character_id = self.characters[character_index].character_id;
        let character_name = self.characters[character_index].character_name.clone();

        if access_token_is_fresh(
            &self.characters[character_index].access_token,
            self.characters[character_index].expires_at,
        ) {
            debug!(
                character_id,
                character_name = %character_name,
                "Using cached EVE SSO access token for player structure lookup"
            );
            if let Some(access_token) = self.characters[character_index].access_token.clone() {
                return Ok(destination::StructureResolutionContext {
                    character_id,
                    access_token,
                });
            }
        }

        info!(
            character_id,
            character_name = %character_name,
            "Refreshing access token for player structure lookup"
        );

        let sso_config = self.sso_config()?;
        let refresh_token = self.token_store.load_refresh_token(character_id)?;
        let refreshed = auth::refresh_access_token(&sso_config, &refresh_token)?;

        if refreshed.character_id != character_id {
            bail!(
                "Refreshed token character mismatch: expected {}, got {}",
                character_id,
                refreshed.character_id
            );
        }

        let missing = missing_structure_scopes(&refreshed.scopes);
        if !missing.is_empty() {
            bail!(
                "{} needs to be re-authenticated with player structure scopes: {}",
                character_name,
                missing.join(", ")
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
        self.characters[character_index].update_access_token(
            refreshed.access_token,
            expires_at,
            refreshed.scopes,
        );

        Ok(destination::StructureResolutionContext {
            character_id,
            access_token,
        })
    }
}

fn access_token_is_fresh(access_token: &Option<String>, expires_at: Option<SystemTime>) -> bool {
    access_token.is_some()
        && expires_at.is_some_and(|expires_at| {
            expires_at > SystemTime::now() + STRUCTURE_TOKEN_REFRESH_BUFFER
        })
}

fn missing_structure_scopes(scopes: &[String]) -> Vec<&'static str> {
    [auth::SCOPE_SEARCH_STRUCTURES, auth::SCOPE_READ_STRUCTURES]
        .into_iter()
        .filter(|scope| !scopes.iter().any(|existing| existing == scope))
        .collect()
}
