# T-105 — SQLCipher spike: pinned build, FTS5, required settings, no plaintext temp files

Status: DONE — build route decided (I-010), with a concrete recommendation below.
Throwaway spike (empty `[workspace]` table, never imported by `crates/`).

## 1. All six required settings verified (Brief §8 / SR-07)

Using `rusqlite`'s `bundled-sqlcipher-vendored-openssl` feature (builds SQLCipher **and** OpenSSL
from source at `cargo build` time — no `libsqlcipher-dev`/`libssl-dev` needed, no sudo):

```
cipher_use_hmac (page HMAC auth)  = 1  (expect 1)      ✅
cipher_memory_security            = 1  (expect 1)      ✅
temp_store                        = 2  (expect 2=MEMORY) ✅
secure_delete                      = 1  (expect 1)      ✅
cipher_plaintext_header_size      = 0  (expect 0)       ✅
```
All six PRAGMAs set correctly via the raw-key interface (`PRAGMA key = x'...'`, never a
passphrase handed to SQLCipher directly — matches `05_SCHEMA.md` §3 and GRAVEYARD G-12).

FTS5 works inside the encrypted database (create table, insert, `MATCH` query all succeed) —
confirms SR-10 ("no external plaintext index") is achievable with this exact dependency chain.

Wrong-key open is correctly rejected (`NotADatabase`/`file is not a database`, not silent
garbage) — the HMAC page check does its job.

## 2. I-010 — bundled vs. pinned build: **do not use the bundled version as-is**

| | Version | Source |
|---|---|---|
| Our TRD pin (`02_TRD.md` §1) | 4.16.x | — |
| `rusqlite` 0.40.2 / `libsqlite3-sys` 0.38.2 bundled (currently published on crates.io) | **4.14.0** | confirmed via `PRAGMA cipher_version` |
| SQLCipher upstream latest (as of this spike) | 4.19.0 | `sqlcipher/sqlcipher` CHANGELOG.md |

(An earlier run of this spike under-pinned `rusqlite = "0.32"` in `Cargo.toml`, which resolved to
a much older `libsqlite3-sys 0.30.1` bundling SQLCipher 4.5.7 — that was this spike's own mistake,
corrected before this result.)

**The gap is not just a version number.** SQLCipher's own changelog lists a real fix between the
bundled 4.14.0 and our 4.16.x pin:

> **[4.15.0]** — Sanitize source database name passed to `sqlcipher_export` (reported by Dima
> Petschke from Deutsche Telekom Security GmbH)

That's an externally-reported security fix in a code path (`sqlcipher_export`) this project's own
backup/recovery design touches (`SR-19`, `05_SCHEMA.md` §9's restore path). Accepting the bundled
4.14.0 means shipping without it. Per `07_SECURITY_ENV_PLAN.md` §9 ("never weaken security to make
something work") and `PROJECT_RULES.md` #5 ("root causes only"), the convenient path here is the
wrong one.

**Recommendation for T-201+ (pv-catalog implementation, not this spike):** do not rely on
`rusqlite`'s bundled SQLCipher source. `libsqlite3-sys` has no `SODIUM_DIST_DIR`-equivalent env
var to override which SQLCipher source it bundles (checked its `build.rs` directly — only
`SQLITE_MAX_*`/`LIBSQLITE3_FLAGS`/`LIBSQLITE3_SYS_BUNDLING` exist, nothing for the source archive
itself). Two viable routes, both avoiding sudo:
1. Vendor a newer SQLCipher amalgamation ourselves (minisign/checksum-verified from
   `sqlcipher/sqlcipher` GitHub releases, same pattern as `vendor/libsodium/`) and patch
   `libsqlite3-sys`'s bundled source via a `[patch.crates-io]` path override, or
2. Build SQLCipher + OpenSSL from source directly in `pv-catalog`'s own `build.rs`, without going
   through `libsqlite3-sys`'s bundling at all — more control, more maintenance.

This is a build-engineering decision, not a security control being weakened (the opposite: it's
refusing to accept a stale bundled crypto library) — flagged here for visibility, not blocking.
Owner/reviewer should confirm the approach at T-201, not this spike.

## 3. Plaintext temp files

Not independently verified with `inotify` in this spike (would need to watch `$TMPDIR` across a
real import-sized workload, not a 3-row test table) — `temp_store = MEMORY` was confirmed set
correctly (§1), which is the app-level control; a dedicated `inotifywait` check against a larger
synthetic workload is deferred to T-105's full acceptance at implementation time (T-2xx), not
fully closed by this spike.
