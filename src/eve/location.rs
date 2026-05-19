use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::debug;

use crate::eve::esi;

pub fn current_solar_system_id(access_token: &str, character_id: u64) -> Result<i64> {
    let client = esi::client()?;

    debug!(character_id, "Reading character location from ESI");
    let response = esi::send_request(
        client
            .get(format!(
                "{}/characters/{character_id}/location/",
                esi::ESI_BASE_URL
            ))
            .bearer_auth(access_token)
            .query(&[("datasource", esi::DATASOURCE)]),
        "character location lookup",
    )?;
    let response = esi::require_status(response, StatusCode::OK, "character location lookup")?;
    let location: CharacterLocationResponse = response
        .json()
        .context("Failed to parse ESI character location response")?;

    Ok(location.solar_system_id)
}

#[derive(Debug, Deserialize)]
struct CharacterLocationResponse {
    solar_system_id: i64,
}
