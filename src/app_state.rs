use std::sync::mpsc::Receiver;

use eframe::egui;
use tracing::debug;

use crate::eve::auth::{self, AuthenticatedCharacter, LoginResult, SsoConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestoMode {
    Manual,
    Clipboard,
}

impl DestoMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::Clipboard => "Clipboard",
        }
    }
}

pub struct SetDestoApp {
    pub debug_mode: bool,
    pub characters: Vec<AuthenticatedCharacter>,
    pub destination: String,
    pub notes: String,
    pub pin_destination: bool,
    pub mode: DestoMode,
    pub status_message: String,
    login_receiver: Option<Receiver<LoginResult>>,
}

impl SetDestoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, debug_mode: bool) -> Self {
        debug!("Initializing Set Desto (debug_mode={})", debug_mode);

        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            debug_mode,
            characters: Vec::new(),
            destination: String::new(),
            notes: String::new(),
            pin_destination: false,
            mode: DestoMode::Manual,
            status_message: "Ready".to_string(),
            login_receiver: None,
        }
    }

    pub fn login_in_progress(&self) -> bool {
        self.login_receiver.is_some()
    }

    pub fn start_character_login(&mut self) {
        if self.login_in_progress() {
            return;
        }

        let config = match SsoConfig::from_env() {
            Ok(config) => config,
            Err(err) => {
                self.status_message = err.to_string();
                return;
            }
        };

        self.status_message = "Opening EVE SSO login...".to_string();
        self.login_receiver = Some(auth::start_login(config));
    }

    pub fn poll_character_login(&mut self) {
        let Some(receiver) = &self.login_receiver else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(character)) => {
                self.status_message = format!("Added {}", character.character_name);
                upsert_character(&mut self.characters, character);
                self.login_receiver = None;
            }
            Ok(Err(error)) => {
                self.status_message = format!("Login failed: {error}");
                self.login_receiver = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.status_message = "Login failed: SSO worker stopped".to_string();
                self.login_receiver = None;
            }
        }
    }

    pub fn set_destination(&mut self) {
        let destination = self.destination.trim().to_owned();

        if destination.is_empty() {
            self.status_message = "Destination required".to_string();
            return;
        }

        self.status_message = format!("Destination queued: {destination}");
        debug!(
            destination,
            mode = ?self.mode,
            pinned = self.pin_destination,
            "Destination queued"
        );
    }

    pub fn clear_destination_form(&mut self) {
        self.destination.clear();
        self.notes.clear();
        self.pin_destination = false;
        self.status_message = "Cleared".to_string();
    }
}

fn upsert_character(
    characters: &mut Vec<AuthenticatedCharacter>,
    character: AuthenticatedCharacter,
) {
    if let Some(existing) = characters
        .iter_mut()
        .find(|existing| existing.character_id == character.character_id)
    {
        *existing = character;
    } else {
        characters.push(character);
    }
}
