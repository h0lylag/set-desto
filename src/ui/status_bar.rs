use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ctx: &egui::Context, app: &SetDestoApp) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(&app.status_message);
        });
    });
}
