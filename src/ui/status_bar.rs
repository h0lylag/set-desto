use eframe::egui;

use crate::app_state::SetDestoApp;

pub fn render(ctx: &egui::Context, app: &mut SetDestoApp) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(&app.status_message);

            if app.can_retry_failed_waypoints() && ui.button("Retry Failed").clicked() {
                app.retry_failed_waypoints();
            }
        });

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
    });
}
