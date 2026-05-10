use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{debug, info};
use url::Url;

const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:18421/callback";
const METADATA_URL: &str = "https://login.eveonline.com/.well-known/oauth-authorization-server";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const SCOPES: &[&str] = &["esi-ui.write_waypoint.v1"];

#[derive(Clone, Debug)]
pub struct SsoConfig {
    pub client_id: String,
    pub redirect_uri: String,
}

impl SsoConfig {
    pub fn from_env() -> Result<Self> {
        let client_id = std::env::var("SET_DESTO_EVE_CLIENT_ID")
            .context("Set SET_DESTO_EVE_CLIENT_ID to your EVE developer app client ID")?;

        let redirect_uri = std::env::var("SET_DESTO_EVE_REDIRECT_URI")
            .unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string());

        Ok(Self {
            client_id,
            redirect_uri,
        })
    }
}

#[derive(Debug)]
pub struct AuthenticatedCharacter {
    pub character_id: u64,
    pub character_name: String,
    pub scopes: Vec<String>,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

pub type LoginResult = Result<AuthenticatedCharacter, String>;

pub fn start_login(config: SsoConfig) -> Receiver<LoginResult> {
    let (sender, receiver) = mpsc::channel();

    debug!("Spawning EVE SSO login worker");
    thread::spawn(move || {
        let result = run_login(config).map_err(|err| err.to_string());
        let _ = sender.send(result);
    });

    receiver
}

fn run_login(config: SsoConfig) -> Result<AuthenticatedCharacter> {
    info!(redirect_uri = %config.redirect_uri, "Starting EVE SSO authorization code flow");
    let client = Client::builder()
        .user_agent(format!("set-desto/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build HTTP client")?;

    let metadata = fetch_metadata(&client)?;
    let redirect_url = Url::parse(&config.redirect_uri).context("Invalid EVE SSO redirect URI")?;
    let bind_addr = redirect_url
        .socket_addrs(|| Some(80))
        .context("Redirect URI must resolve to a local socket address")?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Redirect URI did not include a usable host and port"))?;
    if !bind_addr.ip().is_loopback() {
        bail!("EVE SSO redirect URI must use a loopback address");
    }

    let listener = TcpListener::bind(bind_addr)
        .with_context(|| format!("Failed to listen for EVE SSO callback on {bind_addr}"))?;
    listener
        .set_nonblocking(true)
        .context("Failed to configure callback listener")?;
    info!(bind_addr = %bind_addr, "Listening for EVE SSO callback");

    let state = random_url_token(32);
    let code_verifier = random_url_token(32);
    let code_challenge = pkce_challenge(&code_verifier);
    let authorize_url = build_authorize_url(&metadata, &config, &state, &code_challenge)?;

    info!(scope_count = SCOPES.len(), "Opening browser for EVE SSO");
    webbrowser::open(authorize_url.as_str()).context("Failed to open browser for EVE SSO")?;

    let callback = wait_for_callback(&listener, &state)?;
    info!("Received EVE SSO callback, exchanging authorization code");
    exchange_code(&client, &metadata, &config, &code_verifier, &callback.code)
}

fn fetch_metadata(client: &Client) -> Result<SsoMetadata> {
    debug!("Fetching EVE SSO metadata");
    client
        .get(METADATA_URL)
        .send()
        .context("Failed to fetch EVE SSO metadata")?
        .error_for_status()
        .context("EVE SSO metadata request failed")?
        .json()
        .context("Failed to parse EVE SSO metadata")
}

fn build_authorize_url(
    metadata: &SsoMetadata,
    config: &SsoConfig,
    state: &str,
    code_challenge: &str,
) -> Result<Url> {
    let mut url = Url::parse(&metadata.authorization_endpoint)
        .context("Invalid EVE SSO authorization endpoint")?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &SCOPES.join(" "))
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");

    Ok(url)
}

fn wait_for_callback(listener: &TcpListener, expected_state: &str) -> Result<AuthCallback> {
    let started = Instant::now();

    loop {
        if started.elapsed() > LOGIN_TIMEOUT {
            bail!("Timed out waiting for EVE SSO callback");
        }

        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let callback = read_callback(&mut stream, expected_state);
                let body = match &callback {
                    Ok(_) => "Set Desto login complete. You can close this tab.",
                    Err(_) => "Set Desto login failed. You can close this tab.",
                };
                let _ = write_http_response(&mut stream, body);
                return callback;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(err).context("Failed while waiting for EVE SSO callback"),
        }
    }
}

