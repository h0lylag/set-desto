use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const CONFIG_FILE: &str = "config.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub esi: EsiConfig,
    #[serde(default)]
    pub characters: Vec<CharacterConfig>,
    #[serde(default)]
    pub favorites: Vec<FavoriteDestinationConfig>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        debug!(path = %path.display(), "Loading app config");

        if !path.exists() {
            info!(path = %path.display(), "App config does not exist yet");
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        debug!(
            path = %path.display(),
            character_count = self.characters.len(),
            "Saving app config"
        );
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("Config path did not include a parent directory"))?;

        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory {}", parent.display()))?;

        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize app config")?;
        fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        info!(
            path = %path.display(),
            character_count = self.characters.len(),
            "Saved app config"
        );
        Ok(())
    }

    pub fn upsert_character(&mut self, character: CharacterConfig) {
        if let Some(existing) = self
            .characters
            .iter_mut()
            .find(|existing| existing.character_id == character.character_id)
        {
            *existing = character;
        } else {
            self.characters.push(character);
        }
    }

    pub fn remove_character(&mut self, character_id: u64) -> Option<CharacterConfig> {
        let index = self
            .characters
            .iter()
            .position(|character| character.character_id == character_id)?;

        Some(self.characters.remove(index))
    }

    pub fn upsert_favorite(&mut self, favorite: FavoriteDestinationConfig) {
        if let Some(existing) = self
            .favorites
            .iter_mut()
            .find(|existing| existing.destination_id == favorite.destination_id)
        {
            *existing = favorite;
        } else {
            self.favorites.push(favorite);
        }
    }

    pub fn remove_favorite(&mut self, destination_id: i64) -> Option<FavoriteDestinationConfig> {
        let index = self
            .favorites
            .iter()
            .position(|favorite| favorite.destination_id == destination_id)?;

        Some(self.favorites.remove(index))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EsiConfig {
    #[serde(default)]
    pub client_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CharacterConfig {
    pub character_id: u64,
    pub character_name: String,
    #[serde(default = "default_added_at_unix_seconds")]
    pub added_at_unix_seconds: u64,
    pub scopes: Vec<String>,
    #[serde(default = "default_character_selected")]
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FavoriteDestinationConfig {
    pub destination_id: i64,
    pub destination_name: String,
    pub destination_kind: String,
}

pub fn config_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "h0lylag", "set-desto")
        .ok_or_else(|| anyhow!("Could not determine platform config directory"))?;

    Ok(project_dirs.config_dir().join(CONFIG_FILE))
}

fn default_character_selected() -> bool {
    true
}

fn default_added_at_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_character_replaces_existing_character() {
        let mut config = AppConfig::default();
        config.upsert_character(CharacterConfig {
            character_id: 42,
            character_name: "Old Name".to_string(),
            added_at_unix_seconds: 1_778_000_000,
            scopes: vec!["old.scope".to_string()],
            selected: true,
        });

        config.upsert_character(CharacterConfig {
            character_id: 42,
            character_name: "New Name".to_string(),
            added_at_unix_seconds: 1_778_000_001,
            scopes: vec!["new.scope".to_string()],
            selected: false,
        });

        assert_eq!(config.characters.len(), 1);
        assert_eq!(config.characters[0].character_name, "New Name");
        assert_eq!(config.characters[0].added_at_unix_seconds, 1_778_000_001);
        assert_eq!(config.characters[0].scopes, vec!["new.scope"]);
        assert!(!config.characters[0].selected);
    }

    #[test]
    fn missing_selected_defaults_to_true() {
        let character: CharacterConfig = serde_json::from_str(
            r#"{
                "character_id": 42,
                "character_name": "Test Pilot",
                "added_at_unix_seconds": 1778000000,
                "scopes": []
            }"#,
        )
        .expect("character config should deserialize");

        assert!(character.selected);
    }

    #[test]
    fn missing_added_at_defaults_to_current_time() {
        let before = default_added_at_unix_seconds();
        let character: CharacterConfig = serde_json::from_str(
            r#"{
                "character_id": 42,
                "character_name": "Test Pilot",
                "scopes": []
            }"#,
        )
        .expect("character config should deserialize");
        let after = default_added_at_unix_seconds();

        assert!(character.added_at_unix_seconds >= before);
        assert!(character.added_at_unix_seconds <= after);
    }

    #[test]
    fn missing_esi_config_defaults_to_empty_client_id() {
        let config: AppConfig = serde_json::from_str(
            r#"{
                "characters": []
            }"#,
        )
        .expect("app config should deserialize");

        assert!(config.esi.client_id.is_empty());
    }

    #[test]
    fn missing_favorites_defaults_to_empty_list() {
        let config: AppConfig = serde_json::from_str(
            r#"{
                "characters": []
            }"#,
        )
        .expect("app config should deserialize");

        assert!(config.favorites.is_empty());
    }

    #[test]
    fn upsert_favorite_replaces_existing_destination() {
        let mut config = AppConfig::default();
        config.upsert_favorite(FavoriteDestinationConfig {
            destination_id: 30000142,
            destination_name: "Old Jita".to_string(),
            destination_kind: "solar system".to_string(),
        });

        config.upsert_favorite(FavoriteDestinationConfig {
            destination_id: 30000142,
            destination_name: "Jita".to_string(),
            destination_kind: "solar system".to_string(),
        });

        assert_eq!(config.favorites.len(), 1);
        assert_eq!(config.favorites[0].destination_name, "Jita");
    }

    #[test]
    fn remove_favorite_returns_removed_destination() {
        let mut config = AppConfig::default();
        config.upsert_favorite(FavoriteDestinationConfig {
            destination_id: 30000142,
            destination_name: "Jita".to_string(),
            destination_kind: "solar system".to_string(),
        });

        let removed = config
            .remove_favorite(30000142)
            .expect("favorite should be removed");

        assert_eq!(removed.destination_name, "Jita");
        assert!(config.favorites.is_empty());
        assert!(config.remove_favorite(30000142).is_none());
    }

    #[test]
    fn remove_character_returns_removed_character() {
        let mut config = AppConfig::default();
        config.upsert_character(CharacterConfig {
            character_id: 42,
            character_name: "Test Pilot".to_string(),
            added_at_unix_seconds: 1_778_000_000,
            scopes: Vec::new(),
            selected: true,
        });

        let removed = config
            .remove_character(42)
            .expect("character should be removed");

        assert_eq!(removed.character_name, "Test Pilot");
        assert!(config.characters.is_empty());
        assert!(config.remove_character(42).is_none());
    }
}
