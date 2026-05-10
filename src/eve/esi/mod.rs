use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue};

mod rate_limit;

pub const DATASOURCE: &str = "tranquility";
pub const ESI_BASE_URL: &str = "https://esi.evetech.net/latest";
pub const LANGUAGE: &str = "en";

const COMPATIBILITY_DATE: &str = "2026-05-10";

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(user_agent())
        .default_headers(default_headers())
        .build()
        .context("Failed to build ESI HTTP client")
}

pub fn error_body(response: Response) -> String {
    response
        .text()
        .unwrap_or_else(|_| "failed to read ESI error body".to_string())
}

// Keep ESI transport concerns in one place: user agent, compatibility date,
// rate-limit cooldowns, and consistent status errors.
pub fn send_request(request: RequestBuilder, operation: &str) -> Result<Response> {
    rate_limit::send(request, operation)
}

pub fn require_status(
    response: Response,
    expected_status: StatusCode,
    operation: &str,
) -> Result<Response> {
    let status = response.status();
    if status == expected_status {
        return Ok(response);
    }

    bail!(
        "ESI {operation} failed ({status}): {}",
        error_body(response)
    )
}

fn user_agent() -> String {
    format!("set-desto/{}", env!("CARGO_PKG_VERSION"))
}

fn default_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "X-Compatibility-Date",
        HeaderValue::from_static(COMPATIBILITY_DATE),
    );
    headers
}
