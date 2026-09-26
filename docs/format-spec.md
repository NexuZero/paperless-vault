# Vault format spec v0.1 — Paperless Vault

Status: DRAFT v0.1 (T-102), awaiting Reviewer Gate R1 (T-103) together with `threat-model.md`.
Sources: `05_SCHEMA.md` §2–5, Brief v1.1 §7–8, `07_SECURITY_ENV_PLAN.md` §5 (SR-06…SR-10, SR-21).

This document pins exact byte layouts, sizes and encodings that `05_SCHEMA.md` describes at
design level. Anything here that is **newly decided** (not already fixed in `05_SCHEMA.md`) is
marked **[NEW v0.1]** so the reviewer can see what to scrutinize versus what carries over
unchanged.

## 0. Conventions

- All multi-byte integers are **little-endian**. **[NEW v0.1]** — `05_SCHEMA.md` names field
  widths (`u16`, `u64`) but not byte order; little-endian matches the native encoding on every
  target platform in `02_TRD.md` §1 (x86_64, ARM64 for later mobile), avoiding a swap on the
  hot path.
- All random values (salts, nonces, vault/object IDs) come from `randombytes_buf` (libsodium's
  CSPRNG), never from a non-cryptographic RNG.
- `crypto_kdf_derive_from_key` (libsodium) requires an **exactly 8-byte** context string. The five
  contexts already named in `05_SCHEMA.md` §3 (`PVhdrmac`, `PVdbwrap`, `PVdkwrap`, `PVdigest`,
  `PVbackup`) are each exactly 8 ASCII bytes — this is a hard constraint of the API, not a style
  choice, and the schema already respects it. Any future subkey purpose must also fit 8 bytes.
- AEAD wraps use `crypto_aead_xchacha20poly1305_ietf` (192-bit nonce, 128-bit/16-byte Poly1305
  tag): ciphertext length is always `plaintext_len + 16`. A wrapped 32-byte key therefore always
  serializes as 48 bytes — this is why `wrapped_root[48]` and `wrapped_db_key.ct[48]` in
  `05_SCHEMA.md` §4 are both 48, not a coincidence.
- `header_mac` uses `crypto_auth` (HMAC-SHA-512-256 in libsodium), which always produces a
  32-byte tag — matches `05_SCHEMA.md` §4's `header_mac | 32 bytes`.

## 1. On-disk layout

Unchanged from `05_SCHEMA.md` §2:

```
<VaultDir>/                         0700, owned by the current user (SR-13)
  vault.pvh                         header — the only readable bootstrap data (0600)
  catalog.pvdb                      SQLCipher catalog (0600)
  objects/<2 hex>/<32 hex>.pvo      one encrypted stream per file version or binary derivative (0600)
  staging/<32 hex>.part             uncommitted encrypted imports; cleaned on open
  .writer.lock                      flock() single-writer lock
```

`objects/<2 hex>/` is the first byte of the object's 128-bit ID, hex-encoded — a two-level
fan-out to keep any one directory from holding tens of thousands of entries at the 10,000-item
ceiling (SC-03).

## 2. Header (`vault.pvh`) — exact byte layout

| Offset | Field | Size | Notes |
|---|---|---|---|
| 0 | `magic` | 4 | ASCII `PVLT` |
| 4 | `format_version` | 2 | u16, = `1` for this spec |
| 6 | `cipher_suite` | 2 | u16, = `1` (XChaCha20-Poly1305 secretstream + Argon2id13 + BLAKE2b) |
| 8 | `vault_id` | 16 | random, bound into every wrap and every object stream |
| 24 | `generation` | 8 | u64, incremented on every committed change (SR-07) |
| 32 | `kdf_floor.opslimit` | 8 | u64, minimum accepted opslimit — see §4 |
| 40 | `kdf_floor.memlimit` | 8 | u64, minimum accepted memlimit (bytes) |
| 48 | `slot_count` | 1 | u8. **[NEW v0.1]** `05_SCHEMA.md` implies exactly 2 (password, recovery) but doesn't store a count; storing it explicitly lets a parser validate length without hardcoding "2", and costs one byte. v0.1 always writes `2`; a parser must still read this many, not assume 2, to avoid drifting from the byte-format contract as new slot kinds are considered later. |
| 49 | `slots[]` | `slot_count` × 106 | see slot layout below |
| 49 + 106×slot_count | `wrapped_db_key.nonce` | 24 | |
| +24 | `wrapped_db_key.ct` | 48 | wraps the 32-byte SQLCipher raw key + 16-byte tag |
| +48 | `chunk_size` | 4 | u32, = `65536` in format v1 |
| +4 | `max_object_size` | 8 | u64, checked before any decryption allocation (SR-09) |
| +8 | `padding_mode` | 1 | u8: `0` = off (default), `1` = Padmé buckets (SR-21) |
| +1 | `header_mac` | 32 | `crypto_auth` over every byte above (offset 0 up to here), keyed with the `"PVhdrmac"` subkey |

