use std::time::Duration;

use anyhow::{Result, anyhow};
use eframe::{NativeOptions, egui};

use crate::app_constants::{
    DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH,
    REPAINT_INTERVAL_MS,
};
use crate::app_state::SetDestoApp;

impl eframe::App for SetDestoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::ui::render(ctx, self);
        ctx.request_repaint_after(Duration::from_millis(REPAINT_INTERVAL_MS));
    }
}

pub fn run(debug_mode: bool) -> Result<()> {
    let viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT])
        .with_title(format!("Set Desto - v{}", env!("CARGO_PKG_VERSION")))
        .with_icon(app_icon()?);

    let options = NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };

    eframe::run_native(
        &format!("Set Desto - v{}", env!("CARGO_PKG_VERSION")),
        options,
        Box::new(move |cc| Ok(Box::new(SetDestoApp::new(cc, debug_mode)))),
    )
    .map_err(|err| anyhow!("Failed to launch Set Desto: {err}"))
}

fn app_icon() -> Result<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/com.h0lylag.setdesto.png"))
        .map_err(|err| anyhow!("Failed to load app icon: {err}"))
}
