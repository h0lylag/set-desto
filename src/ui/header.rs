use eframe::egui;

use crate::app_state::{AppTab, SetDestoApp};

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
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

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut app.active_tab,
                AppTab::Destination,
                AppTab::Destination.label(),
            );
            ui.selectable_value(
                &mut app.active_tab,
                AppTab::Characters,
                AppTab::Characters.label(),
            );
            ui.selectable_value(&mut app.active_tab, AppTab::Esi, AppTab::Esi.label());
        });
    });
}
