use eframe::egui;

use crate::app_state::{AppTab, SetDestoApp};

use super::{characters, destination_form, esi_settings};

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(8.0);
        match app.active_tab {
            AppTab::Destination => destination_form::render(ui, app),
            AppTab::Characters => characters::render(ui, app),
            AppTab::Esi => esi_settings::render(ui, app),
        }
    });
}
