use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use tracing::{debug, info};

use crate::eve::esi;

#[derive(Clone, Copy, Debug)]
pub struct WaypointOptions {
    pub add_to_beginning: bool,
    pub clear_other_waypoints: bool,
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

    let response = client
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
        ])
        .send()
        .context("Failed to send ESI waypoint request")?;

    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        info!(destination_id, "ESI waypoint request accepted");
        return Ok(());
    }

    bail!(
        "ESI waypoint request failed ({status}): {}",
        esi::error_body(response)
    )
}
