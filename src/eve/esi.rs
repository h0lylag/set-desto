use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};

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
