use eframe::egui;

use crate::app_state::SetDestoApp;

mod characters;
mod destination_form;
mod esi_settings;
mod favorites;
mod header;
mod main_panel;
mod status_bar;

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    app.poll_character_login();
    app.poll_sde_cache();
    app.poll_waypoint_send();
    header::render(ctx, app);
    main_panel::render(ctx, app);
    status_bar::render(ctx, app);
}
