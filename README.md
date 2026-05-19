# Set Desto

Set Desto is a Rust desktop utility for EVE Online pilots who want to set autopilot destinations across selected authenticated characters. It is built with `eframe`/`egui`, uses EVE SSO with PKCE, and stores OAuth tokens in the OS keyring.

Linux builds enable both X11 and Wayland windowing support.

## What It Does

- Adds EVE characters through browser-based EVE SSO.
- Sends an autopilot destination to selected authenticated characters.
- Resolves numeric ESI destination IDs, solar system names, NPC station names, and accessible player structures.
- Supports route modes for replacing the route, adding the next stop, or adding the final stop.
- Imports FOBScout exports and sets locally optimized shortest-jump routes from each selected character's current system.
- Tracks per-character send progress, failures, and retryable failed sends.
- Stores favorite destinations with optional nicknames.

## What It Does Not Do

- It does not automate piloting, undocking, movement, or gameplay input.

## EVE SSO Setup

Register an application in the EVE Developers portal and add this exact callback URL:

```text
http://127.0.0.1:18421/callback
```

Set Desto requests these EVE SSO scopes:

- `esi-ui.write_waypoint.v1`
- `esi-location.read_location.v1`
- `esi-search.search_structures.v1`
- `esi-universe.read_structures.v1`

The waypoint scope is required to set autopilot destinations. The location scope allows Set Desto to optimize imported FOBScout routes from each selected character's current solar system. The structure scopes allow Set Desto to resolve accessible player-owned structures by ID or name with a selected character that has access.

## SDE Route Cache

FOBScout import routing uses a local shortest-jump stargate graph derived from CCP's JSONL Static Data Export. Set Desto loads any cached graph at startup and refreshes it in the background. If the first download has not completed yet, the import modal remains available but optimized route sending is disabled until the graph is ready.

You can override the callback URL with `SET_DESTO_EVE_REDIRECT_URI`, but the value must also be registered with EVE SSO and must use a loopback address.

## Troubleshooting

- If login fails before the browser opens, confirm that the Client ID is saved in the ESI tab.
- If the browser login finishes but Set Desto does not receive it, confirm that `http://127.0.0.1:18421/callback` is registered in the EVE Developers portal and that no other process is using port `18421`.
- If token storage fails on Linux, confirm that a Secret Service provider such as GNOME Keyring or KWallet is installed and unlocked.
- If a player structure cannot be resolved, re-authenticate at least one selected character with the structure scopes and confirm that the character has access to the structure.
- If optimized imports say a character needs re-authentication, add that character again so the saved token includes `esi-location.read_location.v1`.
- If optimized imports are disabled because the SDE route graph is unavailable, check network access to `developers.eveonline.com` and restart Set Desto to retry the background refresh.
