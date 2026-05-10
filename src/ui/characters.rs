use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.horizontal(|ui| {
        ui.heading("Character Management");

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

    render_selection_toolbar(ui, app);

    ui.add_space(10.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let character_ids: Vec<u64> = app
                .characters
                .iter()
                .map(|character| character.character_id)
                .collect();
            for character_id in character_ids {
                render_character_row(ui, app, character_id);
            }
        });
}

fn render_selection_toolbar(ui: &mut egui::Ui, app: &mut SetDestoApp) {
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
}

fn render_character_row(ui: &mut egui::Ui, app: &mut SetDestoApp, character_id: u64) {
    let (character_id, character_name, token_summary, mut selected, remove_pending) = {
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
            character.token_summary(),
            character.selected,
            app.pending_remove_character_id == Some(character.character_id),
        )
    };
    let mut selection_changed = false;
    let mut remove_clicked = false;
    let mut confirm_clicked = false;
    let mut cancel_clicked = false;

    ui.horizontal(|ui| {
        selection_changed = ui.checkbox(&mut selected, "").changed();
        ui.strong(&character_name);
        ui.label(format!("ID {character_id}"));

        if remove_pending {
            ui.label("Remove?");
            confirm_clicked = ui.button("Confirm").clicked();
            cancel_clicked = ui.button("Cancel").clicked();
        } else {
            remove_clicked = ui.button("Remove").clicked();
        }
    });

    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(token_summary);
    });

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
