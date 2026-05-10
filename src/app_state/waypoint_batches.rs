use std::sync::mpsc::{self, TryRecvError};

use tracing::{debug, error, info, warn};

use crate::eve::sso::SsoConfig;

use super::models::{
    AccessTokenUpdate, WaypointBatchSummary, WaypointSendEvent, WaypointSendJob,
    WaypointSendProgress, WaypointSendRequest, WaypointSendSuccess, has_failed_send_result,
};
use super::waypoint_worker::start_waypoint_send;
use super::{AppTab, CharacterSendResult, SetDestoApp};

impl SetDestoApp {
    pub fn failed_send_count(&self) -> usize {
        self.characters
            .iter()
            .filter(|character| has_failed_send_result(character))
            .count()
    }

    pub fn can_retry_failed_waypoints(&self) -> bool {
        !self.waypoint_send_in_progress()
            && self.last_waypoint_request.is_some()
            && self.failed_send_count() > 0
    }

    pub fn retry_failed_waypoints(&mut self) {
        if self.waypoint_send_in_progress() {
            debug!("Ignoring Retry Failed because a waypoint send is already in progress");
            self.status_message = "Waypoint send already in progress".to_string();
            return;
        }

        let Some(request) = self.last_waypoint_request.clone() else {
            self.status_message = "No failed waypoint batch to retry".to_string();
            return;
        };

        let target_ids: Vec<u64> = self
            .characters
            .iter()
            .filter(|character| {
                matches!(
                    character.last_send_result.as_ref(),
                    Some(CharacterSendResult::Failed { destination_id, .. })
                        if *destination_id == request.destination_id
                )
            })
            .map(|character| character.character_id)
            .collect();

        if target_ids.is_empty() {
            self.status_message = "No failed waypoint sends to retry".to_string();
            return;
        }

        info!(
            destination_id = request.destination_id,
            destination_name = %request.destination_name,
            retry_count = target_ids.len(),
            "Retrying failed waypoint sends"
        );
        self.start_waypoint_send_batch(request, target_ids, 0, false);
    }

    pub fn poll_waypoint_send(&mut self) {
        loop {
            let event = match self
                .waypoint_send_receiver
                .as_ref()
                .map(|receiver| receiver.try_recv())
            {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Empty)) | None => return,
                Some(Err(TryRecvError::Disconnected)) => {
                    error!("Waypoint send worker disconnected");
                    self.status_message = "Waypoint send failed: worker stopped".to_string();
                    self.waypoint_send_receiver = None;
                    self.waypoint_send_progress = None;
                    return;
                }
            };

