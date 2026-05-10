use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const CONFIG_FILE: &str = "config.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub characters: Vec<CharacterConfig>,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CharacterConfig {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    #[serde(default = "default_character_selected")]
    pub selected: bool,
}

pub fn config_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "h0lylag", "set-desto")
        .ok_or_else(|| anyhow!("Could not determine platform config directory"))?;

    Ok(project_dirs.config_dir().join(CONFIG_FILE))
}

fn default_character_selected() -> bool {
    true
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
            scopes: vec!["old.scope".to_string()],
            selected: true,
        });

        config.upsert_character(CharacterConfig {
            character_id: 42,
            character_name: "New Name".to_string(),
            scopes: vec!["new.scope".to_string()],
            selected: false,
        });

        assert_eq!(config.characters.len(), 1);
        assert_eq!(config.characters[0].character_name, "New Name");
        assert_eq!(config.characters[0].scopes, vec!["new.scope"]);
        assert!(!config.characters[0].selected);
    }

    #[test]
    fn missing_selected_defaults_to_true() {
        let character: CharacterConfig = serde_json::from_str(
            r#"{
                "character_id": 42,
                "character_name": "Test Pilot",
                "scopes": []
            }"#,
        )
        .expect("character config should deserialize");

        assert!(character.selected);
    }

    #[test]
    fn remove_character_returns_removed_character() {
        let mut config = AppConfig::default();
        config.upsert_character(CharacterConfig {
            character_id: 42,
            character_name: "Test Pilot".to_string(),
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
