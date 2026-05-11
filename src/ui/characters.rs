use eframe::egui;

use crate::app_state::{CharacterSortColumn, SetDestoApp};

const CHARACTER_COLUMN_MIN_WIDTH: f32 = 120.0;
const CHARACTER_COLUMN_MAX_WIDTH: f32 = 420.0;
const CHARACTER_TABLE_FIXED_WIDTH: f32 = 280.0;
const CHARACTER_ACTION_BUTTON_WIDTH: f32 = 72.0;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.horizontal(|ui| {
        ui.heading("Character Management");

        let add_button = ui.add_enabled(
            !app.login_in_progress() && !app.waypoint_send_in_progress(),
            egui::Button::new("Add Character"),
        );
        if add_button.clicked() {
            app.start_character_login();
        }
    });

    ui.add_space(8.0);

    if app.characters.is_empty() {
        ui.label("No characters added.");
        return;
    }

    render_selection_toolbar(ui, app);

    ui.add_space(10.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_character_table(ui, app);
        });
}

fn render_character_table(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    let character_ids = app.sorted_character_ids();
    let character_column_width = character_column_width(ui.available_width());

    egui::Grid::new("character_manager_grid")
        .num_columns(4)
        .spacing([14.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            render_sort_header(ui, app, CharacterSortColumn::Selected, "Use");
            render_sort_header(ui, app, CharacterSortColumn::Name, "Character");
            render_sort_header(ui, app, CharacterSortColumn::AddedAt, "Added");
            ui.label("Actions");
            ui.end_row();

            for character_id in character_ids {
                render_character_row(ui, app, character_id, character_column_width);
            }
        });
}

fn render_sort_header(
    ui: &mut egui::Ui,
    app: &mut SetDestoApp,
    column: CharacterSortColumn,
    label: &str,
) {
    let marker = app.character_sort_marker(column);
    let button_label = if marker.is_empty() {
        label.to_string()
    } else {
        format!("{label} {marker}")
    };

    if ui.button(button_label).clicked() {
        app.set_character_sort_column(column);
    }
}

fn render_selection_toolbar(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    let send_in_progress = app.waypoint_send_in_progress();

    ui.horizontal(|ui| {
        ui.label(format!(
            "{}/{} selected",
            app.selected_character_count(),
            app.characters.len()
        ));

        if ui
            .add_enabled(!send_in_progress, egui::Button::new("All"))
            .clicked()
        {
            app.set_all_characters_selected(true);
        }

        if ui
            .add_enabled(!send_in_progress, egui::Button::new("None"))
            .clicked()
        {
            app.set_all_characters_selected(false);
        }

        if ui
            .add_enabled(!send_in_progress, egui::Button::new("Invert"))
            .clicked()
        {
            app.invert_character_selection();
        }
    });
}

fn render_character_row(
    ui: &mut egui::Ui,
    app: &mut SetDestoApp,
    character_id: u64,
    character_column_width: f32,
) {
    let (
        character_id,
        character_name,
        added_at_unix_seconds,
        token_summary,
        send_result_summary,
        mut selected,
        remove_pending,
    ) = {
        let Some(character) = app
            .characters
            .iter()
            .find(|character| character.character_id == character_id)
        else {
            return;
        };
        (
            character.character_id,
            character.character_name.clone(),
            character.added_at_unix_seconds,
            character.token_summary(),
            character
                .last_send_result
                .as_ref()
                .map(|result| result.summary()),
            character.selected,
            app.pending_remove_character_id == Some(character.character_id),
        )
    };
    let mut remove_clicked = false;
    let mut confirm_clicked = false;
    let mut cancel_clicked = false;
    let send_in_progress = app.waypoint_send_in_progress();

    let selection_changed = ui
        .add_enabled(!send_in_progress, egui::Checkbox::new(&mut selected, ""))
        .changed();

    ui.vertical(|ui| {
        ui.set_min_width(character_column_width);
        ui.set_max_width(character_column_width);
        ui.add(egui::Label::new(egui::RichText::new(&character_name).strong()).wrap());
        ui.add(egui::Label::new(egui::RichText::new(format!("ID {character_id}")).small()).wrap());
        ui.add(egui::Label::new(egui::RichText::new(token_summary).small()).wrap());

        if let Some(send_result_summary) = send_result_summary {
            ui.add(egui::Label::new(egui::RichText::new(send_result_summary).small()).wrap());
        }
    });

    ui.label(format_added_at(added_at_unix_seconds));

    if remove_pending {
        ui.vertical(|ui| {
            confirm_clicked = ui
                .add_enabled(
                    !send_in_progress,
                    egui::Button::new("Confirm")
                        .min_size(egui::vec2(CHARACTER_ACTION_BUTTON_WIDTH, 0.0)),
                )
                .clicked();
            cancel_clicked = ui
                .add(
                    egui::Button::new("Cancel")
                        .min_size(egui::vec2(CHARACTER_ACTION_BUTTON_WIDTH, 0.0)),
                )
                .clicked();
        });
    } else {
        remove_clicked = ui
            .add_enabled(
                !send_in_progress,
                egui::Button::new("Remove")
                    .min_size(egui::vec2(CHARACTER_ACTION_BUTTON_WIDTH, 0.0)),
            )
            .clicked();
    }

    ui.end_row();

    if selection_changed {
        app.set_character_selected(character_id, selected);
    }

    if remove_clicked {
        app.request_remove_character(character_id);
    }

    if confirm_clicked {
        app.confirm_remove_character(character_id);
    }

    if cancel_clicked {
        app.cancel_remove_character();
    }
}

fn character_column_width(available_width: f32) -> f32 {
    (available_width - CHARACTER_TABLE_FIXED_WIDTH)
        .clamp(CHARACTER_COLUMN_MIN_WIDTH, CHARACTER_COLUMN_MAX_WIDTH)
}

fn format_added_at(unix_seconds: u64) -> String {
    let days_since_epoch = (unix_seconds / 86_400) as i64;
    let (year, month, day) = civil_date_from_unix_days(days_since_epoch);

    format!("{year:04}-{month:02}-{day:02}")
}

// Converts days since 1970-01-01 to a UTC calendar date without taking a
// dependency just for this small display need.
fn civil_date_from_unix_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };

    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_added_at_dates() {
        assert_eq!(format_added_at(0), "1970-01-01");
        assert_eq!(format_added_at(1_778_371_200), "2026-05-10");
    }

    #[test]
    fn character_column_width_scales_with_available_space() {
        assert_eq!(character_column_width(260.0), CHARACTER_COLUMN_MIN_WIDTH);
        assert_eq!(character_column_width(500.0), 220.0);
        assert_eq!(character_column_width(900.0), CHARACTER_COLUMN_MAX_WIDTH);
    }
}
