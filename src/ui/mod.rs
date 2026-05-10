use eframe::egui;

use crate::app_state::SetDestoApp;

mod destination_form;
mod header;
mod status_bar;

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    header::render(ctx, app);
    destination_form::render(ctx, app);
    status_bar::render(ctx, app);
}
