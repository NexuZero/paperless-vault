//! Safe wrappers over libsodium; secret types. The only crate allowed to link libsodium.
//!
//! Scaffolding only. Production code is blocked until GATE-PLAN is approved
//! and until GATE-R1 (T-103) passes. `unsafe` is not yet in use; when the
//! libsodium FFI lands, every `unsafe` block must carry a `// SAFETY:` comment.
#![forbid(unsafe_code)]
