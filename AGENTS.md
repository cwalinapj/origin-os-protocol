# AGENTS.md

## Mission
Get CI green with minimal diffs. Read failing logs first; fix the FIRST root cause; repeat until all checks pass.

## Guardrails
- Do not upgrade pinned toolchains unless explicitly asked.
- One logical change per commit.
- Prefer deterministic installs (`--locked`, pinned versions, lockfiles).

## CI Autopatches
- "binary already exists in destination" -> remove existing binary OR install with `--force`.
- rustfmt failures -> `cargo fmt --all`, remove trailing whitespace.
- cargo-audit MSRV mismatch -> pin cargo-audit or run audit job with newer Rust.
- flaky curl -> retries + clear logs.

## Required PR notes
Each PR must include:
- Root cause
- Patch summary
- Why it fixes CI
