use anyhow::{Context, Result};
use tracing::{error, warn};

use super::SetDestoApp;
use super::models::{FavoriteDestination, ResolvedDestinationDisplay};

impl SetDestoApp {
    pub fn add_favorite_from_input(&mut self) {
        let destination = self.favorite_destination_input.trim().to_string();
        if destination.is_empty() {
            self.status_message = "Favorite destination required".to_string();
            return;
        }

        let resolved_destination = match self.resolve_destination_input(&destination) {
            Ok(destination) => destination,
            Err(err) => {
                warn!(destination = %destination, error = ?err, "Favorite resolution failed");
                self.status_message = err.to_string();
                return;
            }
        };
        let favorite = FavoriteDestination::from_resolved(
            &ResolvedDestinationDisplay::from_resolved(&resolved_destination),
        );

        if self.save_favorite(favorite).is_ok() {
            self.favorite_destination_input.clear();
        }
    }

    pub fn add_resolved_destination_to_favorites(&mut self) {
        let Some(destination) = &self.last_resolved_destination else {
            self.status_message = "Resolve or send a destination before adding it".to_string();
            return;
        };

        let favorite = FavoriteDestination::from_resolved(destination);
        let _ = self.save_favorite(favorite);
    }

    pub fn request_remove_favorite_destination(&mut self, destination_id: i64) {
        let Some(favorite_name) = self
            .favorites
            .iter()
            .find(|favorite| favorite.destination_id == destination_id)
            .map(|favorite| favorite.destination_name.clone())
        else {
            self.status_message = "Favorite destination not found".to_string();
            return;
        };

        self.pending_remove_favorite_destination_id = Some(destination_id);
        self.status_message = format!("Confirm removal of favorite {favorite_name}");
    }

    pub fn cancel_remove_favorite_destination(&mut self) {
        self.pending_remove_favorite_destination_id = None;
    }

    pub fn confirm_remove_favorite_destination(&mut self, destination_id: i64) {
        let Some(favorite_name) = self
            .favorites
            .iter()
            .find(|favorite| favorite.destination_id == destination_id)
            .map(|favorite| favorite.destination_name.clone())
        else {
            self.status_message = "Favorite destination not found".to_string();
            return;
        };

        let mut config = self.config.clone();
        if config.remove_favorite(destination_id).is_none() {
            self.status_message = "Favorite destination not found".to_string();
            return;
        }

        match config.save() {
            Ok(()) => {
                self.config = config;
                self.sync_favorites_from_config();
                self.pending_remove_favorite_destination_id = None;
                self.status_message = format!("Removed favorite {favorite_name}");
            }
            Err(err) => {
                error!(destination_id, error = ?err, "Failed to remove favorite");
                self.status_message = format!("Failed to remove favorite: {err}");
            }
        }
    }

    pub fn set_favorite_destination(&mut self, destination_id: i64) {
        if self.waypoint_send_in_progress() {
            tracing::debug!(
                "Ignoring favorite destination because a waypoint send is already in progress"
            );
            self.status_message = "Waypoint send already in progress".to_string();
            return;
        }

        let Some(favorite) = self
            .favorites
            .iter()
            .find(|favorite| favorite.destination_id == destination_id)
            .cloned()
        else {
            self.status_message = "Favorite destination not found".to_string();
            return;
        };

        self.destination = favorite.destination_name.clone();
        let resolved_destination = ResolvedDestinationDisplay::from_favorite(&favorite);
        self.send_resolved_destination(resolved_destination, &favorite.destination_name);
    }

    fn save_favorite(&mut self, favorite: FavoriteDestination) -> Result<()> {
        let favorite_name = favorite.destination_name.clone();
        let favorite_id = favorite.destination_id;
        if self
            .favorites
            .iter()
            .any(|existing| existing.destination_id == favorite_id)
        {
            self.status_message = format!("{favorite_name} is already a favorite");
            return Ok(());
        }

        let mut config = self.config.clone();
        config.upsert_favorite(favorite.into_config());
        config
            .save()
            .with_context(|| format!("Failed to save favorite {favorite_name}"))?;

        self.config = config;
        self.sync_favorites_from_config();
        self.status_message = format!("Saved favorite {favorite_name} ({favorite_id})");
        Ok(())
    }

    fn sync_favorites_from_config(&mut self) {
        self.favorites = self
            .config
            .favorites
            .iter()
            .cloned()
            .map(FavoriteDestination::from_config)
            .collect();
        self.favorites.sort_by(|left, right| {
            left.destination_name
                .to_lowercase()
                .cmp(&right.destination_name.to_lowercase())
                .then_with(|| left.destination_id.cmp(&right.destination_id))
        });
    }
}
