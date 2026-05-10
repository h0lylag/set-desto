use anyhow::{Context, Result, bail};
use tracing::debug;

use crate::eve::universe::{self, UniverseDestinationCategory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestinationKind {
    NumericId,
    SolarSystem,
    Station,
    Structure,
}

impl DestinationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::NumericId => "numeric ID",
            Self::SolarSystem => "solar system",
            Self::Station => "station",
            Self::Structure => "player structure",
        }
    }
}

pub const PLAYER_STRUCTURE_ID_MIN: i64 = 1_000_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDestination {
    pub id: i64,
    pub name: String,
    pub kind: DestinationKind,
}

#[derive(Clone, Debug)]
pub struct StructureResolutionContext {
    pub character_id: u64,
    pub access_token: String,
}

pub fn resolve(input: &str) -> Result<ResolvedDestination> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Destination required");
    }

    if input.starts_with('-') || input.chars().all(|character| character.is_ascii_digit()) {
        return resolve_numeric_id(input);
    }

    let destination = universe::resolve_destination_name(input)?;
    Ok(ResolvedDestination {
        id: destination.id,
        name: destination.name,
        kind: match destination.category {
            UniverseDestinationCategory::SolarSystem => DestinationKind::SolarSystem,
            UniverseDestinationCategory::Station => DestinationKind::Station,
            UniverseDestinationCategory::Structure => DestinationKind::Structure,
        },
    })
}

pub fn resolve_structure(
    input: &str,
    context: &StructureResolutionContext,
) -> Result<ResolvedDestination> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Destination required");
    }

    let destination = if let Some(structure_id) = player_structure_id(input)? {
        universe::resolve_structure_id(&context.access_token, structure_id)?
    } else {
        universe::resolve_structure_name(context.character_id, &context.access_token, input)?
    };

    Ok(ResolvedDestination {
        id: destination.id,
        name: destination.name,
        kind: match destination.category {
            UniverseDestinationCategory::SolarSystem => DestinationKind::SolarSystem,
            UniverseDestinationCategory::Station => DestinationKind::Station,
            UniverseDestinationCategory::Structure => DestinationKind::Structure,
        },
    })
}

pub fn looks_like_player_structure_id(input: &str) -> bool {
    matches!(player_structure_id(input), Ok(Some(_)))
}

fn resolve_numeric_id(input: &str) -> Result<ResolvedDestination> {
    let id = input
        .parse::<i64>()
        .with_context(|| format!("Destination ID `{input}` was not a valid ESI ID"))?;

    if id <= 0 {
        bail!("Destination ID must be a positive ESI ID");
    }

    debug!(destination_id = id, "Using numeric destination ID");
    Ok(ResolvedDestination {
        id,
        name: input.to_string(),
        kind: DestinationKind::NumericId,
    })
}

fn player_structure_id(input: &str) -> Result<Option<i64>> {
    let input = input.trim();
    if input.starts_with('-') || !input.chars().all(|character| character.is_ascii_digit()) {
        return Ok(None);
    }

    let id = input
        .parse::<i64>()
        .with_context(|| format!("Destination ID `{input}` was not a valid ESI ID"))?;

    Ok((id >= PLAYER_STRUCTURE_ID_MIN).then_some(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_numeric_destination_id() {
        let destination = resolve("30000142").expect("destination should resolve");

        assert_eq!(destination.id, 30000142);
        assert_eq!(destination.kind, DestinationKind::NumericId);
    }

    #[test]
    fn rejects_non_positive_destination_id() {
        let err = resolve("0").unwrap_err().to_string();
        assert!(err.contains("positive ESI ID"));
    }

    #[test]
    fn rejects_negative_destination_id() {
        let err = resolve("-1").unwrap_err().to_string();
        assert!(err.contains("positive ESI ID"));
    }

    #[test]
    fn rejects_empty_destination() {
        let err = resolve("  ").unwrap_err().to_string();
        assert!(err.contains("Destination required"));
    }

    #[test]
    fn detects_player_structure_ids() {
        assert!(looks_like_player_structure_id("1046802738801"));
        assert!(!looks_like_player_structure_id("30000142"));
        assert!(!looks_like_player_structure_id("-1046802738801"));
        assert!(!looks_like_player_structure_id("Jita"));
    }
}
