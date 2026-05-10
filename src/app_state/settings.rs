use tracing::{error, info};

use crate::eve::sso;

use super::SetDestoApp;

impl SetDestoApp {
    pub fn esi_client_id_is_saved(&self) -> bool {
        let client_id = self.esi_client_id.trim();
        !client_id.is_empty() && client_id == self.config.esi.client_id.trim()
    }

    pub fn esi_client_id_is_configured(&self) -> bool {
        !self.esi_client_id.trim().is_empty()
    }

    pub fn effective_redirect_uri(&self) -> String {
        sso::redirect_uri()
    }

    pub fn save_esi_settings(&mut self) {
        let client_id = self.esi_client_id.trim().to_string();
        if client_id.is_empty() {
            self.status_message = "EVE application Client ID required".to_string();
            return;
        }

        self.config.esi.client_id = client_id;
        match self.config.save() {
            Ok(()) => {
                self.esi_client_id = self.config.esi.client_id.clone();
                self.status_message = "Saved ESI settings".to_string();
                info!("Saved ESI settings");
            }
            Err(err) => {
                error!(error = ?err, "Failed to save ESI settings");
                self.status_message = format!("Failed to save ESI settings: {err}");
            }
        }
    }

    pub fn mark_redirect_uri_copied(&mut self) {
        self.status_message = "Copied redirect URI".to_string();
    }
}
