use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Result, anyhow, bail};
use tracing::{debug, error, info};

use crate::app_constants::MAX_CONCURRENT_WAYPOINT_SENDS;
use crate::eve::{location, sso, waypoints};
use crate::storage::tokens::TokenStore;

use super::models::{
    AccessTokenUpdate, WaypointSendEvent, WaypointSendJob, WaypointSendRequestKind,
    WaypointSendSuccess, expires_at_from_now,
};

const ACCESS_TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(60);

pub(super) fn start_waypoint_send(jobs: Vec<WaypointSendJob>, sender: Sender<WaypointSendEvent>) {
    thread::spawn(move || {
        let concurrency = MAX_CONCURRENT_WAYPOINT_SENDS.max(1);
        let mut jobs = jobs.into_iter();

        loop {
            let mut handles = Vec::with_capacity(concurrency);

            for job in jobs.by_ref().take(concurrency) {
                let sender = sender.clone();
                handles.push(thread::spawn(move || {
                    let _ = sender.send(WaypointSendEvent::Started {
                        character_id: job.character_id,
                        character_name: job.character_name.clone(),
                    });

                    let result = run_waypoint_send_job(&job).map_err(|err| err.to_string());
                    let _ = sender.send(WaypointSendEvent::Finished {
                        character_id: job.character_id,
                        character_name: job.character_name,
                        destination_name: job.destination_name,
                        destination_id: job.destination_id,
                        result,
                    });
                }));
            }

            if handles.is_empty() {
                break;
            }

            for handle in handles {
                if handle.join().is_err() {
                    error!("Waypoint send worker panicked");
                }
            }
        }

        let _ = sender.send(WaypointSendEvent::BatchFinished);
    });
}

fn run_waypoint_send_job(job: &WaypointSendJob) -> Result<WaypointSendSuccess> {
    let (access_token, access_token_update) = access_token_for_job(job)?;

    match &job.kind {
        WaypointSendRequestKind::Single { options } => {
            waypoints::set_waypoint(&access_token, job.destination_id, *options)?;
        }
        WaypointSendRequestKind::OptimizedRoute {
            destinations,
            graph,
        } => {
            let origin = location::current_solar_system_id(&access_token, job.character_id)?;
            let route = graph.optimize_open_route(origin, destinations)?;
            for (index, destination) in route.destinations.iter().enumerate() {
                waypoints::set_waypoint(
                    &access_token,
                    destination.system_id,
                    waypoints::options_for_route_stop(index == 0),
                )?;
            }
        }
    }

    Ok(WaypointSendSuccess {
        access_token_update,
    })
}

fn access_token_for_job(job: &WaypointSendJob) -> Result<(String, Option<AccessTokenUpdate>)> {
    if access_token_is_fresh(&job.access_token, job.expires_at) {
        debug!(
            character_id = job.character_id,
            character_name = %job.character_name,
            "Using cached EVE SSO access token"
        );
        let access_token = job
            .access_token
            .clone()
            .ok_or_else(|| anyhow!("Character access token was unexpectedly missing"))?;
        return Ok((access_token, None));
    }

    info!(
        character_id = job.character_id,
        character_name = %job.character_name,
        "Refreshing access token before ESI request"
    );

    let refresh_token = job.token_store.load_refresh_token(job.character_id)?;
    let refreshed = sso::refresh_access_token(&job.sso_config, &refresh_token)?;

    if refreshed.character_id != job.character_id {
        bail!(
            "Refreshed token character mismatch: expected {}, got {}",
            job.character_id,
            refreshed.character_id
        );
    }

    if let Some(refresh_token) = &refreshed.refresh_token {
        job.token_store
            .save_refresh_token(job.character_id, refresh_token)?;
    }

    let expires_at = expires_at_from_now(refreshed.expires_in);
    job.token_store
        .save_access_token(job.character_id, &refreshed.access_token, expires_at)?;

    let access_token = refreshed.access_token.clone();
    let update = AccessTokenUpdate {
        access_token: refreshed.access_token,
        expires_at,
        scopes: refreshed.scopes,
    };

    Ok((access_token, Some(update)))
}

fn access_token_is_fresh(access_token: &Option<String>, expires_at: Option<SystemTime>) -> bool {
    access_token.is_some()
        && expires_at
            .is_some_and(|expires_at| expires_at > SystemTime::now() + ACCESS_TOKEN_REFRESH_BUFFER)
}
