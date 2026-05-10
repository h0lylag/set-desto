use eframe::egui;

use crate::app_state::{DestoMode, SetDestoApp};

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.heading("Set Destination");

    ui.add_space(8.0);
    render_target_summary(ui, app);

    ui.add_space(16.0);
    render_mode_selector(ui, app);

    ui.add_space(16.0);
    render_destination_input(ui, app);

    ui.add_space(8.0);
    ui.checkbox(&mut app.pin_destination, "Pin destination");

    ui.add_space(16.0);
    render_actions(ui, app);

    render_batch_summary(ui, app);
}

fn render_target_summary(ui: &mut egui::Ui, app: &SetDestoApp) {
    ui.horizontal(|ui| {
        ui.label(format!(
            "{}/{} selected targets",
            app.selected_character_count(),
            app.characters.len()
        ));
    });
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
    let response = ui.add(
        egui::TextEdit::singleline(&mut app.destination)
            .hint_text("System, station, or structure")
            .desired_width(f32::INFINITY),
    );
    if response.changed() {
        app.clear_resolved_destination();
    }

    if let Some(resolved_destination) = app.resolved_destination_summary() {
        ui.add_space(6.0);
        ui.label(format!("Resolved: {resolved_destination}"));
    }
}

fn render_actions(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    let selected_character_count = app.selected_character_count();
    let can_set_destination = !app.destination.trim().is_empty()
        && selected_character_count > 0
        && !app.characters.is_empty()
        && !app.waypoint_send_in_progress();
    let set_destination_label = if app.waypoint_send_in_progress() {
        "Sending..."
    } else {
        "Set Destination"
    };

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                can_set_destination,
                egui::Button::new(set_destination_label),
            )
            .clicked()
        {
            app.set_destination();
        }

        if ui.button("Clear").clicked() {
            app.clear_destination_form();
        }

        ui.label(format!(
            "{selected_character_count}/{} targets",
            app.characters.len()
        ));
    });
}

fn render_batch_summary(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    let Some(summary) = app.waypoint_batch_summary() else {
        return;
    };

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.strong(summary.summary_line());

        if app.can_retry_failed_waypoints() && ui.button("Retry Failed").clicked() {
            app.retry_failed_waypoints();
        }
    });

    if summary.in_progress {
        ui.add(
            egui::ProgressBar::new(summary.progress_fraction())
                .text(summary.progress_text())
                .desired_width(f32::INFINITY),
        );
    }

    if let Some(error) = summary.latest_error {
        ui.label(format!("Latest error: {error}"));
    }
}
