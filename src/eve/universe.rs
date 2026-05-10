use anyhow::{Context, Result, bail};
use reqwest::{StatusCode, blocking::Client};
use serde::Deserialize;
use tracing::{debug, info, warn};

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
    let response = lookup_universe_ids(&client, &[name])?;
    let destination = match pick_destination_match(&response) {
        Some(destination) => destination,
        None => resolve_station_shorthand(&client, name)
            .with_context(|| format!("No solar system or station named `{name}` was found"))?,
    };

    info!(
        destination = name,
        destination_id = destination.id,
        destination_name = %destination.name,
        destination_category = destination.category.label(),
        "Resolved destination name"
    );

    Ok(destination)
}

fn lookup_universe_ids(client: &Client, names: &[&str]) -> Result<UniverseIdsResponse> {
    let response = client
        .post(format!("{}/universe/ids/", esi::ESI_BASE_URL))
        .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)])
        .json(names)
        .send()
        .context("Failed to resolve destination name through ESI")?;

    let status = response.status();
    if status != StatusCode::OK {
        bail!(
            "ESI destination lookup failed ({status}): {}",
            esi::error_body(response)
        );
    }

    response
        .json()
        .context("Failed to parse ESI destination lookup response")
}

fn resolve_station_shorthand(client: &Client, name: &str) -> Result<UniverseDestination> {
    let system_name = station_shorthand_system_name(name)
        .with_context(|| format!("Could not infer solar system from station shorthand `{name}`"))?;
    let system = resolve_system_by_name(client, &system_name)?;
    let system = fetch_system(client, system.id)?;

    if system.stations.is_empty() {
        bail!(
            "Solar system `{}` does not have NPC stations listed in ESI",
            system.name
        );
    }

    let mut candidates = Vec::new();
    for station_id in system.stations {
        match fetch_station(client, station_id) {
            Ok(station) => {
                if let Some(score) = station_shorthand_score(name, &station.name) {
                    candidates.push(ScoredStation {
                        id: station.station_id,
                        name: station.name,
                        score,
                    });
                }
            }
            Err(error) => {
                warn!(
                    station_id,
                    error = ?error,
                    "Failed to fetch station while resolving destination shorthand"
                );
            }
        }
    }

    let station = pick_station_candidate(candidates)
        .with_context(|| format!("No station in `{}` matched `{name}`", system.name))?;

    info!(
        destination = name,
        system_name = %system.name,
        station_id = station.id,
        station_name = %station.name,
        "Resolved station shorthand"
    );

    Ok(UniverseDestination {
        id: station.id,
        name: station.name,
        category: UniverseDestinationCategory::Station,
    })
}

fn resolve_system_by_name(client: &Client, name: &str) -> Result<NamedId> {
    let response = lookup_universe_ids(client, &[name])?;
    response
        .systems
        .into_iter()
        .next()
        .with_context(|| format!("No solar system named `{name}` was found"))
}

fn fetch_system(client: &Client, system_id: i64) -> Result<SolarSystemResponse> {
    let response = client
        .get(format!(
            "{}/universe/systems/{system_id}/",
            esi::ESI_BASE_URL
        ))
        .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)])
        .send()
        .with_context(|| format!("Failed to fetch solar system {system_id} through ESI"))?;

    let status = response.status();
    if status != StatusCode::OK {
        bail!(
            "ESI solar system lookup failed ({status}): {}",
            esi::error_body(response)
        );
    }

    response
        .json()
        .context("Failed to parse ESI solar system response")
}

fn fetch_station(client: &Client, station_id: i64) -> Result<StationResponse> {
    let response = client
        .get(format!(
            "{}/universe/stations/{station_id}/",
            esi::ESI_BASE_URL
        ))
        .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)])
        .send()
        .with_context(|| format!("Failed to fetch station {station_id} through ESI"))?;

    let status = response.status();
    if status != StatusCode::OK {
        bail!(
            "ESI station lookup failed ({status}): {}",
            esi::error_body(response)
        );
    }

    response
        .json()
        .context("Failed to parse ESI station response")
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

fn station_shorthand_system_name(name: &str) -> Option<String> {
    let first_segment = name.split(" - ").next()?.trim();
    let (system_name, planet_token) = first_segment.rsplit_once(char::is_whitespace)?;

    if is_roman_numeral(planet_token.trim()) {
        let system_name = system_name.trim();
        (!system_name.is_empty()).then(|| system_name.to_string())
    } else {
        None
    }
}

fn is_roman_numeral(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|character| {
            matches!(
                character.to_ascii_uppercase(),
                'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'
            )
        })
}

fn pick_station_candidate(mut candidates: Vec<ScoredStation>) -> Option<ScoredStation> {
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.len().cmp(&right.name.len()))
            .then_with(|| left.name.cmp(&right.name))
    });
    candidates.into_iter().next()
}

fn station_shorthand_score(input: &str, station_name: &str) -> Option<usize> {
    let input_tokens = normalized_tokens(input);
    let station_tokens = normalized_tokens(station_name);

    if input_tokens.is_empty() || station_tokens.is_empty() {
        return None;
    }

    if !input_tokens.iter().all(|token| {
        station_tokens
            .iter()
            .any(|station_token| station_token == token)
    }) {
        return None;
    }

    let mut score = input_tokens.len() * 10;
    let normalized_input = input_tokens.join(" ");
    let normalized_station = station_tokens.join(" ");

    if normalized_station.contains(&normalized_input) {
        score += 50;
    }

    if tokens_appear_in_order(&input_tokens, &station_tokens) {
        score += 25;
    }

    Some(score.saturating_sub(station_tokens.len().saturating_sub(input_tokens.len())))
}

fn normalized_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn tokens_appear_in_order(needles: &[String], haystack: &[String]) -> bool {
    let mut next_needle = 0;
    for token in haystack {
        if token == &needles[next_needle] {
            next_needle += 1;
            if next_needle == needles.len() {
                return true;
            }
        }
    }

    false
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

#[derive(Debug, Deserialize)]
struct SolarSystemResponse {
    name: String,
    #[serde(default)]
    stations: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct StationResponse {
    name: String,
    station_id: i64,
}

#[derive(Debug)]
struct ScoredStation {
    id: i64,
    name: String,
    score: usize,
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

    #[test]
    fn extracts_station_shorthand_system_name() {
        assert_eq!(
            station_shorthand_system_name("Rakapas V - Home Guard"),
            Some("Rakapas".to_string())
        );
        assert_eq!(
            station_shorthand_system_name("New Caldari IV - Moon 1 - Caldari Navy"),
            Some("New Caldari".to_string())
        );
        assert_eq!(station_shorthand_system_name("Rakapas"), None);
    }

    #[test]
    fn scores_station_shorthand_with_missing_service_suffix() {
        assert!(
            station_shorthand_score(
                "Rakapas V - Home Guard",
                "Rakapas V - Home Guard Assembly Plant"
            )
            .is_some()
        );
    }

    #[test]
    fn scores_station_shorthand_with_missing_moon_segment() {
        assert!(
            station_shorthand_score(
                "Jita IV - Caldari Navy",
                "Jita IV - Moon 4 - Caldari Navy Assembly Plant"
            )
            .is_some()
        );
    }

    #[test]
    fn rejects_station_shorthand_when_tokens_are_missing() {
        assert_eq!(
            station_shorthand_score(
                "Rakapas V - Home Guard",
                "Rakapas II - State Protectorate Logistic Support"
            ),
            None
        );
    }
}
