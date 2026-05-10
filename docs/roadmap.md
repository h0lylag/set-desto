# Set Desto Roadmap

This is the remaining implementation plan from the current SSO proof of life to a useful cross-platform waypoint tool.

## Current State

- Egui app scaffold is in place.
- Manual and Clipboard destination modes exist, with Clipboard still a stub.
- EVE SSO Authorization Code with PKCE works through a loopback callback.
- Add Character can authenticate a character, save non-secret metadata to config, and persist tokens in the OS keyring.
- Access tokens are cached with an absolute expiry timestamp and reused until they are close to expiry.
- Refresh tokens are persisted through the OS keyring and used to renew stale access tokens before ESI calls.
- ESI waypoint calls are wired for numeric destination IDs and exact solar system/station names through ESI `/universe/ids/`.
- Characters can be selected individually, and waypoint requests only target selected characters.

## 1. Cross-Platform Storage

Goal: persist characters safely across restarts without storing refresh tokens in plaintext.

- `storage::config` exists for non-secret app data.
- `storage::tokens` exists for OS keyring access.
- Use a cross-platform config directory helper such as `directories`.
- Use the Rust `keyring` crate for refresh tokens:
  - Linux: Secret Service / GNOME Keyring / KWallet.
  - Windows: Windows Credential Manager.
- Store non-secret metadata in config:
  - character ID
  - character name
  - granted scopes
  - selected state
  - token/keyring entry identifier
- Refresh tokens are stored only in the OS keyring.
- Access tokens are cached in the OS keyring with expiry metadata so restarts do not force a refresh when the token is still valid.
- Add remove/logout behavior:
  - delete character metadata
  - delete keyring refresh token
  - optionally call EVE SSO revoke endpoint

## 2. Token Manager

Goal: make ESI callers ask for an access token without caring whether refresh is needed.

- Add `eve::tokens` or `eve::auth::TokenManager`.
- Track in-memory access token expiry with a safety buffer.
- Implement:
  - `access_token_for(character_id)`
  - `refresh_character(character_id)`
  - `save_login(character, token_response)`
  - `remove_character(character_id)`
- Refresh automatically when:
  - no access token exists
  - token is expired
  - token is close to expiry
- If EVE returns a rotated refresh token, replace the keyring value.
- Surface token failures per character in the UI.

## 3. ESI Client

Goal: centralize HTTP behavior and headers for all ESI calls.

- `eve::esi` exists for shared ESI HTTP setup.
- Use `reqwest` with a shared user agent.
- ESI base URL, datasource, language, and compatibility-date headers are centralized.
- Add request helpers for:
  - authenticated requests
  - JSON parsing
  - empty `204 No Content` responses
  - ESI error-limit headers
  - readable error messages
- Ensure all authenticated calls go through `TokenManager`.

## 4. Waypoints

Goal: set a destination for one or more authenticated characters.

- Add `eve::waypoints`.
- Implement:
  - `set_waypoint(character_id, destination_id, add_to_beginning, clear_other_waypoints)`
  - `set_waypoints_for_group(group, destination, options)`
- ESI endpoint:
  - `POST /latest/ui/autopilot/waypoint/`
  - required scope: `esi-ui.write_waypoint.v1`
  - query params:
    - `destination_id`
    - `add_to_beginning`
    - `clear_other_waypoints`
    - `datasource=tranquility`
- Report per-character success/failure.

## 5. Destination Resolution

Goal: turn user input into an ESI `destination_id`.

- `domain::destination` exists.
- Direct numeric IDs are supported.
- Exact solar system and station name lookup use ESI `/universe/ids/`.
- Add structure support after the basic flow works.
- Decide whether to use:
  - ESI search
  - bundled/static SDE data
  - both, with SDE for public locations and ESI for private structures
- Add clear UI feedback when a name resolves to multiple possible destinations.

## 6. Character Groups

Goal: let users set waypoints for selected groups of characters.

- Add `domain::character`.
- Add group metadata to config.
- UI:
  - authenticated character list exists
  - selected/unselected state exists
  - group creation/editing
  - batch result status
- Keep groups as non-secret config data.

## 7. UI Polish

Goal: keep the app simple but operational.

- Show login state and token status without exposing token values.
- Add Remove Character.
- Add Retry Failed.
- Disable Set Destination until:
  - a destination is present
  - at least one character is selected
  - destination has resolved
- Keep Clipboard as a stub until destination parsing is ready.
- Add friendly errors for:
  - redirect URI mismatch
  - port already in use
  - missing client ID
  - missing scope
  - keyring unavailable

## 8. Cross-Platform Packaging

Goal: Linux and Windows builds should both work from the same codebase.

- Keep OS-specific behavior behind small storage/platform modules.
- For Linux/Nix:
  - include DBus/Secret Service dependencies required by keyring.
  - keep `nix build` passing.
- For Windows:
  - add a Windows build check in CI.
  - verify browser launch and loopback callback.
  - verify Windows Credential Manager storage.
- Consider app metadata/installers later:
  - Linux desktop file/metainfo already exists.
  - Windows icon/resources and installer are still future work.

## 9. Verification Bar

Before stopping work on implementation changes, run:

```bash
nix develop --command cargo fmt -- --check
nix develop --command cargo test
nix develop --command cargo clippy -- -D warnings
nix build
```

When adding files used by the Rust crate, make sure they are tracked by git before `nix build`, because flake builds do not include untracked files.

## Suggested Next Step

Add Remove Character/logout so stale pilots and keyring entries can be cleaned up from the app.
