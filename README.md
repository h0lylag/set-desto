# Set Desto

Set Desto is a Rust desktop utility for EVE Online to set autopilot destinations across selected authenticated characters. It is built with `eframe`/`egui`, uses EVE SSO with PKCE, and stores OAuth tokens in the OS keyring.

## What It Does

- Adds EVE characters through browser-based EVE SSO.
- Sends an autopilot destination to selected authenticated characters.
- Resolves numeric ESI destination IDs, solar system names, NPC station names, and accessible player structures.
- Supports route modes for replacing the route, adding the next stop, or adding the final stop.
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
