use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        if let Some(summary) = app.waypoint_batch_summary()
            && summary.in_progress
        {
            ui.add(
                egui::ProgressBar::new(summary.progress_fraction())
                    .animate(true)
                    .text(summary.progress_text())
                    .desired_width(f32::INFINITY),
            );
        }

        ui.horizontal(|ui| {
            ui.add(egui::Label::new(&app.status_message).truncate());

            if app.can_retry_failed_waypoints() && ui.button("Retry Failed").clicked() {
                app.retry_failed_waypoints();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(format!("Route map: {}", app.sde_cache_status)).truncate());
                if app.sde_cache_in_progress() {
                    ui.add(egui::Spinner::new().size(14.0));
                }
            });
        });
    });
}
