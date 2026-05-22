use std::sync::mpsc::TryRecvError;

use tracing::{error, info, warn};

use crate::sde::SdeCacheEvent;

use super::SetDestoApp;

impl SetDestoApp {
    pub fn poll_sde_cache(&mut self) {
        let Some(receiver) = &self.sde_cache_receiver else {
            return;
        };

        match receiver.try_recv() {
            Ok(SdeCacheEvent::Status { message }) => {
                self.sde_cache_status = message;
            }
            Ok(SdeCacheEvent::Ready {
                graph,
                refreshed,
                message,
            }) => {
                info!(
                    build_number = graph.build_number(),
                    release_date = %graph.release_date(),
                    system_count = graph.system_count(),
                    edge_count = graph.edge_count(),
                    refreshed,
                    "SDE route cache ready"
                );
                self.sde_cache_status = format!(
                    "{} · build {}, {} systems",
                    message,
                    graph.build_number(),
                    format_system_count(graph.system_count())
                );
                self.sde_route_graph = Some(graph);
                self.sde_cache_receiver = None;
                self.refresh_fob_import_preview();
            }
            Ok(SdeCacheEvent::Failed { error }) => {
                if self.sde_route_graph.is_some() {
                    warn!(error, "SDE route cache refresh failed; using cached graph");
                    self.sde_cache_status = format!("using cached map; update failed: {error}");
                } else {
                    error!(error, "SDE route cache unavailable");
                    self.sde_cache_status = format!("unavailable: {error}");
                }
                self.sde_cache_receiver = None;
                self.refresh_fob_import_preview();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                if self.sde_route_graph.is_some() {
                    self.sde_cache_status = "using cached map; updater stopped".to_string();
                } else {
                    self.sde_cache_status = "updater stopped".to_string();
                }
                self.sde_cache_receiver = None;
            }
        }
    }

    pub fn sde_route_graph_ready(&self) -> bool {
        self.sde_route_graph.is_some()
    }

    pub fn sde_cache_in_progress(&self) -> bool {
        self.sde_cache_receiver.is_some()
    }
}

fn format_system_count(count: usize) -> String {
    let digits = count.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(character);
    }

    formatted
}
