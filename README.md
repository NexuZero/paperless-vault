# Paperless Vault

Offline, encrypted document vault for Ubuntu 24.04.

- Project brain, plan and rules: `../paperless-vault-ai-brain/ai-brain/projects/paperless-vault/`
- AI builder contract: `CLAUDE.md` (imports the brain's contract)

Status: planning / repo scaffolding (T-002). No application code until the plan is approved (GATE-PLAN).

## One-time machine setup (needs sudo — run these yourself, the builder agent cannot)

```
sudo apt-get install -y libsodium-dev libsqlcipher-dev sqlcipher
sudo snap install flutter --classic
```

Then, from this directory:

```
flutter create --platforms=linux .   # generates app/ properly — do this before hand-editing app/
```

Rust is installed via rustup under this user account (no sudo needed) — see `rust-toolchain.toml`
for the pinned version. `just`, `cargo-deny` and `cargo-audit` install via `cargo install`, also
no sudo needed.
