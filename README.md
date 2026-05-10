# Set Desto

Set Desto is a Rust desktop utility scaffolded with `eframe`/`egui` and the same basic Nix package/dev-shell structure used by EVE Preview Manager.

## Development

```bash
nix develop
cargo run
```

## EVE SSO

Register an application in the EVE Developers portal and add this exact callback URL:

```text
http://127.0.0.1:18421/callback
```

Then launch Set Desto with your client ID:

```bash
export SET_DESTO_EVE_CLIENT_ID="your-client-id"
cargo run
```

You can override the callback URL with `SET_DESTO_EVE_REDIRECT_URI`, but it must also be registered with EVE SSO and use a loopback address.

## Build

```bash
cargo build --release
nix build
```