**Slot layout (106 bytes each):**

| Offset (within slot) | Field | Size | Notes |
|---|---|---|---|
| 0 | `slot_type` | 1 | u8: `0` = password, `1` = recovery |
| 1 | `alg` | 1 | u8: `0` = Argon2id v1.3 (only value defined in v1) |
| 2 | `opslimit` | 8 | u64 — this slot's actual KDF ops, ≥ `kdf_floor.opslimit` |
| 10 | `memlimit` | 8 | u64 — this slot's actual KDF mem, ≥ `kdf_floor.memlimit` |
| 18 | `salt` | 16 | Argon2id salt, random per slot |
| 34 | `nonce` | 24 | XChaCha20-Poly1305 nonce for this slot's wrap |
| 58 | `wrapped_root` | 48 | AEAD-wrapped 256-bit root key |

Total: 1+1+8+8+16+24+48 = **106 bytes**, confirmed by hand against `05_SCHEMA.md`'s
`{type, alg, opslimit, memlimit, salt[16], nonce[24], wrapped_root[48]}` field list.

**Validation order on open** (every length/count checked before any allocation, per
`PROJECT_RULES.md` #9): `magic` → `format_version` (refuse unknown-newer without writing, A22) →
`slot_count` bounds (reject 0 or anything absurdly large before reading that many slot records) →
`header_mac` (fail closed → read-only with a tamper warning) → generation-counter cross-check
against the catalog (§4 below).

## 3. Key hierarchy — KDF contexts (unchanged from `05_SCHEMA.md` §3)

```
password ──Argon2id v1.3──► KEK_pw ──AEAD wrap──┐
recovery key (256-bit random) ──Argon2id (lighter params)──► KEK_rk ──AEAD wrap──┤► slot 0 / slot 1
                                                                                  ▼
                                                ROOT KEY (256-bit random, created once)
                      crypto_kdf_derive_from_key(root, subkey_id, context)
      "PVhdrmac"    "PVdbwrap"     "PVdkwrap"      "PVdigest"      "PVbackup"
```

`subkey_id` is a small integer distinguishing which derivation under a given context (libsodium's
`crypto_kdf` takes both a context and a `u64` subkey id) — **[NEW v0.1]**: `subkey_id = 0` for all
five contexts in format v1, since each context is used exactly once per vault. This is reserved
for future multi-key scenarios (e.g. per-folder keys), not used yet.

Associated data on every AEAD wrap: `vault_id ‖ purpose_byte ‖ format_version`, where
`purpose_byte` is the slot's `slot_type` for slot wraps, a fixed `0xDB` for the db-key wrap.
Data-key wraps (per file version, §5) additionally bind `object_id ‖ version_no`.

## 4. Generation counter — crash-safe ordering (`I-003`, from `05_SCHEMA.md` §4)

1. Commit the catalog transaction with `generation = N+1` (in `vault_meta.generation`).
2. Write the header with `generation = N+1` to a temp file in the same directory, `fsync` it,
   `rename()` over `vault.pvh`, then `fsync` the directory fd.

On open:
- `catalog.generation == header.generation` → OK.
- `catalog.generation == header.generation + 1` → a crash happened between steps 1 and 2. Rewrite
  the header to match, log a repair event (`03_history/EVENT_LOG.md`-equivalent inside the app,
  not the brain).
- Anything else → **read-only**, surfaced as a tamper/corruption warning (A29).

**Open question for the reviewer (T-103):** confirm that tolerating exactly `+1` gives an
attacker nothing beyond the already-accepted "an old header still opens with an old password"
case (`05_SCHEMA.md` explicitly flags this as needing Gate R1 confirmation — not resolved here).

## 5. Recovery key — Base32 encoding and checksum **[NEW v0.1]**

`07_SECURITY_ENV_PLAN.md` §3 pins the shape ("exactly 52 Crockford Base32 characters plus a
checksum group") but not the checksum algorithm. Proposed for v0.1, reusing Crockford's own
published check-symbol scheme rather than inventing one:

- **Alphabet (32 symbols, data):** `0123456789ABCDEFGHJKMNPQRSTVWXYZ` — excludes `I`, `L`, `O`,
  `U` (Crockford's own exclusions, to avoid confusion with `1`/`1`/`0` and to avoid accidental
  words). Decoding is case-insensitive; `I`/`L` decode as `1`, `O` decodes as `0` (Crockford's
  documented tolerance for transcription slips).
- **Data payload:** the 256-bit (32-byte) recovery key, encoded as 52 Crockford Base32 symbols
  (52 × 5 = 260 bits ≥ 256; the top 4 bits of the last symbol are always zero-padded).
- **Checksum symbol:** one additional symbol from Crockford's extended 37-symbol check alphabet
  (the 32 data symbols plus `*`, `~`, `$`, `=`, and `U` — `U` is valid *only* as a check symbol,
  never as data). Computed as `(the 256-bit value interpreted as an integer) mod 37`, mapped
  through the 37-symbol alphabet. This catches the overwhelming majority of single-character
  transcription errors and adjacent-transposition errors, per Crockford's own analysis — it is
  not itself a cryptographic integrity check (that's `header_mac` and per-slot AEAD
  authentication; the checksum only helps a human catch a copying mistake before it reaches
  those).
- **Display grouping:** 4 groups of 13 characters (52 data symbols), e.g.
  `XXXXXXXXXXXXX-XXXXXXXXXXXXX-XXXXXXXXXXXXX-XXXXXXXXXXXXX`, with the checksum symbol appended
  after the last group, separated by a space: `... XXXXXXXXXXXXX C`.
- Checksum is verified **before** the value is passed to Argon2id — a failed checksum is reported
  immediately as "this doesn't look right, please re-check", not as a wrong-key error.

This is a proposal, not yet implemented or test-vectored (T-203/T-704 implement it). Flagged for
explicit reviewer sign-off since it's new at v0.1, not carried over from an already-approved doc.

## 6. Object file (`.pvo`) — format (unchanged from `05_SCHEMA.md` §5)

```
magic "PVOB" (4) · format_version u16 (2) · object_id (16) · version_no u32 (4) · secretstream header (24)
chunk_1 … chunk_n   (65,536 plaintext bytes each + 17 bytes auth tag; last chunk tagged FINAL)
```

Fixed header size: 4 + 2 + 16 + 4 + 24 = **50 bytes** before the first chunk. **[NEW v0.1]** —
`05_SCHEMA.md` lists the fields but not their running total; recorded here for parser bounds
checks (SR-09: validate `max_object_size` and expected chunk count from `byte_length` in the
catalog before decrypting, using this fixed 50-byte offset).

Every chunk's associated data is `vault_id ‖ object_id ‖ version_no` (SR-09) — this is
libsodium secretstream's own AD mechanism, not a separate MAC field per chunk.

## 7. Versioning and compatibility rule (A22)

- `format_version` is refused-if-unknown-and-newer, **without writing anything** — a vault from a
  future version of the app must never be silently mutated by an older build.
- A supported older format version opens and, on first write, is upgraded — the upgrade path
  itself is not yet specified (no version 2 exists yet at v0.1; deferred until a real format
  change is proposed).

## 8. Test vectors

Not yet produced — this needs a real `pv-format`/`pv-crypto` implementation (T-201+, gated behind
GATE-R1) to generate genuine encrypt/decrypt round-trips, not hand-computed values. `05_SCHEMA.md`
§10 already reserves `fixtures/` for this; property tests (`proptest`, per `02_TRD.md` §5) are the
plan for parser fuzzing once real code exists.
