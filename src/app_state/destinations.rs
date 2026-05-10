use tracing::{debug, info, warn};

use crate::domain::destination;
use crate::eve::waypoints::WaypointOptions;

use super::SetDestoApp;
use super::models::{ResolvedDestinationDisplay, WaypointSendRequest};

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

        let resolved_destination = match destination::resolve(&destination) {
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
}
