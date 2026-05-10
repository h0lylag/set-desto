use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use tracing::{debug, info};

const ESI_WAYPOINT_URL: &str = "https://esi.evetech.net/latest/ui/autopilot/waypoint/";

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
    let client = Client::builder()
        .user_agent(format!("set-desto/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build ESI HTTP client")?;

    debug!(
        destination_id,
        add_to_beginning = options.add_to_beginning,
        clear_other_waypoints = options.clear_other_waypoints,
        "Sending ESI waypoint request"
    );

    let response = client
        .post(ESI_WAYPOINT_URL)
        .bearer_auth(access_token)
        .query(&[
            ("datasource", "tranquility".to_string()),
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

    let body = response
        .text()
        .unwrap_or_else(|_| "failed to read ESI error body".to_string());
    bail!("ESI waypoint request failed ({status}): {body}")
}
