use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::{RequestBuilder, Response};
use reqwest::header::HeaderMap;
use tracing::{debug, warn};

const MAX_RATE_LIMIT_RETRIES: usize = 2;
const MAX_AUTO_WAIT: Duration = Duration::from_secs(5);
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(5);
const LOW_BUCKET_COOLDOWN: Duration = Duration::from_secs(1);
const ERROR_LIMIT_PAUSE_THRESHOLD: u64 = 5;
const RATE_LIMIT_WARN_THRESHOLD: u64 = 10;

static RATE_LIMIT_STATE: OnceLock<Mutex<RateLimitState>> = OnceLock::new();
static REQUEST_GATE: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) fn send(request: RequestBuilder, operation: &str) -> Result<Response> {
    // ESI now has both the older global error limit and newer per-route token buckets.
    // Keep outbound requests serialized so concurrent waypoint workers share one cooldown view.
    let _request_gate = request_gate()
        .lock()
        .expect("ESI request gate mutex poisoned");
    let retry_template = request.try_clone();
    let mut next_request = Some(request);

    for attempt in 0..=MAX_RATE_LIMIT_RETRIES {
        wait_for_recorded_cooldown(operation)?;
        let request = match next_request.take() {
            Some(request) => request,
            None => retry_template
                .as_ref()
                .and_then(RequestBuilder::try_clone)
                .ok_or_else(|| anyhow!("ESI request for {operation} could not be retried"))?,
        };

        let response = request
            .send()
            .with_context(|| format!("Failed to send ESI request for {operation}"))?;

        match rate_limit_decision(operation, &response)? {
            RateLimitDecision::ReturnResponse => return Ok(response),
            RateLimitDecision::RetryAfter(wait) if attempt < MAX_RATE_LIMIT_RETRIES => {
                wait_for_duration(wait, operation)?;
                next_request = None;
            }
            RateLimitDecision::RetryAfter(wait) => {
                bail!(
                    "ESI rate limit remained active after retries for {operation}; retry after {}s",
                    wait_seconds(wait)
                );
            }
        }
    }

    bail!("ESI request for {operation} exhausted rate-limit retries")
}

fn rate_limit_decision(operation: &str, response: &Response) -> Result<RateLimitDecision> {
    inspect_error_limit_headers(operation, response);
    inspect_bucket_limit_headers(operation, response);

    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        let wait = retry_after_duration(response.headers()).unwrap_or(DEFAULT_RETRY_AFTER);
        record_cooldown(wait, "ESI bucket rate limit");
        warn!(
            operation,
            retry_after_seconds = wait_seconds(wait),
            "ESI bucket rate limit returned 429"
        );
        return Ok(RateLimitDecision::RetryAfter(wait));
    }

    if status.as_u16() == 420 || response.headers().contains_key("X-Esi-Error-Limited") {
        let wait = error_limit_reset_duration(response.headers()).unwrap_or(DEFAULT_RETRY_AFTER);
        record_cooldown(wait, "ESI error limit");
        bail!(
            "ESI error limit reached while {operation}; retry after {}s",
            wait_seconds(wait)
        );
    }

    Ok(RateLimitDecision::ReturnResponse)
}

fn inspect_error_limit_headers(operation: &str, response: &Response) {
    let Some(remaining) = header_u64(response.headers(), "X-Esi-Error-Limit-Remain") else {
        return;
    };

    let reset = error_limit_reset_duration(response.headers());
    debug!(
        operation,
        error_limit_remaining = remaining,
        error_limit_reset_seconds = reset.map(wait_seconds),
        "Observed ESI error-limit headers"
    );

    if remaining <= ERROR_LIMIT_PAUSE_THRESHOLD {
        let wait = reset.unwrap_or(DEFAULT_RETRY_AFTER);
        record_cooldown(wait, "ESI error budget is low");
        warn!(
            operation,
            error_limit_remaining = remaining,
            reset_seconds = wait_seconds(wait),
            "ESI error budget is low; pausing future ESI requests"
        );
    }
}

