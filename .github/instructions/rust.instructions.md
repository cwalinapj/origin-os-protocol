---
applyTo: "**/*.rs"
---

# Rust Instructions

- No `.unwrap()` / `.expect()` in production code; handle errors explicitly.
- Use `checked_*` math for arithmetic that can overflow; only use `saturating_*` when clamping is intentional.
- Keep changes minimal and add tests for bug fixes.
- If formatting fails, run `cargo fmt --all`.
