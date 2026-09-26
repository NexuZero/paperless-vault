//! bwrap launch, seccomp, Landlock, rlimits, worker protocol. The only crate
//! allowed to launch workers.
//!
//! Scaffolding only. Production code is blocked until GATE-PLAN is approved.
//! When sandbox-launch `unsafe` lands, every block must carry a `// SAFETY:` comment.
#![forbid(unsafe_code)]
