use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    app.poll_character_login();

    ui.horizontal(|ui| {
        ui.heading("Characters");

        let add_button =
            ui.add_enabled(!app.login_in_progress(), egui::Button::new("Add Character"));
        if add_button.clicked() {
            app.start_character_login();
        }
    });

    ui.add_space(8.0);

    if app.characters.is_empty() {
        ui.label("No characters added.");
        return;
    }

    for character in &app.characters {
        ui.horizontal(|ui| {
            ui.strong(&character.character_name);
            ui.label(format!("ID {}", character.character_id));
            ui.label(character.token_summary());
        });
    }
}
