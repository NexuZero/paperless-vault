# vendor/libsodium/

Pinned, minisign-verified libsodium 1.0.22 source tarball. Verified at T-104 against libsodium's
own published Ed25519 key, fetched directly from `https://doc.libsodium.org/installation` on
2026-09-26 (not taken from memory or a cached copy):

```
RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3
```

Verification command used (`rsign2`, a minisign-compatible Rust implementation — no sudo needed
to install it, `cargo install rsign2`):

```
rsign verify -P RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3 \
    -x libsodium-1.0.22.tar.gz.minisig libsodium-1.0.22.tar.gz
# => Signature and comment signature verified
```

`libsodium-1.0.22.tar.gz.sha256` records the resulting SHA-256 for a second, offline check.

`LATEST.tar.gz` / `LATEST.tar.gz.minisig` are identical copies under the fixed filename
`libsodium-sys-stable`'s build script requires for `SODIUM_DIST_DIR` (see `spikes/sodium/`) — the
crate independently re-verifies the signature against the same public key before using it, so this
isn't "trust the copy," it's "the crate checks it too."

Built and self-tested from this exact tarball at T-104 (101/101 of libsodium's own unit tests
passed); the extracted/built tree itself is not committed here, only the pinned source archive.
