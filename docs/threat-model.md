# Threat model — Paperless Vault

Status: DRAFT v0.1 (T-101), awaiting Reviewer Gate R1 (T-103).
Sources: Brief v1.1 §0 (SR-01…SR-23), §6, §7, §8, §12, §17; `07_SECURITY_ENV_PLAN.md` §5.

## 1. What we are protecting, and from whom

**Assets** (see `07_SECURITY_ENV_PLAN.md` §7 for the full inventory):
- Document contents (every stored version).
- Original names, folder paths, tags, notes, titles, issuers, dates.
- OCR text, extracted text, the search index and snippets.
- Thumbnails and searchable-PDF derivatives.
- The vault root key, the SQLCipher database key, and every per-version data key.
- The recovery key and the release signing key (outside the running app; see §1 of `07_SECURITY_ENV_PLAN.md`).

**Owner (P1, single user).** Trusted. Holds the password and the recovery key. The vault protects
the owner's own documents against everyone else, including other accounts on a shared machine and
anyone who later possesses the disk or a backup.

**Out of scope by design** (`01_PRD.md` §8): multi-user access, remote adversaries over a network
(v1 is fully offline — there is no network attack surface to defend), and defeating an adversary who
already controls the unlocked device at the OS/root level (see §3, "Malware controls an unlocked
device").

## 2. Adversary model — one row per Brief §6 situation

| # | Situation (attacker capability) | Required protection | Important limit — what we do NOT claim |
|---|---|---|---|
| 1 | Someone copies the locked vault (stolen laptop, stolen backup drive, cloned disk) | Content, catalog, indexes and derivatives are all encrypted; nothing readable exists on disk while locked (G1) | Object sizes, counts and filesystem activity timestamps may remain observable — SR-21 (optional padding, off by default) |
| 2 | A laptop or backup drive is lost | Vault keys stay wrapped by the owner's password or recovery key; losing the device ≠ losing the keys | An **already unlocked** device has strictly greater exposure than a locked one — locking promptly matters |
| 3 | An object is modified or truncated on disk (bit rot, tampering, a bad backup) | Authentication fails closed; the item is rejected, not silently served as valid | An entire older *valid* snapshot can still be replayed; local catalog rollback is caught by the generation counter (SR-07), not by object auth alone |
| 4 | The password is guessed offline (stolen vault, unlimited attempts) | Strong enforced passphrase (SR-05) + Argon2id costly KDF (SR-06) | Local login delays do nothing against offline guessing — the KDF cost is the only defense, so the memory/ops floor in SR-06 must never be weakened for convenience |
| 5 | An imported file is crafted to exploit a preview/OCR parser | Sandboxed parsers: bwrap + seccomp + Landlock, no network, no home directory, memfd/pipe input only, hard rlimits (SR-03) | Encryption of the vault does nothing to make an untrusted file safe to *parse* — the sandbox is the only control here, and if it fails to launch, OCR/preview must disable themselves rather than run unsandboxed |
| 6 | Malware or another process controls an already-unlocked device | Minimize key lifetime and plaintext exposure: zeroize on lock, non-dumpable process, no core dumps (SR-15, SR-16) | We explicitly do **not** claim to defeat a keylogger, a compromised kernel, or an admin-level attacker on an unlocked session — this is an accepted limit, not a gap to close later |
| 7 | Another program in an X11 session reads keystrokes/screen | Detect X11, warn clearly before the password is typed; Wayland is the reference session (SR-01) | Any X11 client can read keystrokes and screen contents by design of X11 itself — a warning is the only mitigation available to an app, and some Ubuntu configs fall back to X11 with certain GPU drivers (tracked in ENVIRONMENT.md) |
| 8 | Desktop services copy data out from under the vault (recent files, thumbnails, search indexing, notifications, clipboard) | Named control per channel: no GTK recent-files entries, no shared thumbnail cache writes, desktop-search export warning, generic window titles/notifications, clipboard auto-clear (SR-02) | Copies made by *other* apps after a deliberate export are outside vault control by definition — export is the trust boundary, and the UI must say so |
| 9 | The drive fails, or files are deleted (accidental or malicious) | Separate, versioned backups (T-801–T-806) and a restore that is actually tested (US-13, SC-08) | Encryption provides zero availability guarantee on its own — a same-disk-only backup does not survive disk failure (I-009); this is why real documents wait for GATE-R3, not just for backup code to exist |

## 3. Trust boundaries and data flow

```
Owner ──password/recovery key──► pv-core (holds keys in guarded memory while unlocked)
Flutter UI  ──flutter_rust_bridge──►  pv-core   (UI never reads vault files, never holds keys)
pv-core  ──pipes / memfd only──►  pv-sandbox (bwrap + seccomp + Landlock)  ──►  pv-worker
                                    (no keys, no network, no home directory)
pv-core  ──raw key──►  SQLCipher catalog (pv-catalog only opens it)
pv-core  ──libsodium──►  encrypted objects on the local filesystem (pv-crypto only calls libsodium)
```

Trust boundaries, each with its own control:
- **Owner ↔ pv-core**: the only place a password or recovery key ever exists in plaintext, and only
  transiently (zeroized after key derivation — §1 of `07_SECURITY_ENV_PLAN.md`).
- **UI ↔ pv-core**: advisory only. `pv-core` enforces `require_unlocked()` on every command
  cryptographically, not because the UI asked nicely (§4 of `07_SECURITY_ENV_PLAN.md`).
- **pv-core ↔ pv-worker**: the sandbox boundary. Workers receive descriptors, never paths, never
  keys, and cannot reach the network or the home directory (SR-03).
- **Vault ↔ outside world (export)**: the one deliberate, owner-initiated crossing out of vault
  protection. Everything past it (SR-02's desktop-leakage controls) is best-effort warning, not
  enforcement, because the file is no longer under our control once it's on disk unencrypted.

## 4. Security requirement matrix (SR-01…SR-23)

Full detail, "Built in"/"Verified by"/Gate columns: `07_SECURITY_ENV_PLAN.md` §5. Restated here with
the threat each one closes, for reviewer traceability at Gate R1:

| SR | Closes situation # (§2) | Residual risk after the control |
|---|---|---|
| SR-01 | 7 | X11 fallback on some GPU drivers; warning-only, not a technical block |
| SR-02 | 8 | Accessibility bus is a same-user-observable channel — accepted, see SR-22 |
| SR-03 | 5 | Sandbox launch itself depends on the AppArmor userns profile installing correctly (I-001); OCR/preview disable rather than run unsandboxed if it doesn't |
| SR-04 | 1, 2 | A photographed or memorized recovery key is the owner's own OPSEC, outside app control |
| SR-05 | 4 | Strength estimation (zxcvbn) is heuristic, not a proof of unguessability |
| SR-06 | 4 | Rewrap requires an unlock first — a vault whose *only* slot is already below floor and unreadable can't be upgraded without recovery-key access |
| SR-07 | 3 | Detects rollback; does not prevent replay of an entire valid older snapshot (accepted, see §2 row 3) |
| SR-08 | — (integrity aid) | Keyed digests prove nothing about *freshness*, only content identity — that's SR-07's job |
| SR-09 | — (binding) | Protects against cross-slot/cross-object substitution; does not protect against a valid object being *withheld* |
| SR-10 | 8 (search-specific) | None known — index never leaves SQLCipher |
| SR-11 | 6 (swap-specific) | **This machine currently fails the swap/hibernation check — see §5, I-002 and I-011.** The app's own control (memfd-only + startup warning) is unaffected; the OS-level gap is the owner's to close |
| SR-12 | — (display safety) | Sanitizing catches known bidi/control-character tricks; a genuinely novel Unicode spoofing technique could still confuse a human reader |
| SR-13 | 1 (at-rest perms) | Meaningless if the OS itself is multi-user-compromised; assumes a single trusted account |
| SR-14 | — (import safety) | descriptor-based no-follow closes the classic TOCTOU symlink swap; a kernel-level race is out of scope |
| SR-15 | 6 (lock discipline) | Zeroization reduces but cannot guarantee erasure from all copies the OS/allocator may have made (see SR-16, I-008) |
| SR-16 | 6 (memory scraping) | Non-dumpable blocks `/proc`-based and debugger-based reads by other same-user processes; does not stop a kernel-privileged attacker |
| SR-17 | — (deletion) | SQLCipher secure_delete + VACUUM is "crypto-erasure," not physical overwrite — explained to the owner, not hidden (accepted, Release gate) |
| SR-18 | — (supply chain) | Reduces but does not eliminate supply-chain risk; a compromised crates.io mirror before checksum-pinning would still be a gap (mitigated by vendoring + checksums) |
| SR-19 | — (no 2nd crypto impl) | Recovery CLI reusing the core crate means a core bug affects both paths equally — accepted trade-off vs. maintaining two implementations |
| SR-20 | Phase 3 | Not applicable to v1 (desktop-only); tracked for when mobile ships |
| SR-21 | 1 (traffic analysis) | Off by default — sizes/timestamps ARE observable in the default configuration; owner can opt into padding at a storage cost |
| SR-22 | — (tampered binary / a11y bus) | **Explicitly accepted, not mitigated**: if the installed binary itself is tampered with while the owner is away, or another same-user process rides the accessibility bus, the vault cannot defend against that from inside the app. Package-signature verification on install/update is the only control (SR-18), and it does not cover a machine already compromised before install |
| SR-23 | — (log hygiene) | Reduces information leakage through logs to IDs/codes; assumes the log storage itself isn't separately compromised |

## 5. Environment-dependent residual risks (from ENVIRONMENT.md, T-003)

These are facts about *this specific reference machine*, not gaps in the app design — but they
change how much the app's controls can actually deliver, so they belong in the threat model:

- **I-011 (new, High) — no full-disk encryption is present on the reference machine at all**
  (confirmed via `lsblk`: plain ext4/vfat/swap on `nvme0n1`, no `crypto_LUKS`). The vault's own
  encryption (G1) is unaffected — document content is still only ever readable through `pv-core`
  with the owner's key. But anything the *OS* writes outside the vault directory — swap, `/tmp`,
  crash/core dumps, hibernation image — is currently **unencrypted at the OS level**, so if the
  disk itself is lost or copied, those OS-level artifacts are exposed even though the vault
  contents are not. Owner has committed to enabling LUKS before GATE-R3 (real documents).
- **I-002 (Medium) — swap is confirmed unencrypted** (subset of I-011, `swapon --show` +
  `lsblk`). SR-11's memfd-only design means the app itself should never *choose* to swap OCR
  plaintext, but this depends on `MADV_DONTFORK`/no page-out guarantees the app can influence but
  not fully control on a general-purpose kernel — hence SR-11 also requires a startup
  swap-encryption check with a confirm-to-proceed warning, which becomes load-bearing here.
- **Hibernation status unconfirmed** — Ubuntu ships it disabled by default, and the reference
  machine's swap partition (14.9G) is smaller than installed RAM (~15Gi), which typically
  precludes a full-RAM hibernation image; not independently verified. If hibernation is later
  enabled, its image is subject to the same I-011/I-002 exposure.
- **GPU is Intel integrated (Iris Xe), no NVIDIA** — removes the specific "NVIDIA driver forces
  X11" branch of SR-01's limit on this machine; Wayland is confirmed active.

## 6. Explicitly accepted limits (do not re-raise these as new findings without new evidence)

1. The vault cannot defend against a compromised or malware-controlled *unlocked* session (§2 row 6).
2. The accessibility bus is a same-user-observable channel by OS design (SR-22).
3. A tampered installed binary, if it happens while the owner is away, can capture the next unlock
   (SR-22) — mitigated only by signature verification on install/update, never fully closed.
4. Preview plaintext passes through the Dart heap and cannot be reliably wiped from it (I-008).
5. Crypto-erasure (SQLCipher `secure_delete` + VACUUM) is not physical media overwrite (SR-17).
6. Object sizes and access timestamps are observable in a copied vault unless the owner opts into
   padding, which is off by default and costs storage (SR-21).

## 7. Open items for Gate R1 (not yet resolved by this draft)

- T-102 (vault format spec v0.1) must still specify the exact header layout, generation-counter
  protocol (I-003), and recovery-key encoding referenced but not detailed here.
- I-011/I-002 (no FDE, unencrypted swap) are environment facts, not app-format decisions — Gate R1
  reviews the *app's* threat model; whether the owner has closed I-011 by the time real documents
  are imported is checked at Gate R3, not R1.