fn inspect_bucket_limit_headers(operation: &str, response: &Response) {
    let Some(remaining) = header_u64(response.headers(), "X-Ratelimit-Remaining") else {
        return;
    };

    let group = header_string(response.headers(), "X-Ratelimit-Group");
    let limit = header_string(response.headers(), "X-Ratelimit-Limit");
    let used = header_u64(response.headers(), "X-Ratelimit-Used");

    debug!(
        operation,
        rate_limit_group = group.as_deref(),
        rate_limit = limit.as_deref(),
        rate_limit_remaining = remaining,
        rate_limit_used = used,
        "Observed ESI bucket rate-limit headers"
    );

    if remaining <= RATE_LIMIT_WARN_THRESHOLD {
        record_cooldown(LOW_BUCKET_COOLDOWN, "ESI bucket rate-limit budget is low");
        warn!(
            operation,
            rate_limit_group = group.as_deref(),
            rate_limit_remaining = remaining,
            "ESI bucket rate-limit budget is low"
        );
    }
}

fn wait_for_recorded_cooldown(operation: &str) -> Result<()> {
    if let Some(wait) = recorded_cooldown() {
        wait_for_duration(wait, operation)?;
    }

    Ok(())
}

fn wait_for_duration(wait: Duration, operation: &str) -> Result<()> {
    if wait.is_zero() {
        return Ok(());
    }

    if wait > MAX_AUTO_WAIT {
        bail!(
            "ESI rate limit cooldown active for {}s before {operation}; try again shortly",
            wait_seconds(wait)
        );
    }

    warn!(
        operation,
        wait_seconds = wait_seconds(wait),
        "Waiting for ESI rate-limit cooldown"
    );
    thread::sleep(wait);
    Ok(())
}

fn recorded_cooldown() -> Option<Duration> {
    let now = Instant::now();
    let mut state = rate_limit_state()
        .lock()
        .expect("ESI rate-limit state mutex poisoned");
    let cooldown_until = state.cooldown_until?;

    if cooldown_until <= now {
        state.cooldown_until = None;
        return None;
    }

    Some(cooldown_until.duration_since(now))
}

fn record_cooldown(wait: Duration, reason: &str) {
    if wait.is_zero() {
        return;
    }

    let cooldown_until = Instant::now() + wait;
    let mut state = rate_limit_state()
        .lock()
        .expect("ESI rate-limit state mutex poisoned");
    if state
        .cooldown_until
        .is_none_or(|existing| cooldown_until > existing)
    {
        state.cooldown_until = Some(cooldown_until);
    }

    warn!(
        reason,
        wait_seconds = wait_seconds(wait),
        "Recorded ESI rate-limit cooldown"
    );
}

fn rate_limit_state() -> &'static Mutex<RateLimitState> {
    RATE_LIMIT_STATE.get_or_init(|| Mutex::new(RateLimitState::default()))
}

fn request_gate() -> &'static Mutex<()> {
    REQUEST_GATE.get_or_init(|| Mutex::new(()))
}

fn retry_after_duration(headers: &HeaderMap) -> Option<Duration> {
    header_u64(headers, "Retry-After").map(Duration::from_secs)
}

fn error_limit_reset_duration(headers: &HeaderMap) -> Option<Duration> {
    header_u64(headers, "X-Esi-Error-Limit-Reset").map(Duration::from_secs)
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_string)
}

fn wait_seconds(duration: Duration) -> u64 {
    duration.as_secs().max(1)
}

#[derive(Debug, Default)]
struct RateLimitState {
    cooldown_until: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RateLimitDecision {
    ReturnResponse,
    RetryAfter(Duration),
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    #[test]
    fn parses_retry_after_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Retry-After", HeaderValue::from_static("7"));

        assert_eq!(retry_after_duration(&headers), Some(Duration::from_secs(7)));
    }

    #[test]
    fn parses_error_limit_reset_header() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Esi-Error-Limit-Reset", HeaderValue::from_static("43"));

        assert_eq!(
            error_limit_reset_duration(&headers),
            Some(Duration::from_secs(43))
        );
    }

    #[test]
    fn ignores_invalid_numeric_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Ratelimit-Remaining", HeaderValue::from_static("nope"));

        assert_eq!(header_u64(&headers, "X-Ratelimit-Remaining"), None);
    }
}
