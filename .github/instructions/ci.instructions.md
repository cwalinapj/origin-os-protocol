---
applyTo: ".github/workflows/**"
---

# CI Instructions

## Always
- Keep installs deterministic and cache-safe.
- If installing a tool that might already exist, remove it first or use `--force`.

## Rust/Cargo installs
- Prefer `cargo install ... --locked`.
- If install errors include "binary already exists in destination":
  - add a cleanup step: `rm -f ~/.cargo/bin/<binary>` OR add `--force`.

## Anchor CLI installs
- Prefer AVM-managed Anchor versions when used.
- If Anchor install fails due to an existing binary:
  - `rm -f ~/.cargo/bin/anchor` before installing/using the pinned version.

## MSRV policy
- If security tools (cargo-audit, etc.) require newer Rust than the pinned MSRV:
  - pin the tool version OR run it in a separate job using newer Rust.
  - do not change pinned toolchain unless explicitly requested.

## Net flakiness
- For curl-based downloads, add retries and verbose error output.
