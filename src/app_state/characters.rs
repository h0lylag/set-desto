use std::cmp::Ordering;

use anyhow::{Context, Result, anyhow};
use tracing::{debug, error, info, warn};

use crate::storage::tokens::TokenStore;

use super::{CharacterSortColumn, CharacterState, SetDestoApp};

impl SetDestoApp {
    pub fn set_character_sort_column(&mut self, column: CharacterSortColumn) {
        self.character_sort.toggle_column(column);
    }

    pub fn character_sort_marker(&self, column: CharacterSortColumn) -> &'static str {
        self.character_sort.marker(column)
    }

    pub fn sorted_character_ids(&self) -> Vec<u64> {
        let mut characters: Vec<&CharacterState> = self.characters.iter().collect();
        characters.sort_by(|left, right| self.compare_characters(left, right));

        characters
            .into_iter()
            .map(|character| character.character_id)
            .collect()
    }

    pub fn selected_character_count(&self) -> usize {
        self.characters
            .iter()
            .filter(|character| character.selected)
            .count()
    }

    pub fn set_character_selected(&mut self, character_id: u64, selected: bool) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring character selection because a waypoint send is in progress");
            self.status_message = "Wait for the current waypoint send to finish".to_string();
            return;
        }

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
        let character_name = character.character_name.clone();
        self.clear_send_results();
        info!(
            character_id,
            character_name = %character_name,
            selected,
            "Updated character selection"
        );
        self.save_character_selection();
    }

    pub fn set_all_characters_selected(&mut self, selected: bool) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring bulk character selection because a waypoint send is in progress");
            self.status_message = "Wait for the current waypoint send to finish".to_string();
            return;
        }

        let changed = self
            .characters
            .iter()
            .any(|character| character.selected != selected);
        if !changed {
            return;
        }

        for character in &mut self.characters {
            character.selected = selected;
        }
        self.clear_send_results();
        info!(
            selected,
            character_count = self.characters.len(),
            "Updated all character selections"
        );
        self.save_character_selection();
    }

    pub fn invert_character_selection(&mut self) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring invert character selection because a waypoint send is in progress");
            self.status_message = "Wait for the current waypoint send to finish".to_string();
            return;
        }

        if self.characters.is_empty() {
            return;
        }

        for character in &mut self.characters {
            character.selected = !character.selected;
        }
        self.clear_send_results();
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
        self.clear_send_results();

        Ok(character_name)
    }

    pub(super) fn clear_send_results(&mut self) {
        for character in &mut self.characters {
            character.last_send_result = None;
        }
        self.last_waypoint_batch = None;
        self.last_waypoint_request = None;
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

    fn compare_characters(&self, left: &CharacterState, right: &CharacterState) -> Ordering {
        let ordering = match self.character_sort.column {
            CharacterSortColumn::Selected => left.selected.cmp(&right.selected),
            CharacterSortColumn::Name => left.character_name.cmp(&right.character_name),
            CharacterSortColumn::AddedAt => {
                left.added_at_unix_seconds.cmp(&right.added_at_unix_seconds)
            }
        };

        let ordering = if self.character_sort.ascending {
            ordering
        } else {
            ordering.reverse()
        };

        ordering
            .then_with(|| left.character_name.cmp(&right.character_name))
            .then_with(|| left.character_id.cmp(&right.character_id))
    }
}
