use anyhow::Result;
use reqwest::StatusCode;
use tracing::{debug, info};

use crate::eve::esi;

#[derive(Clone, Copy, Debug)]
pub struct WaypointOptions {
    pub add_to_beginning: bool,
    pub clear_other_waypoints: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaypointRouteMode {
    #[default]
    ReplaceRoute,
    AddNextStop,
    AddFinalStop,
}

impl WaypointRouteMode {
    pub const ALL: [Self; 3] = [Self::ReplaceRoute, Self::AddNextStop, Self::AddFinalStop];

    pub fn label(self) -> &'static str {
        match self {
            Self::ReplaceRoute => "Replace route",
            Self::AddNextStop => "Add next stop",
            Self::AddFinalStop => "Add final stop",
        }
    }

    pub fn options(self) -> WaypointOptions {
        match self {
            Self::ReplaceRoute => WaypointOptions {
                add_to_beginning: false,
                clear_other_waypoints: true,
            },
            Self::AddNextStop => WaypointOptions {
                add_to_beginning: true,
                clear_other_waypoints: false,
            },
            Self::AddFinalStop => WaypointOptions {
                add_to_beginning: false,
                clear_other_waypoints: false,
            },
        }
    }
}

pub fn options_for_route_stop(first_stop: bool) -> WaypointOptions {
    WaypointOptions {
        add_to_beginning: false,
        clear_other_waypoints: first_stop,
    }
}

pub fn set_waypoint(
    access_token: &str,
    destination_id: i64,
    options: WaypointOptions,
) -> Result<()> {
    let client = esi::client()?;

    debug!(
        destination_id,
        add_to_beginning = options.add_to_beginning,
        clear_other_waypoints = options.clear_other_waypoints,
        "Sending ESI waypoint request"
    );

    let response = esi::send_request(
        client
            .post(format!("{}/ui/autopilot/waypoint/", esi::ESI_BASE_URL))
            .bearer_auth(access_token)
            .query(&[
                ("datasource", esi::DATASOURCE.to_string()),
                ("destination_id", destination_id.to_string()),
                ("add_to_beginning", options.add_to_beginning.to_string()),
                (
                    "clear_other_waypoints",
                    options.clear_other_waypoints.to_string(),
                ),
            ]),
        "waypoint update",
    )?;
    esi::require_status(response, StatusCode::NO_CONTENT, "waypoint update")?;

    info!(destination_id, "ESI waypoint request accepted");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_route_stop_replaces_existing_route() {
        let options = options_for_route_stop(true);

        assert!(!options.add_to_beginning);
        assert!(options.clear_other_waypoints);
    }

    #[test]
    fn later_route_stops_append_to_route() {
        let options = options_for_route_stop(false);

        assert!(!options.add_to_beginning);
        assert!(!options.clear_other_waypoints);
    }
}
