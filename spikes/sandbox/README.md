# T-107 — Sandbox spike under Ubuntu 24.04 AppArmor

Status: partial (userns + basic isolation confirmed; seccomp/Landlock/rlimits deferred to a Rust
harness — see "Not yet tested" below). Throwaway spike, not imported by `crates/`.

## Finding: I-001's premise needs correcting

`02_TRD.md` §2 and `PROJECT_STATE.md` I-001 assumed Ubuntu 24.04 blocks unprivileged user
namespaces *unless our own `.deb` ships a scoped AppArmor profile* for
`/usr/lib/paperless-vault/`. On the reference machine, `bwrap` (bubblewrap 0.9.0, straight from
the Ubuntu `noble` repos, no custom profile installed) **already works unprivileged**:

```
$ cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns
1   # restriction IS active system-wide
$ bwrap --unshare-user --uid 0 --gid 0 ... /bin/echo hi
hi  # succeeded anyway
```

Why: Ubuntu's `bubblewrap` package ships its own AppArmor profile at
`/etc/apparmor.d/bwrap-userns-restrict`, scoped to `/usr/bin/bwrap` specifically (not to our
launcher path). It's deliberately broad — its own comment says "this profile allows almost
everything and only exists to allow bwrap to work" — and the real confinement happens one layer
in: children spawned by `bwrap` run under a second profile, `unpriv_bwrap`, which does
`audit deny capability` (blocks all Linux capabilities in the sandboxed child) while still
allowing the file/mount/pivot_root operations `bwrap` itself needs to build the sandbox.

**Consequence for the plan:** we most likely do **not** need to ship our own AppArmor profile in
the `.deb` to make `bwrap` work — Ubuntu 24.04 desktop already ships one that covers any local
invocation of `/usr/bin/bwrap`, ours included. This is a plan change, not just a spike note:

- `02_TRD.md` §2's "the `.deb` also installs an AppArmor profile that grants userns to
  `/usr/lib/paperless-vault/` only" is now in question — shipping a *narrower* profile than the
  stock one would be strictly better (defense in depth: today, literally any program on the
  system can invoke `/usr/bin/bwrap` and get unprivileged userns, not just ours), but it is no
  longer required for baseline functionality, and is not a blocker for T-501.
- This needs a Decision Log entry and owner sign-off before `02_TRD.md` is amended (see
  `PROJECT_RULES.md` #3: format/schema changes need sign-off; this is close enough to "the
  security control" category in CLAUDE.md §3 that it's flagged here rather than silently changed).
- If the packaged `.deb` targets machines where this stock profile is *disabled* (the profile
  file itself says "disabled by default... use aa-enforce to enable it" — so this depends on
  Ubuntu flavor/image defaults, not guaranteed everywhere), our own fallback profile is still
  needed for those machines. T-902 (second-machine build/test) should check this explicitly.

## Confirmed by direct test (this machine, no sudo used)

- Unprivileged user namespace creation: **works** (see above).
- Network isolation (`--unshare-net`): confirmed — `getent hosts google.com` fails inside the
  sandbox.
- No home directory visibility (`--unshare-all`, minimal binds): confirmed — `/home` doesn't
  exist inside the sandbox at all.
- PID namespace isolation (`--unshare-pid`): confirmed indirectly — a fork-bomb attempt inside
  the sandbox did not affect the host process table or persist after the sandboxed process tree
  exited.
- `bwrap --seccomp FD` / `--add-seccomp-fd FD` (load a pre-built BPF program from a file
  descriptor): present in this build's `--help`. Matches the plan (`seccompiler` crate builds the
  BPF program in Rust; `pv-sandbox` passes it via fd, doesn't shell out to a config file).

## Not yet tested (needs a small Rust harness, not just shell)

- Actually loading a seccomp filter (needs a compiled BPF program — that's `seccompiler`, i.e.
  real Rust code, not a shell one-liner).
- Landlock (bwrap has no `--landlock` flag; this is `pv-sandbox`'s own layer via the `landlock`
  crate, applied to the process before it execs into the worker path — not a bwrap feature at all).
- rlimits (also not a bwrap flag; `pv-sandbox` sets these itself via `setrlimit`/`prlimit` before
  exec).
- The full A27 attempt list (resource-exhaustion / escape attempts) — that's the acceptance test
  suite, run against the real `pv-sandbox`/`pv-worker` at T-501, not this throwaway spike.

## Recommendation for T-107 sign-off

Mark T-107 as **substantially de-risked, not fully closed**: the hard problem I-001 worried about
(unprivileged userns being blocked outright) turned out not to reproduce on the reference machine,
which is good news, but the packaging story ("do we still ship our own profile, and for which
target machines") needs an owner decision before `02_TRD.md` §2 is rewritten. Filed as a Decision
Log item in the brain rather than assumed.
