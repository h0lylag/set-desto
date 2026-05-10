use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ctx: &egui::Context, app: &SetDestoApp) {
    egui::TopBottomPanel::top("header").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Set Desto");
            ui.separator();
            ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));

            if app.debug_mode {
                ui.separator();
                ui.strong("Debug");
            }
        });
    });
}
