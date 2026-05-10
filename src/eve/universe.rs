use anyhow::{Context, Result, bail};
use reqwest::{StatusCode, blocking::Client};
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::eve::esi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniverseDestinationCategory {
    SolarSystem,
    Station,
    Structure,
}

impl UniverseDestinationCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::SolarSystem => "solar system",
            Self::Station => "station",
            Self::Structure => "player structure",
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

pub fn resolve_structure_id(access_token: &str, structure_id: i64) -> Result<UniverseDestination> {
    if structure_id <= 0 {
        bail!("Player structure ID must be positive");
    }

    debug!(structure_id, "Resolving player structure ID through ESI");
    let client = esi::client()?;
    let structure = fetch_structure(&client, access_token, structure_id)?;

    info!(
        structure_id,
        structure_name = %structure.name,
        "Resolved player structure ID"
    );

    Ok(UniverseDestination {
        id: structure_id,
        name: structure.name,
        category: UniverseDestinationCategory::Structure,
    })
}

pub fn resolve_structure_name(
    character_id: u64,
    access_token: &str,
    name: &str,
) -> Result<UniverseDestination> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Destination required");
    }

    debug!(
        character_id,
        structure_name = name,
        "Resolving player structure name through ESI"
    );
    let client = esi::client()?;
    let mut structure_ids =
        search_character_structures(&client, character_id, access_token, name, true)?;

    if structure_ids.is_empty() {
        debug!(
            character_id,
            structure_name = name,
            "Strict player structure search returned no results; trying partial search"
        );
        structure_ids =
            search_character_structures(&client, character_id, access_token, name, false)?;
    }

    if structure_ids.is_empty() {
        bail!("No accessible player structure named `{name}` was found");
    }

    let mut candidates = Vec::new();
    for structure_id in structure_ids {
        match fetch_structure(&client, access_token, structure_id) {
            Ok(structure) => candidates.push(StructureCandidate {
                id: structure_id,
                name: structure.name,
            }),
            Err(error) => {
                warn!(
                    structure_id,
                    error = ?error,
                    "Failed to fetch player structure returned by search"
                );
            }
        }
    }

    let structure = pick_structure_candidate(name, candidates)
        .with_context(|| format!("No accessible player structure named `{name}` was found"))?;

    info!(
        character_id,
        structure_id = structure.id,
        structure_name = %structure.name,
        "Resolved player structure name"
    );

    Ok(UniverseDestination {
        id: structure.id,
        name: structure.name,
        category: UniverseDestinationCategory::Structure,
    })
}

fn lookup_universe_ids(client: &Client, names: &[&str]) -> Result<UniverseIdsResponse> {
    let response = esi::send_with_rate_limit(
        client
            .post(format!("{}/universe/ids/", esi::ESI_BASE_URL))
            .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)])
            .json(names),
        "universe destination name lookup",
    )?;

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

fn search_character_structures(
    client: &Client,
    character_id: u64,
    access_token: &str,
    name: &str,
    strict: bool,
) -> Result<Vec<i64>> {
    let strict = if strict { "true" } else { "false" };
    let response = esi::send_with_rate_limit(
        client
            .get(format!(
                "{}/characters/{character_id}/search/",
                esi::ESI_BASE_URL
            ))
            .bearer_auth(access_token)
            .query(&[
                ("datasource", esi::DATASOURCE),
                ("categories", "structure"),
                ("search", name),
                ("strict", strict),
            ]),
        "player structure search",
    )?;

    let status = response.status();
    match status {
        StatusCode::OK => {}
        StatusCode::FORBIDDEN => bail!(
            "ESI player structure search was forbidden; re-authenticate the selected character with structure scopes: {}",
            esi::error_body(response)
        ),
        _ => bail!(
            "ESI player structure search failed ({status}): {}",
            esi::error_body(response)
        ),
    }

    let mut response: CharacterSearchResponse = response
        .json()
        .context("Failed to parse ESI player structure search response")?;
    response.structure.sort_unstable();
    response.structure.dedup();

    Ok(response.structure)
}