fn read_callback(stream: &mut impl Read, expected_state: &str) -> Result<AuthCallback> {
    let mut buffer = [0_u8; 8192];
    let byte_count = stream
        .read(&mut buffer)
        .context("Failed to read EVE SSO callback")?;
    let request = String::from_utf8_lossy(&buffer[..byte_count]);
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("Callback request was empty"))?;
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("Callback request did not include a URL"))?;
    let url = Url::parse(&format!("http://localhost{target}"))
        .context("Failed to parse EVE SSO callback URL")?;

    let mut code = None;
    let mut state = None;
    let mut error = None;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }

    if let Some(error) = error {
        bail!("EVE SSO returned an error: {error}");
    }

    let state = state.ok_or_else(|| anyhow!("EVE SSO callback did not include state"))?;
    if state != expected_state {
        bail!("EVE SSO callback state did not match");
    }

    let code = code.ok_or_else(|| anyhow!("EVE SSO callback did not include code"))?;
    Ok(AuthCallback { code })
}

fn write_http_response(stream: &mut impl Write, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

fn exchange_code(
    client: &Client,
    metadata: &SsoMetadata,
    config: &SsoConfig,
    code_verifier: &str,
    code: &str,
) -> Result<AuthenticatedCharacter> {
    let token_response: TokenResponse = client
        .post(&metadata.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &config.client_id),
            ("code_verifier", code_verifier),
        ])
        .send()
        .context("Failed to exchange EVE SSO authorization code")?
        .error_for_status()
        .context("EVE SSO token exchange failed")?
        .json()
        .context("Failed to parse EVE SSO token response")?;

    let claims = parse_access_token_claims(&token_response.access_token)?;
    let character_id = claims
        .sub
        .strip_prefix("CHARACTER:EVE:")
        .ok_or_else(|| anyhow!("EVE SSO token subject was not a character"))?
        .parse()
        .context("Failed to parse EVE character ID from SSO token")?;

    info!(
        character_id,
        character_name = %claims.name,
        scope_count = claims.scp.len(),
        expires_in = token_response.expires_in,
        "EVE SSO token exchange succeeded"
    );

    Ok(AuthenticatedCharacter {
        character_id,
        character_name: claims.name,
        scopes: claims.scp,
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_in: token_response.expires_in,
    })
}

fn parse_access_token_claims(access_token: &str) -> Result<AccessTokenClaims> {
    let payload = access_token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow!("EVE SSO access token was not a JWT"))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .context("Failed to decode EVE SSO access token payload")?;

    let claims: Value =
        serde_json::from_slice(&decoded).context("Failed to parse EVE SSO access token JSON")?;

    let sub = claims
        .get("sub")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("EVE SSO access token did not include a string sub claim"))?
        .to_string();

    let name = claims
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| character_name_from_subject(&sub));

    let scp = parse_scope_claim(claims.get("scp"))?;

    Ok(AccessTokenClaims { sub, name, scp })
}

fn parse_scope_claim(value: Option<&Value>) -> Result<Vec<String>> {
    match value {
        Some(Value::Array(scopes)) => scopes
            .iter()
            .map(|scope| {
                scope
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("EVE SSO scope array contained a non-string value"))
            })
            .collect(),
        Some(Value::String(scopes)) => Ok(scopes.split_whitespace().map(str::to_string).collect()),
        Some(_) => bail!("EVE SSO access token scp claim had an unexpected type"),
        None => Ok(Vec::new()),
    }
}

fn character_name_from_subject(subject: &str) -> String {
    subject
        .strip_prefix("CHARACTER:EVE:")
        .map(|character_id| format!("Character {character_id}"))
        .unwrap_or_else(|| "Unknown Character".to_string())
}

fn random_url_token(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[derive(Debug)]
struct AuthCallback {
    code: String,
}

#[derive(Debug, Deserialize)]
struct SsoMetadata {
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    name: String,
    #[serde(default)]
    scp: Vec<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_access_token_with_scope_array() {
        let token = unsigned_test_token(json!({
            "sub": "CHARACTER:EVE:2112625428",
            "name": "Test Pilot",
            "scp": ["esi-ui.write_waypoint.v1"]
        }));

        let claims = parse_access_token_claims(&token).expect("claims should parse");

        assert_eq!(claims.sub, "CHARACTER:EVE:2112625428");
        assert_eq!(claims.name, "Test Pilot");
        assert_eq!(claims.scp, vec!["esi-ui.write_waypoint.v1"]);
    }

    #[test]
    fn parses_access_token_with_scope_string() {
        let token = unsigned_test_token(json!({
            "sub": "CHARACTER:EVE:2112625428",
            "name": "Test Pilot",
            "scp": "esi-ui.write_waypoint.v1 esi-search.search_structures.v1"
        }));

        let claims = parse_access_token_claims(&token).expect("claims should parse");

        assert_eq!(
            claims.scp,
            vec![
                "esi-ui.write_waypoint.v1",
                "esi-search.search_structures.v1"
            ]
        );
    }

    #[test]
    fn falls_back_to_character_id_when_name_is_missing() {
        let token = unsigned_test_token(json!({
            "sub": "CHARACTER:EVE:2112625428",
            "scp": []
        }));

        let claims = parse_access_token_claims(&token).expect("claims should parse");

        assert_eq!(claims.name, "Character 2112625428");
    }

    fn unsigned_test_token(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.")
    }
}
