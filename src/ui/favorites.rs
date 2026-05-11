use eframe::egui;

use crate::app_state::SetDestoApp;

const FAVORITE_DETAIL_MIN_WIDTH: f32 = 220.0;
const FAVORITE_DETAIL_MAX_WIDTH: f32 = 620.0;
const FAVORITE_FIXED_WIDTH: f32 = 100.0;
const FAVORITE_NICKNAME_MIN_WIDTH: f32 = 120.0;
const FAVORITE_NICKNAME_MAX_WIDTH: f32 = 280.0;
const FAVORITE_ACTION_BUTTON_WIDTH: f32 = 72.0;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.heading("Favorites");

    ui.add_space(16.0);
    render_add_favorite(ui, app);

    ui.add_space(16.0);
    render_favorite_list(ui, app);
}

fn render_add_favorite(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.label("Destination");
    ui.add(
        egui::TextEdit::singleline(&mut app.favorite_destination_input)
            .hint_text("System, station, or numeric destination ID")
            .desired_width(f32::INFINITY),
    );

    ui.add_space(8.0);
    if ui.button("Add Favorite").clicked() {
        app.add_favorite_from_input();
    }
}

fn render_favorite_list(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    if app.favorites.is_empty() {
        ui.label("No favorites saved.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let favorites: Vec<FavoriteEditorRow> = app
                .favorites
                .iter()
                .map(|favorite| FavoriteEditorRow {
                    destination_id: favorite.destination_id,
                    destination_name: favorite.destination_name.clone(),
                    destination_kind: favorite.destination_kind.clone(),
                    nickname: favorite.nickname.clone(),
                    remove_pending: app.pending_remove_favorite_destination_id
                        == Some(favorite.destination_id),
                })
                .collect();

            let detail_width = favorite_detail_width(ui.available_width());
            egui::Grid::new("favorites_editor_grid")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Favorite");
                    ui.label("Actions");
                    ui.end_row();

                    for favorite in favorites {
                        render_favorite_row(ui, app, favorite, detail_width);
                    }
                });
        });
}

fn render_favorite_row(
    ui: &mut egui::Ui,
    app: &mut SetDestoApp,
    favorite: FavoriteEditorRow,
    detail_width: f32,
) {
    let mut save_clicked = false;
    let mut reset_clicked = false;
    let mut remove_clicked = false;
    let mut confirm_clicked = false;
    let mut cancel_clicked = false;

    ui.vertical(|ui| {
        ui.set_min_width(detail_width);
        ui.set_max_width(detail_width);

        ui.add(egui::Label::new(egui::RichText::new(&favorite.destination_name).strong()).wrap());
        ui.label(
            egui::RichText::new(format!(
                "{} - {}",
                favorite.destination_id, favorite.destination_kind
            ))
            .small(),
        );
        ui.add_space(4.0);

        let nickname = app
            .favorite_nickname_edits
            .entry(favorite.destination_id)
            .or_insert_with(|| favorite.nickname.clone());
        let changed = nickname.trim() != favorite.nickname.trim();
        let nickname_width = favorite_nickname_width(detail_width);

        ui.horizontal_wrapped(|ui| {
            ui.label("Nickname");
            ui.add(
                egui::TextEdit::singleline(nickname)
                    .hint_text("Optional")
                    .desired_width(nickname_width),
            );
            save_clicked = ui.add_enabled(changed, egui::Button::new("Save")).clicked();
            reset_clicked = ui
                .add_enabled(changed, egui::Button::new("Reset"))
                .clicked();
        });
    });

    if favorite.remove_pending {
        ui.vertical(|ui| {
            confirm_clicked = ui
                .add(
                    egui::Button::new("Confirm")
                        .min_size(egui::vec2(FAVORITE_ACTION_BUTTON_WIDTH, 0.0)),
                )
                .clicked();
            cancel_clicked = ui
                .add(
                    egui::Button::new("Cancel")
                        .min_size(egui::vec2(FAVORITE_ACTION_BUTTON_WIDTH, 0.0)),
                )
                .clicked();
        });
    } else {
        remove_clicked = ui
            .add(
                egui::Button::new("Remove").min_size(egui::vec2(FAVORITE_ACTION_BUTTON_WIDTH, 0.0)),
            )
            .clicked();
    }

    ui.end_row();

    if save_clicked {
        app.save_favorite_nickname(favorite.destination_id);
    }

    if reset_clicked {
        app.reset_favorite_nickname_edit(favorite.destination_id);
    }

    if remove_clicked {
        app.request_remove_favorite_destination(favorite.destination_id);
    }

    if confirm_clicked {
        app.confirm_remove_favorite_destination(favorite.destination_id);
    }

    if cancel_clicked {
        app.cancel_remove_favorite_destination();
    }
}

struct FavoriteEditorRow {
    destination_id: i64,
    destination_name: String,
    destination_kind: String,
    nickname: String,
    remove_pending: bool,
}

fn favorite_detail_width(available_width: f32) -> f32 {
    (available_width - FAVORITE_FIXED_WIDTH)
        .clamp(FAVORITE_DETAIL_MIN_WIDTH, FAVORITE_DETAIL_MAX_WIDTH)
}

fn favorite_nickname_width(detail_width: f32) -> f32 {
    (detail_width - 180.0).clamp(FAVORITE_NICKNAME_MIN_WIDTH, FAVORITE_NICKNAME_MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorite_detail_width_scales_with_available_space() {
        assert_eq!(favorite_detail_width(260.0), FAVORITE_DETAIL_MIN_WIDTH);
        assert_eq!(favorite_detail_width(500.0), 400.0);
        assert_eq!(favorite_detail_width(900.0), FAVORITE_DETAIL_MAX_WIDTH);
    }

    #[test]
    fn favorite_nickname_width_scales_with_detail_width() {
        assert_eq!(favorite_nickname_width(220.0), FAVORITE_NICKNAME_MIN_WIDTH);
        assert_eq!(favorite_nickname_width(420.0), 240.0);
        assert_eq!(favorite_nickname_width(700.0), FAVORITE_NICKNAME_MAX_WIDTH);
    }
}
