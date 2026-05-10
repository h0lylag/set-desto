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
            let favorites: Vec<(i64, String)> = app
                .favorites
                .iter()
                .map(|favorite| (favorite.destination_id, favorite.summary()))
                .collect();

            for (destination_id, summary) in favorites {
                ui.horizontal(|ui| {
                    ui.label(summary);

                    if ui.button("Set").clicked() {
                        app.set_favorite_destination(destination_id);
                    }

                    if ui.button("Remove").clicked() {
                        app.remove_favorite_destination(destination_id);
                    }
                });
            }
        });
}
