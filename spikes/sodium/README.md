# T-104 — libsodium spike: offline build + Argon2id timing

Status: DONE. Throwaway spike (this Cargo project is intentionally excluded from the workspace —
see the empty `[workspace]` table in its `Cargo.toml` — and never imported by `crates/`).

## 1. Verified source

`../../vendor/libsodium/` holds libsodium 1.0.22, minisign-verified against the key published at
`https://doc.libsodium.org/installation` (fetched fresh, not from memory). See that directory's
README for the exact verification command.

## 2. Offline build via `libsodium-sys-stable`

```
SODIUM_DIST_DIR="$(pwd)/../../vendor/libsodium" cargo build --offline
```

This crate's own `build.rs` re-verifies the archive's minisign signature against the *same*
published key before using it — independent of our own verification in `vendor/libsodium/`, so a
tampered `vendor/` copy would still be caught at build time. Confirmed working fully offline
(`cargo build --offline` succeeds) once `SODIUM_DIST_DIR` points at our pinned copy.

**Gotcha found:** the crate expects the files to be named exactly `LATEST.tar.gz` /
`LATEST.tar.gz.minisig` inside `SODIUM_DIST_DIR` regardless of the actual version pinned inside —
not `libsodium-1.0.22.tar.gz`. `vendor/libsodium/` keeps both names (the version-named files for
human/checksum traceability, `LATEST.*` as copies satisfying the crate's convention).

**Cross-check:** without `SODIUM_DIST_DIR` set, the crate builds anyway using its *own* bundled
`LATEST.tar.gz`, embedded in the crate package itself — which happened to also resolve to
libsodium 1.0.22, matching our independent pin. Good corroboration, but we still pin explicitly
via `SODIUM_DIST_DIR` for SR-18 (the project's own supply-chain control must not depend on trusting
a third-party crate's bundling choices).

Ran libsodium's own test suite against our exact pinned source: **101/101 passed**
(`make check` in a temp build directory — the project path itself contains a space, which trips
`libtool`'s configure check; see the D-019 path issue already logged. Built in `/tmp` instead,
using the same vendored+verified tarball).

## 3. Argon2id timing at the SR-06/ENVIRONMENT 256 MiB memory floor

Measured with `crypto_pwhash(..., crypto_pwhash_ALG_ARGON2ID13)` directly (libsodium's high-level
API fixes parallelism at 1 lane, matching what the app will actually call), on the reference
machine (8-core i7/Iris Xe laptop, single-threaded — Argon2id here doesn't parallelize across
cores by design of the high-level API):

| opslimit | seconds |
|---|---|
| 2 | 0.220 |
| 4 | 0.378 |
| 6 | 0.541 |
| 8 | 0.712 |
| 10 | 0.870 |
| 12 | 1.041 |
| 14 | 1.186 |
| 16 | 1.339 |
| 18 | 1.506 |
| **20** | **1.730** |
| 22 | 1.878 |
| 24 | 2.032 (over the 2 s ceiling) |

**Recommendation for SC-05 / T-104's acceptance ("opslimit giving 1–2 s at 256 MiB"):
`opslimit = 20`, `memlimit = 256 MiB`, giving ~1.73 s on the reference machine** — comfortably
inside the 1–2 s target with margin below the ceiling for a slower CPU. `opslimit = 22` (1.88 s)
is the alternate if more KDF cost is preferred over more margin.

This is a *recommendation*, not a final pin — `07_SECURITY_ENV_PLAN.md` §5 (SR-06) says these
parameters get set at T-202/T-212 and recorded in the versioned vault header, not hardcoded once
here. Recorded in `ENVIRONMENT.md` for now as the T-104 measurement.
