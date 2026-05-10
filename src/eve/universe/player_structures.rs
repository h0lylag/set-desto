use anyhow::{Context, Result, bail};
use reqwest::{StatusCode, blocking::Client};
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::eve::esi;

use super::{
    UniverseDestination, UniverseDestinationCategory, normalized_tokens, tokens_appear_in_order,
};

// Player-owned Upwell structures are not public universe entities. ESI only
// resolves them with a scoped token for a character that has access.
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

fn search_character_structures(
    client: &Client,
    character_id: u64,
    access_token: &str,
    name: &str,
    strict: bool,
) -> Result<Vec<i64>> {
    let strict = if strict { "true" } else { "false" };
    let response = esi::send_request(
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
    let response = esi::send_request(
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

#[derive(Debug, Default, Deserialize)]
struct CharacterSearchResponse {
    #[serde(default, alias = "structures")]
    structure: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct StructureResponse {
    name: String,
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
