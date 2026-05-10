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

    ui.horizontal(|ui| {
        ui.label(format!(
            "{}/{} selected",
            app.selected_character_count(),
            app.characters.len()
        ));

        if ui.button("All").clicked() {
            app.set_all_characters_selected(true);
        }

        if ui.button("None").clicked() {
            app.set_all_characters_selected(false);
        }

        if ui.button("Invert").clicked() {
            app.invert_character_selection();
        }
    });

    ui.add_space(6.0);

    for index in 0..app.characters.len() {
        let (character_id, character_name, token_summary, mut selected) = {
            let character = &app.characters[index];
            (
                character.character_id,
                character.character_name.clone(),
                character.token_summary(),
                character.selected,
            )
        };
        let mut selection_changed = false;

        ui.horizontal(|ui| {
            selection_changed = ui.checkbox(&mut selected, "").changed();
            ui.strong(&character_name);
            ui.label(format!("ID {character_id}"));
            ui.label(token_summary);
        });

        if selection_changed {
            app.set_character_selected(character_id, selected);
        }
    }
}