fn fetch_structure(
    client: &Client,
    access_token: &str,
    structure_id: i64,
) -> Result<StructureResponse> {
    let response = esi::send_with_rate_limit(
        client
            .get(format!(
                "{}/universe/structures/{structure_id}/",
                esi::ESI_BASE_URL
            ))
            .bearer_auth(access_token)
            .query(&[("datasource", esi::DATASOURCE)]),
        "player structure lookup",
    )?;

    let status = response.status();
    match status {
        StatusCode::OK => {}
        StatusCode::FORBIDDEN => bail!(
            "Selected character cannot access player structure {structure_id}; confirm ACL/docking access and structure scopes: {}",
            esi::error_body(response)
        ),
        StatusCode::NOT_FOUND => bail!(
            "Player structure {structure_id} was not found or is not accessible to the selected character"
        ),
        _ => bail!(
            "ESI player structure lookup failed ({status}): {}",
            esi::error_body(response)
        ),
    }

    response
        .json()
        .context("Failed to parse ESI player structure response")
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
    let response = esi::send_with_rate_limit(
        client
            .get(format!(
                "{}/universe/systems/{system_id}/",
                esi::ESI_BASE_URL
            ))
            .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)]),
        "solar system lookup",
    )?;

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
    let response = esi::send_with_rate_limit(
        client
            .get(format!(
                "{}/universe/stations/{station_id}/",
                esi::ESI_BASE_URL
            ))
            .query(&[("datasource", esi::DATASOURCE), ("language", esi::LANGUAGE)]),
        "station lookup",
    )?;

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

fn pick_structure_candidate(
    input: &str,
    mut candidates: Vec<StructureCandidate>,
) -> Option<StructureCandidate> {
    candidates.sort_by(|left, right| {
        structure_candidate_score(input, &right.name)
            .cmp(&structure_candidate_score(input, &left.name))
            .then_with(|| left.name.len().cmp(&right.name.len()))
            .then_with(|| left.name.cmp(&right.name))
    });
    candidates.into_iter().next()
}

fn structure_candidate_score(input: &str, structure_name: &str) -> usize {
    let input_tokens = normalized_tokens(input);
    let structure_tokens = normalized_tokens(structure_name);
    let normalized_input = input_tokens.join(" ");
    let normalized_structure = structure_tokens.join(" ");

    let mut score = 0;
    if structure_name.eq_ignore_ascii_case(input.trim()) {
        score += 1_000;
    }
    if !normalized_input.is_empty() && normalized_input == normalized_structure {
        score += 500;
    }
    if !normalized_input.is_empty() && normalized_structure.contains(&normalized_input) {
        score += 100;
    }
    if !input_tokens.is_empty() && tokens_appear_in_order(&input_tokens, &structure_tokens) {
        score += 50;
    }

    score
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

#[derive(Debug, Default, Deserialize)]
struct CharacterSearchResponse {
    #[serde(default, alias = "structures")]
    structure: Vec<i64>,
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

#[derive(Debug, Deserialize)]
struct StructureResponse {
    name: String,
}

#[derive(Debug)]
struct ScoredStation {
    id: i64,
    name: String,
    score: usize,
}

#[derive(Debug)]
struct StructureCandidate {
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

    #[test]
    fn picks_exact_structure_name_before_partial_matches() {
        let structure = pick_structure_candidate(
            "Home - Staging",
            vec![
                StructureCandidate {
                    id: 1046802738802,
                    name: "Home - Staging Backup".to_string(),
                },
                StructureCandidate {
                    id: 1046802738801,
                    name: "Home - Staging".to_string(),
                },
            ],
        )
        .expect("structure should resolve");

        assert_eq!(structure.id, 1046802738801);
    }
}
