use eframe::egui;

use crate::app_state::{FobImportSortColumn, SetDestoApp};
use crate::eve::waypoints::WaypointRouteMode;

const DESTINATION_INPUT_MIN_WIDTH: f32 = 160.0;
const SET_DESTINATION_BUTTON_WIDTH: f32 = 116.0;
const DESTINATION_ROW_RIGHT_PADDING: f32 = 8.0;
const IMPORT_MODAL_HORIZONTAL_MARGIN: f32 = 32.0;
const IMPORT_MODAL_VERTICAL_MARGIN: f32 = 32.0;
const IMPORT_MODAL_MIN_WIDTH: f32 = 560.0;
const IMPORT_MODAL_MIN_HEIGHT: f32 = 420.0;
const IMPORT_PASTE_TARGET_HEIGHT: f32 = 48.0;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.heading("Set Destination");

    ui.add_space(12.0);
    render_destination_controls(ui, app);
    render_import_modal(ui.ctx(), app);

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

        ui.separator();
        if ui
            .add_enabled(
                !app.waypoint_send_in_progress(),
                egui::Button::new("Import"),
            )
            .clicked()
        {
            app.open_fob_import();
        }
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

fn render_import_modal(ctx: &egui::Context, app: &mut SetDestoApp) {
    if !app.fob_import_open {
        return;
    }

    handle_import_paste(ctx, app);

    let mut open = app.fob_import_open;
    let content_rect = ctx.content_rect();
    let modal_size = egui::vec2(
        (content_rect.width() - IMPORT_MODAL_HORIZONTAL_MARGIN * 2.0).max(IMPORT_MODAL_MIN_WIDTH),
        (content_rect.height() - IMPORT_MODAL_VERTICAL_MARGIN * 2.0).max(IMPORT_MODAL_MIN_HEIGHT),
    );
    egui::Window::new("Import FOBScout Export")
        .id(egui::Id::new("fobscout_import_modal"))
        .collapsible(false)
        .resizable(true)
        .default_width(modal_size.x)
        .default_height(modal_size.y)
        .default_pos(content_rect.center())
        .pivot(egui::Align2::CENTER_CENTER)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(&app.sde_cache_status);
            ui.add_space(8.0);

            render_import_paste_target(ui, app);
            ui.add_space(8.0);
            render_import_messages(ui, app);

            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} valid, {} selected",
                    app.valid_fob_import_count(),
                    app.selected_fob_import_count()
                ));
                if app.selected_fob_import_count() > crate::sde::MAX_OPTIMIZED_ROUTE_STOPS {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "Select {} or fewer systems",
                            crate::sde::MAX_OPTIMIZED_ROUTE_STOPS
                        ),
                    );
                }
            });

            ui.add_space(6.0);
            render_import_table(ui, app);

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let route_button = ui.add_enabled(
                    app.can_set_imported_fob_route(),
                    egui::Button::new("Set Optimized Route"),
                );
                if route_button.clicked() {
                    app.set_imported_fob_route();
                }

                if ui
                    .add_enabled(
                        !app.fob_import_text.trim().is_empty(),
                        egui::Button::new("Clear"),
                    )
                    .clicked()
                {
                    app.set_fob_import_text(String::new());
                }

                if ui.button("Close").clicked() {
                    app.close_fob_import();
                }
            });
        });

    if app.fob_import_open {
        app.fob_import_open = open;
    }
}

fn handle_import_paste(ctx: &egui::Context, app: &mut SetDestoApp) {
    let pasted = ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Paste(text) if !text.trim().is_empty() => Some(text.clone()),
                _ => None,
            })
            .next_back()
    });

    if let Some(text) = pasted {
        app.set_fob_import_text(text);
    }
}

fn render_import_paste_target(ui: &mut egui::Ui, app: &SetDestoApp) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(IMPORT_PASTE_TARGET_HEIGHT);
        ui.vertical_centered(|ui| {
            ui.add_space(7.0);
            if app.fob_import_text.trim().is_empty() {
                single_line_label(ui, egui::RichText::new("Paste FOBScout export").strong());
            } else {
                single_line_label(ui, egui::RichText::new("FOBScout export loaded").strong());
            }
        });
    });
}

fn render_import_messages(ui: &mut egui::Ui, app: &SetDestoApp) {
    for message in &app.fob_import_messages {
        ui.add(
            egui::Label::new(egui::RichText::new(message).color(ui.visuals().warn_fg_color))
                .extend(),
        );
    }
}

fn render_import_table(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    let rows = app.sorted_fob_import_rows();
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .max_height(240.0)
        .show(ui, |ui| {
            egui::Grid::new("fobscout_import_grid")
                .num_columns(5)
                .spacing([12.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    sort_header(ui, app, FobImportSortColumn::Use);
                    sort_header(ui, app, FobImportSortColumn::System);
                    sort_header(ui, app, FobImportSortColumn::Region);
                    sort_header(ui, app, FobImportSortColumn::LastSeenUtc);
                    sort_header(ui, app, FobImportSortColumn::ClaimedBy);
                    ui.end_row();

                    for row in rows {
                        let mut selected = row.selected;
                        let changed = ui
                            .add_enabled(row.valid(), egui::Checkbox::new(&mut selected, ""))
                            .changed();

                        ui.vertical(|ui| {
                            single_line_label(ui, row.display_system());
                            if let Some(error) = &row.error {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(error)
                                            .color(ui.visuals().error_fg_color),
                                    )
                                    .extend(),
                                );
                            } else if row.resolved_system_name.as_deref()
                                != Some(row.input_system.as_str())
                            {
                                single_line_label(
                                    ui,
                                    egui::RichText::new(format!(
                                        "Imported as {}",
                                        row.input_system
                                    ))
                                    .small(),
                                );
                            }
                        });
                        single_line_label(ui, row.region);
                        single_line_label(ui, row.last_seen_utc);
                        single_line_label(ui, row.claimed_by);
                        ui.end_row();

                        if changed {
                            app.set_fob_import_selected(row.import_index, selected);
                        }
                    }
                });
        });
}

fn sort_header(ui: &mut egui::Ui, app: &mut SetDestoApp, column: FobImportSortColumn) {
    let arrow = if app.fob_import_sort_column == column {
        if app.fob_import_sort_ascending {
            "⬆"
        } else {
            "⬇"
        }
    } else {
        "↕"
    };
    let label = format!("{} {arrow}", column.label());
    if ui
        .add(egui::Button::new(label).wrap_mode(egui::TextWrapMode::Extend))
        .clicked()
    {
        app.toggle_fob_import_sort(column);
    }
}

fn single_line_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.add(egui::Label::new(text).extend());
}
