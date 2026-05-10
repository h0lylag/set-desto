use eframe::egui;

use crate::app_state::SetDestoApp;

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
    ui.horizontal(|ui| {
        if ui.button("Add Favorite").clicked() {
            app.add_favorite_from_input();
        }

        if ui
            .add_enabled(
                app.has_resolved_destination(),
                egui::Button::new("Add Resolved"),
            )
            .clicked()
        {
            app.add_resolved_destination_to_favorites();
        }
    });
}

fn render_favorite_list(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    if app.favorites.is_empty() {
        ui.label("No favorites saved.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let favorites: Vec<(i64, String, bool)> = app
                .favorites
                .iter()
                .map(|favorite| {
                    (
                        favorite.destination_id,
                        favorite.summary(),
                        app.pending_remove_favorite_destination_id == Some(favorite.destination_id),
                    )
                })
                .collect();

            for (destination_id, summary, remove_pending) in favorites {
                let mut remove_clicked = false;
                let mut confirm_clicked = false;
                let mut cancel_clicked = false;

                ui.horizontal(|ui| {
                    ui.label(summary);

                    if remove_pending {
                        ui.label("Remove?");
                        confirm_clicked = ui.button("Confirm").clicked();
                        cancel_clicked = ui.button("Cancel").clicked();
                    } else if ui.button("Remove").clicked() {
                        remove_clicked = true;
                    }
                });

                if remove_clicked {
                    app.request_remove_favorite_destination(destination_id);
                }

                if confirm_clicked {
                    app.confirm_remove_favorite_destination(destination_id);
                }

                if cancel_clicked {
                    app.cancel_remove_favorite_destination();
                }
            }
        });
}
