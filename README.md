# Set Desto

Set Desto is a Rust desktop utility for EVE Online pilots who want to set autopilot destinations across selected authenticated characters. It is built with `eframe`/`egui`, uses EVE SSO with PKCE, and stores OAuth tokens in the OS keyring.

## What It Does

- Adds EVE characters through browser-based EVE SSO.
- Sends an autopilot destination to selected authenticated characters.
- Resolves numeric ESI destination IDs, solar system names, NPC station names, and accessible player structures.
- Supports route modes for replacing the route, adding the next stop, or adding the final stop.
- Tracks per-character send progress, failures, and retryable failed sends.
- Stores favorite destinations with optional nicknames.

## What It Does Not Do

- It does not automate piloting, undocking, movement, or gameplay input.
- It does not store refresh tokens in the JSON config file.
- It does not ship prebuilt release binaries yet. For v0.0.1, source builds and the Linux Nix package are the supported paths.

## Platform Support

- Linux: primary supported platform for v0.0.1. The Nix package installs the binary, desktop entry, icon, and AppStream metadata.
- Windows: source builds are supported. The app uses Windows Credential Manager for tokens and embeds the app icon when a Windows resource compiler is available.
- macOS: not currently packaged or tested.

## EVE SSO Setup

Register an application in the EVE Developers portal and add this exact callback URL:

```text
http://127.0.0.1:18421/callback
```

Set Desto requests these EVE SSO scopes:

- `esi-ui.write_waypoint.v1`
- `esi-search.search_structures.v1`
- `esi-universe.read_structures.v1`

The waypoint scope is required to set autopilot destinations. The structure scopes allow Set Desto to resolve accessible player-owned structures by ID or name with a selected character that has access.

You can override the callback URL with `SET_DESTO_EVE_REDIRECT_URI`, but the value must also be registered with EVE SSO and must use a loopback address.

## Development

```bash
nix develop
cargo run
```

Open the ESI tab and save your EVE application Client ID. The tab shows the redirect URI to register with EVE SSO and includes a copy button.

Character metadata is stored in the platform config directory. Access and refresh tokens are stored in the OS keyring: Secret Service on Linux and Windows Credential Manager on Windows. Access tokens are reused until they are close to expiry, then refreshed with the saved refresh token.

## Build

With Nix:

```bash
nix build
```

With Cargo:

```bash
cargo build --release
```

Linux Cargo builds require the native libraries used by `eframe`, X11, OpenGL, DBus, and Secret Service.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
nix build
```

## Troubleshooting

- If login fails before the browser opens, confirm that the Client ID is saved in the ESI tab.
- If the browser login finishes but Set Desto does not receive it, confirm that `http://127.0.0.1:18421/callback` is registered in the EVE Developers portal and that no other process is using port `18421`.
- If token storage fails on Linux, confirm that a Secret Service provider such as GNOME Keyring or KWallet is installed and unlocked.
- If a player structure cannot be resolved, re-authenticate at least one selected character with the structure scopes and confirm that the character has access to the structure.

## Release Checklist

Before tagging a release:

- Run `cargo fmt --check`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run `cargo test`.
- Run `cargo build --release`.
- Run `nix build`.
- Perform manual EVE SSO and waypoint QA with a real EVE test setup.
