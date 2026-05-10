use eframe::egui;
use tracing::debug;

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
    pub destination: String,
    pub notes: String,
    pub pin_destination: bool,
    pub mode: DestoMode,
    pub status_message: String,
}

impl SetDestoApp {
    pub fn new(cc: &eframe::CreationContext<'_>, debug_mode: bool) -> Self {
        debug!("Initializing Set Desto (debug_mode={})", debug_mode);

        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            debug_mode,
            destination: String::new(),
            notes: String::new(),
            pin_destination: false,
            mode: DestoMode::Manual,
            status_message: "Ready".to_string(),
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
