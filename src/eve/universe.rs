use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::{debug, info};

use crate::eve::esi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniverseDestinationCategory {
    SolarSystem,
    Station,
}

impl UniverseDestinationCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::SolarSystem => "solar system",
            Self::Station => "station",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniverseDestination {
    pub id: i64,
    pub name: String,
    pub category: UniverseDestinationCategory,
}

pub fn resolve_destination_name(name: &str) -> Result<UniverseDestination> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Destination required");
    }

    debug!(destination = name, "Resolving destination name through ESI");
    let client = esi::client()?;
    let response = client
        .post(format!("{}/universe/ids/", esi::ESI_BASE_URL))
        .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)])
        .json(&[name])
        .send()
        .context("Failed to resolve destination name through ESI")?;

    let status = response.status();
    if status != StatusCode::OK {
        bail!(
            "ESI destination lookup failed ({status}): {}",
            esi::error_body(response)
        );
    }

    let response: UniverseIdsResponse = response
        .json()
        .context("Failed to parse ESI destination lookup response")?;
    let destination = pick_destination_match(&response)
        .with_context(|| format!("No solar system or station named `{name}` was found"))?;

    info!(
        destination = name,
        destination_id = destination.id,
        destination_name = %destination.name,
        destination_category = destination.category.label(),
        "Resolved destination name"
    );

    Ok(destination)
}

fn pick_destination_match(response: &UniverseIdsResponse) -> Option<UniverseDestination> {
    response
        .systems
        .first()
        .map(|system| UniverseDestination {
            id: system.id,
            name: system.name.clone(),
            category: UniverseDestinationCategory::SolarSystem,
        })
        .or_else(|| {
            response
                .stations
                .first()
                .map(|station| UniverseDestination {
                    id: station.id,
                    name: station.name.clone(),
                    category: UniverseDestinationCategory::Station,
                })
        })
}

#[derive(Debug, Default, Deserialize)]
struct UniverseIdsResponse {
    #[serde(default, alias = "solar_systems")]
    systems: Vec<NamedId>,
    #[serde(default)]
    stations: Vec<NamedId>,
}

#[derive(Debug, Deserialize)]
struct NamedId {
    id: i64,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_system_matches_over_station_matches() {
        let response = UniverseIdsResponse {
            systems: vec![NamedId {
                id: 30045339,
                name: "Rakapas".to_string(),
            }],
            stations: vec![NamedId {
                id: 60000001,
                name: "Rakapas".to_string(),
            }],
        };

        let destination = pick_destination_match(&response).expect("destination should resolve");

        assert_eq!(destination.id, 30045339);
        assert_eq!(
            destination.category,
            UniverseDestinationCategory::SolarSystem
        );
    }

    #[test]
    fn falls_back_to_station_matches() {
        let response = UniverseIdsResponse {
            systems: Vec::new(),
            stations: vec![NamedId {
                id: 60003760,
                name: "Jita IV - Moon 4 - Caldari Navy Assembly Plant".to_string(),
            }],
        };

        let destination = pick_destination_match(&response).expect("destination should resolve");

        assert_eq!(destination.id, 60003760);
        assert_eq!(destination.category, UniverseDestinationCategory::Station);
    }
}
