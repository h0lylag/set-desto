use std::cmp::Ordering;
use std::collections::HashMap;

use tracing::{info, warn};

use crate::eve::sso;
use crate::fobscout;
use crate::sde::{MAX_OPTIMIZED_ROUTE_STOPS, RouteDestination};

use super::SetDestoApp;
use super::models::{FobImportSortColumn, FobImportSystem, WaypointSendRequest};

impl SetDestoApp {
    pub fn open_fob_import(&mut self) {
        self.fob_import_open = true;
        self.refresh_fob_import_preview();
    }

    pub fn close_fob_import(&mut self) {
        self.fob_import_open = false;
    }

    pub fn refresh_fob_import_preview(&mut self) {
        let previous_selection: HashMap<String, bool> = self
            .fob_import_rows
            .iter()
            .map(|row| (row.input_system.to_ascii_lowercase(), row.selected))
            .collect();
        let parsed = fobscout::parse_export(&self.fob_import_text);
        let graph = self.sde_route_graph.as_ref();

        self.fob_import_messages = parsed.messages;
        self.fob_import_rows = parsed
            .rows
            .into_iter()
            .map(|row| {
                let previous_selected = previous_selection
                    .get(&row.system.to_ascii_lowercase())
                    .copied();
                let resolved = graph.and_then(|graph| graph.resolve_system(&row.system));
                let (resolved_system_id, resolved_system_name, error) = match resolved {
                    Some(system) => (Some(system.id), Some(system.name), None),
                    None if graph.is_some() => (
                        None,
                        None,
                        Some(format!("{} was not found in the route map", row.system)),
                    ),
                    None => (None, None, Some("Route map is still loading".to_string())),
                };
                let valid = resolved_system_id.is_some() && error.is_none();

                FobImportSystem {
                    import_index: row.import_index,
                    input_system: row.system,
                    resolved_system_id,
                    resolved_system_name,
                    region: row.region,
                    last_seen_utc: row.last_seen_utc,
                    claimed_by: row.claimed_by,
                    selected: valid && previous_selected.unwrap_or(false),
                    error,
                }
            })
            .collect();
    }

    pub fn set_fob_import_text(&mut self, text: String) {
        if self.fob_import_text == text {
            return;
        }
        self.fob_import_text = text;
        self.refresh_fob_import_preview();
    }

    pub fn set_fob_import_selected(&mut self, import_index: usize, selected: bool) {
        if let Some(row) = self
            .fob_import_rows
            .iter_mut()
            .find(|row| row.import_index == import_index)
            && row.valid()
        {
            row.selected = selected;
        }
    }

    pub fn toggle_fob_import_sort(&mut self, column: FobImportSortColumn) {
        if self.fob_import_sort_column == column {
            self.fob_import_sort_ascending = !self.fob_import_sort_ascending;
        } else {
            self.fob_import_sort_column = column;
            self.fob_import_sort_ascending = column.default_ascending();
        }
    }

    pub fn sorted_fob_import_rows(&self) -> Vec<FobImportSystem> {
        let mut rows = self.fob_import_rows.clone();
        let column = self.fob_import_sort_column;
        let ascending = self.fob_import_sort_ascending;
        rows.sort_by(|left, right| {
            let ordering = compare_fob_import_rows(left, right, column);
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
            .then_with(|| left.import_index.cmp(&right.import_index))
        });
        rows
    }

    pub fn valid_fob_import_count(&self) -> usize {
        self.fob_import_rows
            .iter()
            .filter(|row| row.valid())
            .count()
    }

    pub fn selected_fob_import_count(&self) -> usize {
        self.fob_import_rows
            .iter()
            .filter(|row| row.valid() && row.selected)
            .count()
    }

    pub fn can_set_imported_fob_route(&self) -> bool {
        self.sde_route_graph_ready()
            && !self.login_in_progress()
            && !self.waypoint_send_in_progress()
            && !self.characters.is_empty()
            && self.selected_character_count() > 0
            && self.selected_fob_import_count() > 0
            && self.selected_fob_import_count() <= MAX_OPTIMIZED_ROUTE_STOPS
    }

    pub fn set_imported_fob_route(&mut self) {
        if self.waypoint_send_in_progress() {
            self.status_message = "Waypoint send already in progress".to_string();
            return;
        }

        let Some(graph) = self.sde_route_graph.clone() else {
            self.status_message = "Route map is still loading".to_string();
            return;
        };

        if self.characters.is_empty() {
            self.status_message = "Add at least one character first".to_string();
            return;
        }

        if self.selected_character_count() == 0 {
            self.status_message = "Select at least one character".to_string();
            return;
        }

        let missing_scope: Vec<String> = self
            .characters
            .iter()
            .filter(|character| character.selected)
            .filter(|character| {
                !character
                    .scopes
                    .iter()
                    .any(|scope| scope == sso::SCOPE_READ_LOCATION)
            })
            .map(|character| character.character_name.clone())
            .collect();
        if !missing_scope.is_empty() {
            self.status_message = format!(
                "Re-authenticate {} with {} to use optimized imports",
                missing_scope.join(", "),
                sso::SCOPE_READ_LOCATION
            );
            return;
        }

        let destinations = self.selected_fob_route_destinations();
        if destinations.is_empty() {
            self.status_message = "Select at least one imported system".to_string();
            return;
        }
        if destinations.len() > MAX_OPTIMIZED_ROUTE_STOPS {
            self.status_message = format!(
                "Select {MAX_OPTIMIZED_ROUTE_STOPS} or fewer systems for exact optimized routing"
            );
            return;
        }

        let request = WaypointSendRequest::optimized_route(destinations.clone(), graph);
        let target_ids: Vec<u64> = self
            .characters
            .iter()
            .filter(|character| character.selected)
            .map(|character| character.character_id)
            .collect();
        let skipped = self.characters.len().saturating_sub(target_ids.len());

        info!(
            selected_character_count = target_ids.len(),
            stop_count = destinations.len(),
            "Setting optimized imported FOB route"
        );
        self.start_waypoint_send_batch(request, target_ids, skipped, true);
        self.fob_import_open = false;
    }

    fn selected_fob_route_destinations(&self) -> Vec<RouteDestination> {
        self.fob_import_rows
            .iter()
            .filter(|row| row.valid() && row.selected)
            .filter_map(|row| {
                let Some(system_id) = row.resolved_system_id else {
                    warn!(system = %row.input_system, "Skipping unresolved imported system");
                    return None;
                };

                Some(RouteDestination {
                    system_id,
                    system_name: row.display_system().to_string(),
                    import_index: row.import_index,
                })
            })
            .collect()
    }
}

impl FobImportSortColumn {
    fn default_ascending(self) -> bool {
        match self {
            Self::Use => false,
            Self::System | Self::Region | Self::ClaimedBy => true,
            Self::LastSeenUtc => false,
        }
    }
}

fn compare_fob_import_rows(
    left: &FobImportSystem,
    right: &FobImportSystem,
    column: FobImportSortColumn,
) -> Ordering {
    match column {
        FobImportSortColumn::Use => left
            .selected
            .cmp(&right.selected)
            .then_with(|| left.valid().cmp(&right.valid())),
        FobImportSortColumn::System => compare_text(left.display_system(), right.display_system()),
        FobImportSortColumn::Region => compare_text(&left.region, &right.region),
        FobImportSortColumn::LastSeenUtc => compare_text(&left.last_seen_utc, &right.last_seen_utc),
        FobImportSortColumn::ClaimedBy => compare_text(&left.claimed_by, &right.claimed_by),
    }
}

fn compare_text(left: &str, right: &str) -> Ordering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}
