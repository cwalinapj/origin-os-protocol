# GitHub Copilot Instructions for Origin OS Protocol (Repo-wide)

These instructions apply to ALL Copilot Chat / Copilot Agent suggestions and code generation for this repository.

---

## Overview

Origin OS Protocol is a trustless tokenized rewards system for decentralized AI-powered encrypted hosting + CDN, built on Solana using the Anchor framework.

---

## Critical Security Rules

⚠️ IMPORTANT: Read and follow all security rules in `/AGENTS.md` before making any code changes.

DO NOT modify these security-sensitive programs without explicit human approval:
- `programs/session_escrow/` — Escrow, vault interactions, claims, permit verification
- `programs/collateral_vault/` — Collateral custody, slashing
- `programs/mode_registry/` — Mode config, verifier allowlist

For any changes to these programs, you must:
1. STOP and explain what change is needed
2. Provide code as a suggestion only (no direct edits unless approved)
3. Request human review via the Security PR process

---

## Tech Stack (Pinned — do not “upgrade everything” unless explicitly asked)

### Core Technologies
- Language: Rust 1.79.0 (pinned in `rust-toolchain.toml`)
- Framework: Anchor 0.30.1
- Blockchain: Solana 1.18.26
- Testing: TypeScript with Anchor test framework
- Package Management: Yarn Classic 1.22.22 (lockfile required)

### Build and Test
```bash
# Build all programs
anchor build

# Run full test suite
anchor test

# Run specific test file
anchor test -- tests/your-test.ts
See BUILD.md for detailed setup instructions.


Code Style and Conventions

General Guidelines
	1.	Atomic commits — one logical change per commit with descriptive messages
	2.	No secrets — never commit keys, credentials, or .env files
	3.	Version pinning — never use latest or * for dependency versions
	4.	Tests first — add/update tests before implementing features where possible
	5.	Minimal changes — smallest possible changes to achieve the goal

Rust/Anchor Conventions
	•	Follow Rust naming conventions (snake_case for functions/variables, PascalCase for types)
	•	Use explicit error enums, not generic Error
	•	Document public APIs with doc comments (///)
	•	Validate all account constraints in Anchor instructions
	•	Use PDA derivation consistently: ["seed", key1, key2]
	•	No .unwrap() / .expect() in production on-chain code

TypeScript Test Conventions
	•	Use descriptive test names that explain the scenario
	•	Group related tests with describe blocks
	•	Clean up test state when feasible (close accounts, return funds)
	•	Test both success and error cases

⸻

Architecture Guidelines

Program Interactions
	•	session_escrow can CPI to collateral_vault for reserve/release/slash operations
	•	gateway uses pyth_helpers for price validation
	•	All programs read from mode_registry for configuration
	•	Never bypass escrow or vault account ownership checks

Account Security
	•	All PDAs must use consistent seeds across the codebase
	•	Verify signer requirements on all state-changing instructions
	•	Check token account ownership before transfers
	•	Validate discriminators on deserialized accounts

Insurance and Collateral
	•	Insurance formula: coverage_p = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)
	•	Collateral reservation: reserve_r = ceil(coverage_p * cr_bps / 10_000)
	•	Invariant: reserved <= total in collateral vault
	•	Claims only paid from reserved collateral

⸻

Safe Zones for Autonomous Changes

You MAY freely modify these areas (without extra approval):
	•	programs/gateway/ — currently stub-only until pyth_helpers tests pass
	•	programs/pyth_helpers/ — price utilities and tests
	•	tests/ — all test files
	•	*.md — documentation
	•	.github/ — CI/CD workflows (verify they run correctly)
	•	Cargo.toml / package.json — dependency management (with version verification)

You MUST be extra cautious (and follow Critical Security Rules) when changes touch:
	•	escrow flows, permits, slashing, vault custody, or mode governance

⸻

Testing Requirements

pyth_helpers Must Have Coverage For
	•	stale price rejection (age checks)
	•	confidence ratio rejection (bounds checks)
	•	conservative min_out calculations

gateway Program
	•	Do NOT implement actual swap CPI until pyth_helpers test suite is complete and passing
	•	Keep as stub-only for now

Before Committing
	•	Ensure all existing tests still pass
	•	Build must complete without errors
	•	Run anchor test to validate changes
	•	Only fix test failures related to your changes

⸻

CI/CD Guardrails (Auto-fix the recurring issues)

rustfmt failures
	•	CI may run cargo fmt --all --check
	•	If it fails due to whitespace/formatting, run:
	•	cargo fmt --all
	•	Remove trailing whitespace (blank lines must not contain spaces)

cargo-audit MSRV drift
	•	If cargo-audit requires newer Rust than the pinned 1.79 toolchain:
	•	Do NOT upgrade the entire Solana/Anchor toolchain automatically
	•	Instead: run audit in a separate CI job with newer Rust OR pin cargo-audit to a compatible version

Solana installer flakiness in CI
	•	If Solana CLI install fails due to TLS/network (e.g., SSL errors):
	•	Use hardened curl flags (HTTPS/TLS) and retries
	•	Do not silently ignore missing Solana; fail fast with clear logs

Yarn reproducibility
	•	Commit yarn.lock
	•	Use yarn install --frozen-lockfile in CI when lockfile exists

⸻

Common Tasks

Adding a New Instruction
	1.	Define instruction in program’s lib.rs
	2.	Add context struct with account validation
	3.	Implement instruction logic
	4.	Add TypeScript test coverage
	5.	Update documentation if public API

Updating Dependencies
	1.	Check version compatibility with pinned toolchain
	2.	Update Cargo.toml or package.json
	3.	Run full test suite to verify compatibility
	4.	Document any breaking changes

Price Oracle Integration
	1.	Always use pyth_helpers for price validation
	2.	Check staleness (max_age parameter)
	3.	Verify confidence bounds
	4.	Use conservative calculations for slippage protection

⸻

When in Doubt

If you’re unsure whether a change is security-sensitive or appropriate:
	1.	Assume it requires review
	2.	Ask for clarification
	3.	Document uncertainty in PR description
	4.	Reference /AGENTS.md and /BUILD.md
If you want to go one step further: GitHub also supports **path-specific** instruction files under `.github/instructions/*.instructions.md` with `applyTo:` globs (useful if you want even stricter rules only for `programs/session_escrow/**`).  [oai_citation:1‡GitHub Docs](https://docs.github.com/enterprise-cloud%40latest/copilot/customizing-copilot/adding-repository-custom-instructions-for-github-copilot?utm_source=chatgpt.com)
