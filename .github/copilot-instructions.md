# Copilot Instructions (Repo-wide)

These instructions apply to Copilot Chat / Copilot coding agent for this repository.

## Operating mode
- Always fix CI by reading logs first and addressing the FIRST root cause.
- Make minimal diffs and atomic commits.
- Do not "upgrade everything" unless explicitly asked.
- Prefer deterministic builds: pin versions, use lockfiles, avoid `latest`.

## CI troubleshooting defaults
- If you see "binary already exists in destination" during installs:
  - remove the conflicting binary (e.g. `rm -f ~/.cargo/bin/anchor`) OR install with `--force`.
- If rustfmt fails: run `cargo fmt --all` and remove trailing whitespace.
- If cargo-audit fails due to MSRV mismatch:
  - pin cargo-audit to a compatible version OR run audit in a separate job with newer Rust.
  - Do NOT bump pinned Rust/Solana/Anchor toolchains automatically.
- If network installs fail (curl SSL / transient):
  - add retries and fail fast with clear logs.

## PR discipline
- Describe: root cause, patch, and expected effect on CI.
- Update docs when behavior changes.
