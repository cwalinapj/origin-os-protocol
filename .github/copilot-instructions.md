# Copilot Instructions for Origin OS Protocol

This repository contains the Origin OS Protocol - a trustless tokenized rewards system for decentralized AI-powered encrypted hosting + CDN built on Solana.

## Project Overview

Origin OS Protocol is built around an **escrow-only** settlement model:
- Users pre-fund a per-session escrow in an allowed mint (USDC / wSOL / WBTC, etc.)
- Providers lock collateral in the same mint
- Providers can only withdraw via **one-time signed permits**
- Objective, on-chain claim conditions can slash reserved collateral to pay insurance

## Architecture

The protocol consists of 6 main programs:
1. **mode_registry** (Upgradeable) - Manages allowlist of collateral/payment mints
2. **collateral_vault** (Immutable) - Custody provider collateral, track free vs reserved
3. **session_escrow** (Immutable) - User escrow, insurance computation, permits, claims
4. **staking_rewards** - Native token emissions for staked Position NFTs
5. **pyth_helpers** - Shared utilities for Pyth pull-oracle integrations
6. **gateway** - Atomic "gateway" flows (currently stub-only)

## Critical Security Rules

**🔒 DO NOT modify the following without explicit human approval via a Security PR:**

### Security-Sensitive Components
- `programs/session_escrow/` - Escrow, vault interactions, claims, permit verification
- `programs/collateral_vault/` - Collateral custody, slashing
- `programs/mode_registry/` - Mode config, verifier allowlist

### What This Means
1. **No autonomous changes** to fund custody logic
2. **No autonomous changes** to signature verification
3. **No autonomous changes** to payout/slashing calculations
4. **No autonomous changes** to account constraints or PDA seeds

### If Asked to Modify Security Code
When asked to modify security-critical code:
1. **STOP** - Do not make the change autonomously
2. **EXPLAIN** - Describe what change is needed and why
3. **DRAFT** - Provide the code as a suggestion only
4. **REQUIRE** - Request human review via Security PR process

## Safe Modification Zones

Agents MAY freely modify:
- `programs/gateway/` - Stub-only until pyth_helpers tests pass
- `programs/pyth_helpers/` - Price utilities and tests
- `tests/` - All test files
- `*.md` - Documentation
- `.github/` - CI/CD workflows
- `Cargo.toml` - Dependency management (version bumps need verification)

## Toolchain Requirements (PINNED - Do Not Change)

| Component | Version | Notes |
|-----------|---------|-------|
| **Rust** | 1.79.0 | Pinned in `rust-toolchain.toml` |
| **Anchor** | 0.30.1 | CLI and framework |
| **Solana CLI** | 1.18.26 | Must match program dependencies |
| **Node.js** | 18+ | For TypeScript tests |
| **Yarn** | 1.x or 3.x | Package manager |

**NEVER** use `latest` or `*` for versions. All toolchain versions are pinned.

## Build and Test

### Build all programs
```bash
anchor build
```

### Run tests
```bash
anchor test
# OR
yarn test
```

### Clean build cache
```bash
anchor clean
```

## Code Quality Rules

### 1. Tests First
- Add tests before implementing features
- Ensure existing tests pass before committing
- pyth_helpers must have coverage for:
  - Stale price rejection
  - Confidence ratio rejection
  - Conservative min_out calculations

### 2. No Stub Removal Without Tests
The gateway program is **stub-only**. Do not implement actual swap CPI until:
- pyth_helpers test suite is complete and passing
- Price validation is thoroughly tested

### 3. Atomic Commits
- One logical change per commit
- Descriptive messages explaining what and why
- No secrets - Never commit keys, credentials, or .env files
- Run checks - Build must pass before commit

### 4. Dependency Management
- Version bumps require compatibility verification
- Security advisory check for all dependencies
- Full test suite must pass after updates

## Coding Conventions

### Rust / Anchor
- Follow standard Rust conventions
- Use descriptive variable and function names
- Document all public APIs with doc comments
- Prefer explicit error handling over `.unwrap()`
- Security-sensitive code requires extra validation

### TypeScript (Tests)
- Use async/await for asynchronous operations
- Follow existing test patterns in `tests/` directory
- Use meaningful test descriptions
- Clean up resources in test teardown

### Documentation
- Keep README.md updated with high-level changes
- Update BUILD.md if toolchain requirements change
- Document security considerations in code comments
- Use markdown for all documentation files

## PR Guidelines

### Standard PR
- Clear title and description
- Reference related issues
- Ensure all tests pass
- Update relevant documentation
- One logical change per PR

### Security PR (Required for Security-Sensitive Code)
- Must have `security` label
- Minimum 2 approvals from security-designated reviewers
- No force merge - Must not bypass required reviews
- All discussions must be resolved before merge
- Full audit trail required

## When in Doubt

If unsure whether a change is security-sensitive:
1. **Assume it is** - Err on the side of caution
2. **Ask** - Request clarification from humans
3. **Document** - Note your uncertainty in the PR

## What Requires a Security PR

A Security PR is required for any change that:
1. Modifies fund transfer logic
2. Changes signature or permit verification
3. Alters slashing or payout calculations
4. Modifies account validation constraints
5. Changes PDA derivation seeds
6. Adds new authorities or signers
7. Modifies the verifier allowlist logic

## Additional Resources

- See `AGENTS.md` for detailed agent rules
- See `BASELINE.md` for security baseline
- See `BUILD.md` for detailed build instructions
- See `README.md` for project overview and architecture

## Project Context

This is a Solana program project using:
- **Language**: Rust with Anchor framework
- **Blockchain**: Solana
- **Testing**: TypeScript with Mocha/Chai
- **Build System**: Anchor CLI
- **Purpose**: Decentralized escrow and collateral management for AI-powered hosting

Keep code changes minimal, focused, and well-tested. Security is the top priority.
