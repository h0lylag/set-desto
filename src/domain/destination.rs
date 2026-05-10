use anyhow::{Context, Result, bail};
use tracing::debug;

use crate::eve::universe::{self, UniverseDestinationCategory};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestinationKind {
    NumericId,
    SolarSystem,
    Station,
}

impl DestinationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::NumericId => "numeric ID",
            Self::SolarSystem => "solar system",
            Self::Station => "station",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDestination {
    pub id: i64,
    pub name: String,
    pub kind: DestinationKind,
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
        },
    })
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
}
