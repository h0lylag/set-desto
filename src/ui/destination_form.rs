use eframe::egui;

use crate::app_state::SetDestoApp;
use crate::eve::waypoints::WaypointRouteMode;

const DESTINATION_INPUT_MIN_WIDTH: f32 = 160.0;
const SET_DESTINATION_BUTTON_WIDTH: f32 = 116.0;
const DESTINATION_ROW_RIGHT_PADDING: f32 = 8.0;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.heading("Set Destination");

    ui.add_space(12.0);
    render_destination_controls(ui, app);

    ui.add_space(16.0);
    render_favorite_buttons(ui, app);
}

fn render_destination_controls(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    render_control_toolbar(ui, app);

    ui.add_space(10.0);
    render_destination_row(ui, app);
}

fn render_control_toolbar(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.horizontal(|ui| {
        ui.label(format!(
            "{}/{} selected targets",
            app.selected_character_count(),
            app.characters.len()
        ));

        ui.separator();
        egui::ComboBox::from_id_salt("waypoint_route_mode")
            .selected_text(app.waypoint_route_mode.label())
            .show_ui(ui, |ui| {
                for mode in WaypointRouteMode::ALL {
                    ui.selectable_value(&mut app.waypoint_route_mode, mode, mode.label());
                }
            });
    });
}

fn render_favorite_buttons(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    if app.favorites.is_empty() {
        return;
    }

    ui.separator();
    ui.label("Favorites");

    let favorites: Vec<(i64, String)> = app
        .favorites
        .iter()
        .map(|favorite| (favorite.destination_id, favorite.display_name().to_string()))
        .collect();
    let favorite_send_enabled = app.favorite_send_enabled();

    ui.horizontal_wrapped(|ui| {
        for (destination_id, destination_name) in favorites {
            if ui
                .add_enabled(favorite_send_enabled, egui::Button::new(destination_name))
                .clicked()
            {
                app.set_favorite_destination(destination_id);
            }
        }
    });
}

fn render_destination_row(ui: &mut egui::Ui, app: &mut SetDestoApp) {
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

    egui::Grid::new("destination_form_grid")
        .num_columns(2)
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            ui.label("Destination");
            ui.horizontal(|ui| {
                let input_width = (ui.available_width()
                    - SET_DESTINATION_BUTTON_WIDTH
                    - ui.spacing().item_spacing.x
                    - DESTINATION_ROW_RIGHT_PADDING)
                    .max(DESTINATION_INPUT_MIN_WIDTH);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.destination)
                        .hint_text("System, station, or structure")
                        .desired_width(input_width),
                );
                if response.changed() {
                    app.clear_resolved_destination();
                }

                if ui
                    .add_enabled(
                        can_set_destination,
                        egui::Button::new(set_destination_label).min_size(egui::vec2(
                            SET_DESTINATION_BUTTON_WIDTH,
                            ui.spacing().interact_size.y,
                        )),
                    )
                    .clicked()
                {
                    app.set_destination();
                }

                ui.add_space(DESTINATION_ROW_RIGHT_PADDING);
            });
            ui.end_row();

            if let Some(resolved_destination) = app.resolved_destination_summary() {
                ui.label("");
                ui.label(format!("Resolved: {resolved_destination}"));
                ui.end_row();
            }
        });
}
