# T-110 — end-to-end prototype spike

Status: DONE, all acceptance checks pass — **plus one important new finding that needs follow-up
before finalizing KDF parameters (see §2).**

## 1. Full round trip — all passed

```
=== T-110 end-to-end prototype ===
unlock (Argon2id opslimit=20 @ 256MiB): 6.982s
generating 1 GiB source file...
encrypted 1 GiB in 13.23s
search 'lighthouse' -> "Invoice A-2026-0913" (expect the invoice item)
closed. root key zeroed. scratch object/catalog files remain on disk (as ciphertext).
reopened offline: 2 items visible (expect 2)
export digest matches source digest: true
byte-for-byte file comparison: true
wrong password correctly rejected: authentication failed decrypting a chunk (rc=-1)
corruption correctly detected: authentication failed decrypting a chunk (rc=-1)
=== ALL CHECKS PASSED ===
```

Create → import (a small "scan" item + a 1 GiB file) → search → close (zero the root key) →
reopen fully offline (re-derive everything from the password + stored salt, nothing cached) →
export the 1 GiB object back out → byte-identical (keyed-BLAKE2b digest match **and** a direct
byte-by-byte comparison) → wrong password rejected → a flipped byte mid-object detected. Every
crypto primitive uses the exact contexts from `docs/format-spec.md` §3
(`PVdbwrap`/`PVdkwrap`/`PVdigest`) and the exact object-file layout from §6.

**Scope boundary (by design, not an oversight):** this does not implement `vault.pvh`'s exact
byte layout (uses a throwaway sidecar for salt/params instead — that's real `pv-format` work,
gated behind GATE-R1) and does not integrate PDFium/OCR (T-108 already proved OCR works
separately; wiring OCR output into the catalog is future work).

## 2. Important finding: Argon2id is ~4-5x slower through `libsodium-sys-stable`'s own build

T-104 measured opslimit=20 @ 256 MiB at **1.73s** using a hand-built libsodium (`./configure &&
make`, linked into a small C harness). This spike measured the **same** libsodium 1.0.22, same
parameters, same machine, but built by `libsodium-sys-stable`'s own `build.rs` — and got
**6.98s–8.95s** across several runs (opslimit=12 also went from 1.04s to 4.1–5.6s).

Investigated so far:
- Not a fluke — reproduced 3 times, consistent 4-5x gap.
- Not system load — checked `uptime` (load average 2.4 on 8 cores) and running processes at the
  time; not enough to explain a 4-5x gap on a CPU-bound single-threaded operation.
- Likely cause, not yet confirmed: `libsodium-sys-stable`'s `build.rs` derives `CFLAGS` from
  Cargo's own `cc` crate (`cc::Build::new().get_compiler().cflags_env()`) rather than
  libsodium's own `./configure` defaults, and/or doesn't compile in (or the linker doesn't
  select) the AVX2 Argon2/BLAKE2b implementation the same way a plain `./configure && make`
  build does. Direct confirmation (checking which object files/symbols got linked into this
  build's output) was inconclusive in the time available — `target/release/build/.../out`
  didn't retain the intermediate static library to inspect after the fact.

**This matters for real work, not just this spike:** if `pv-crypto` ships using
`libsodium-sys-stable`'s default build (the obvious choice — it's already the TRD-pinned crate),
the actual unlock time could be 4-5x worse than the T-104 number that fed into the SC-05
recommendation. **T-201 must re-measure Argon2id timing against whatever the real production
build pipeline actually produces**, not carry forward T-104's number unchecked. Recorded as a new
open item rather than silently trusted.
