use eframe::egui;

use crate::app_state::SetDestoApp;

use super::{characters, destination_form};

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(8.0);
        characters::render(ui, app);

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(16.0);

        destination_form::render(ui, app);
    });
}
