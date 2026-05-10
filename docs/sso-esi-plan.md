# EVE SSO and ESI Waypoint Plan

Set Desto is a desktop app, so the SSO implementation should use OAuth 2.0 Authorization Code with PKCE. That keeps the client secret out of the app while still giving us refresh tokens for repeat ESI calls.

## Current Source Layout

- `src/main.rs`: application entrypoint.
- `src/cli.rs`: command-line arguments.
- `src/logging.rs`: tracing setup.
- `src/app.rs`: eframe launch and repaint loop.
- `src/app_state.rs`: UI/application state and user actions.
- `src/app_constants.rs`: window and timing constants.
- `src/ui/`: egui panels and controls.

## Proposed Modules

- `src/eve/auth.rs`: EVE SSO metadata discovery, PKCE verifier/challenge generation, authorization URL building, callback validation, token exchange, token refresh, and token revocation.
- `src/eve/esi.rs`: shared ESI HTTP client, compatibility-date header, request retries, error-limit handling, and typed route helpers.
- `src/eve/waypoints.rs`: waypoint-specific service that calls the ESI autopilot waypoint endpoint for one or more characters.
- `src/storage/tokens.rs`: secure token storage keyed by character ID.
- `src/storage/config.rs`: non-secret app configuration such as client ID, redirect URI, character group definitions, and UI preferences.
- `src/domain/character.rs`: authenticated character identity, granted scopes, token status, and display state.
- `src/domain/destination.rs`: destination IDs and resolution state for systems, stations, and structures.

## Required SSO Shape

1. Register Set Desto in the EVE Developers portal.
2. Configure the app's client ID and exact redirect URI in Set Desto config.
3. Request the smallest initial scope set:
   - `esi-ui.write_waypoint.v1`
   - Add `esi-search.search_structures.v1` only if we support authenticated structure search.
   - Add `esi-universe.read_structures.v1` only if we need private structure details.
4. Generate `state`, `code_verifier`, and `code_challenge` per login attempt.
5. Open the EVE SSO authorize URL in the user's browser.
6. Receive the redirect, verify `state`, exchange the authorization code using `code_verifier`, and validate the access-token JWT.
7. Store the refresh token securely per character, and cache access tokens with their expiry.

## Token Storage

Refresh tokens should not live in a plain JSON config file. Prefer an OS keyring backend through a Rust keyring crate, with metadata in the app config and secrets stored under a service name like `set-desto`.

Store per character:

- Character ID.
- Character name.
- Granted scopes.
- Refresh token in the keyring.
- Access token and expiry in memory, optionally persisted only if there is a strong reason.

Support logout by deleting the keyring entry and calling the SSO revoke endpoint when possible.

## Waypoint Flow

1. User logs in each character they want Set Desto to control.
2. User selects a destination.
3. App resolves the destination to an ESI-compatible `destination_id`.
4. App refreshes access tokens as needed.
5. App calls ESI for each selected character:
   - `POST /latest/ui/autopilot/waypoint/`
   - query parameters: `destination_id`, `add_to_beginning`, `clear_other_waypoints`, `datasource=tranquility`
   - required scope: `esi-ui.write_waypoint.v1`
6. UI reports per-character success/failure instead of treating the batch as all-or-nothing.

## Implementation Order

1. Add dependencies: `serde`, `serde_json`, `reqwest`, `url`, `rand`, and PKCE helpers. Initial implementation uses `SET_DESTO_EVE_CLIENT_ID` and `SET_DESTO_EVE_REDIRECT_URI`, with `http://127.0.0.1:18421/callback` as the default redirect URI.
2. Add static config loading for client ID and redirect URI.
3. Build `eve::auth` with authorize URL generation, PKCE, callback handling, and token exchange.
4. Add a login panel and authenticated-character list.
5. Add secure token storage, token refresh, token revocation, and JWT/JWKS validation.
6. Add `eve::esi` client and the waypoint route.
7. Add destination resolution.
8. Add group selection and batch waypoint execution.

## References

- EVE SSO documentation: https://developers.eveonline.com/docs/services/sso/
- ESI overview: https://developers.eveonline.com/docs/services/esi/overview/
- ESI OpenAPI spec: https://esi.evetech.net/latest/swagger.json
