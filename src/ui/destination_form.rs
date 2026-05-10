use eframe::egui;

use crate::app_state::{DestoMode, SetDestoApp};

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    render_mode_selector(ui, app);

    ui.add_space(16.0);
    render_destination_input(ui, app);

    ui.add_space(8.0);
    ui.checkbox(&mut app.pin_destination, "Pin destination");

    ui.add_space(16.0);
    render_notes_input(ui, app);

    ui.add_space(16.0);
    render_actions(ui, app);
}

fn render_mode_selector(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.horizontal(|ui| {
        ui.label("Mode");
        ui.selectable_value(&mut app.mode, DestoMode::Manual, DestoMode::Manual.label());
        ui.selectable_value(
            &mut app.mode,
            DestoMode::Clipboard,
            DestoMode::Clipboard.label(),
        );
    });
}

fn render_destination_input(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.label("Destination");
    ui.add(
        egui::TextEdit::singleline(&mut app.destination)
            .hint_text("System, station, or structure")
            .desired_width(f32::INFINITY),
    );
}

fn render_notes_input(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.label("Notes");
    ui.add(
        egui::TextEdit::multiline(&mut app.notes)
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
}

fn render_actions(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.horizontal(|ui| {
        if ui.button("Set Destination").clicked() {
            app.set_destination();
        }

        if ui.button("Clear").clicked() {
            app.clear_destination_form();
        }
    });
}
