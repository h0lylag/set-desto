use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ui: &mut egui::Ui, app: &mut SetDestoApp) {
    ui.heading("ESI Settings");

    ui.add_space(16.0);
    ui.label("Client ID");
    ui.add(
        egui::TextEdit::singleline(&mut app.esi_client_id)
            .hint_text("EVE application client ID")
            .desired_width(f32::INFINITY),
    );

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            app.save_esi_settings();
        }

        ui.label(settings_status(app));
    });

    ui.add_space(16.0);
    ui.label("Redirect URI");
    ui.monospace(app.effective_redirect_uri());
}

fn settings_status(app: &SetDestoApp) -> &'static str {
    if app.esi_client_id_is_saved() {
        "Saved"
    } else if app.esi_client_id_is_configured() {
        "Unsaved"
    } else {
        "Missing"
    }
}