            self.handle_waypoint_send_event(event);
        }
    }

    pub(super) fn start_waypoint_send_batch(
        &mut self,
        request: WaypointSendRequest,
        target_ids: Vec<u64>,
        skipped: usize,
        mark_non_targets_skipped: bool,
    ) {
        let sso_config = match self.sso_config() {
            Ok(config) => config,
            Err(err) => {
                warn!(error = ?err, "Cannot start waypoint send");
                self.status_message = err.to_string();
                self.active_tab = AppTab::Esi;
                return;
            }
        };
        self.prepare_send_results(&request, &target_ids, mark_non_targets_skipped);

        let jobs = self.waypoint_send_jobs(&request, &target_ids, &sso_config);
        let total = jobs.len();
        let (sender, receiver) = mpsc::channel();
        start_waypoint_send(jobs, sender);

        self.waypoint_send_receiver = Some(receiver);
        self.waypoint_send_progress = Some(WaypointSendProgress {
            destination_name: request.destination_name.clone(),
            destination_id: request.destination_id,
            total,
            completed: 0,
            successes: 0,
            failures: 0,
            skipped,
            latest_error: None,
        });
        self.last_waypoint_batch = None;
        self.last_waypoint_request = Some(request.clone());
        self.status_message = format!(
            "Sending {} ({}) to {total} characters...",
            request.destination_name, request.destination_id
        );
    }

    fn prepare_send_results(
        &mut self,
        request: &WaypointSendRequest,
        target_ids: &[u64],
        mark_non_targets_skipped: bool,
    ) {
        for character in &mut self.characters {
            if target_ids.contains(&character.character_id) {
                character.last_send_result = Some(CharacterSendResult::Pending {
                    destination_name: request.destination_name.clone(),
                    destination_id: request.destination_id,
                });
            } else if mark_non_targets_skipped {
                character.last_send_result = Some(CharacterSendResult::Skipped {
                    reason: "not selected".to_string(),
                });
            }
        }
    }

    fn waypoint_send_jobs(
        &self,
        request: &WaypointSendRequest,
        target_ids: &[u64],
        sso_config: &SsoConfig,
    ) -> Vec<WaypointSendJob> {
        self.characters
            .iter()
            .filter(|character| target_ids.contains(&character.character_id))
            .map(|character| WaypointSendJob {
                character_id: character.character_id,
                character_name: character.character_name.clone(),
                access_token: character.access_token.clone(),
                expires_at: character.expires_at,
                destination_name: request.destination_name.clone(),
                destination_id: request.destination_id,
                options: request.options,
                token_store: self.token_store,
                sso_config: sso_config.clone(),
            })
            .collect()
    }

    fn handle_waypoint_send_event(&mut self, event: WaypointSendEvent) {
        match event {
            WaypointSendEvent::Started {
                character_id,
                character_name,
            } => {
                debug!(character_id, character_name, "Waypoint send started");
                self.status_message = format!("Sending waypoint for {character_name}...");
            }
            WaypointSendEvent::Finished {
                character_id,
                character_name,
                destination_name,
                destination_id,
                result,
            } => {
                self.record_waypoint_send_result(
                    character_id,
                    character_name,
                    destination_name,
                    destination_id,
                    result,
                );
            }
            WaypointSendEvent::BatchFinished => {
                self.finish_waypoint_send();
            }
        }
    }

    fn record_waypoint_send_result(
        &mut self,
        character_id: u64,
        character_name: String,
        destination_name: String,
        destination_id: i64,
        result: std::result::Result<WaypointSendSuccess, String>,
    ) {
        let status_message = {
            let Some(progress) = &mut self.waypoint_send_progress else {
                return;
            };
            progress.completed += 1;

            match &result {
                Ok(_) => {
                    progress.successes += 1;
                }
                Err(error) => {
                    progress.failures += 1;
                    progress.latest_error = Some(format!("{character_name}: {error}"));
                }
            }

            progress.status_message()
        };

        match result {
            Ok(success) => {
                if let Some(update) = success.access_token_update {
                    self.apply_access_token_update(character_id, update);
                }
                if let Some(character) = self.character_mut(character_id) {
                    character.last_send_result = Some(CharacterSendResult::Sent {
                        destination_name: destination_name.clone(),
                        destination_id,
                    });
                }
                info!(
                    character_id,
                    character_name, destination_id, "Set waypoint for character"
                );
            }
            Err(error) => {
                if let Some(character) = self.character_mut(character_id) {
                    character.last_send_result = Some(CharacterSendResult::Failed {
                        destination_name: destination_name.clone(),
                        destination_id,
                        error: error.clone(),
                    });
                }
                error!(
                    character_id,
                    character_name, destination_id, error, "Failed to set waypoint for character"
                );
            }
        }

        self.status_message = status_message;
    }

    fn finish_waypoint_send(&mut self) {
        if let Some(progress) = &self.waypoint_send_progress {
            let mut summary = self
                .last_waypoint_request
                .as_ref()
                .map(|request| self.summarize_send_results(request, false))
                .unwrap_or_else(|| progress.summary(false));
            summary.latest_error = progress.latest_error.clone();

            self.status_message = summary.status_message();
            self.last_waypoint_batch = Some(summary);
        }
        self.waypoint_send_receiver = None;
        self.waypoint_send_progress = None;
    }

    fn apply_access_token_update(&mut self, character_id: u64, update: AccessTokenUpdate) {
        if let Some(character) = self.character_mut(character_id) {
            character.update_access_token(update.access_token, update.expires_at, update.scopes);
        }
    }

    fn summarize_send_results(
        &self,
        request: &WaypointSendRequest,
        in_progress: bool,
    ) -> WaypointBatchSummary {
        let mut summary = WaypointBatchSummary {
            destination_name: request.destination_name.clone(),
            destination_id: request.destination_id,
            total: 0,
            completed: 0,
            successes: 0,
            failures: 0,
            skipped: 0,
            in_progress,
            latest_error: None,
        };

        for character in &self.characters {
            match character.last_send_result.as_ref() {
                Some(CharacterSendResult::Pending { destination_id, .. })
                    if *destination_id == request.destination_id =>
                {
                    summary.total += 1;
                }
                Some(CharacterSendResult::Sent { destination_id, .. })
                    if *destination_id == request.destination_id =>
                {
                    summary.total += 1;
                    summary.completed += 1;
                    summary.successes += 1;
                }
                Some(CharacterSendResult::Failed { destination_id, .. })
                    if *destination_id == request.destination_id =>
                {
                    summary.total += 1;
                    summary.completed += 1;
                    summary.failures += 1;
                }
                Some(CharacterSendResult::Skipped { .. }) => {
                    summary.skipped += 1;
                }
                _ => {}
            }
        }

        summary
    }
}
