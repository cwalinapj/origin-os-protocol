# GitHub Copilot Instructions for Origin OS Protocol

## Overview

Origin OS Protocol is a trustless tokenized rewards system for decentralized AI-powered encrypted hosting + CDN, built on Solana using Anchor framework.

## Critical Security Rules

⚠️ **IMPORTANT**: Read and follow all security rules in `/AGENTS.md` before making any code changes.

**DO NOT modify these security-sensitive programs without explicit human approval:**
- `programs/session_escrow/` - Escrow, vault interactions, claims, permit verification
- `programs/collateral_vault/` - Collateral custody, slashing
- `programs/mode_registry/` - Mode config, verifier allowlist

For any changes to these programs, you must:
1. STOP and explain what change is needed
2. Provide code as a suggestion only
3. Request human review via Security PR process

## Tech Stack

### Core Technologies
- **Language**: Rust 1.79.0 (pinned in `rust-toolchain.toml`)
- **Framework**: Anchor 0.30.1
- **Blockchain**: Solana 1.18.26
- **Testing**: TypeScript with Anchor test framework
- **Package Management**: Yarn 1.x/3.x for Node.js dependencies

### Build and Test
```bash
# Build all programs
anchor build

# Run full test suite
anchor test

# Run specific test file
anchor test -- tests/your-test.ts
```

See `BUILD.md` for detailed setup instructions.

## Code Style and Conventions

### General Guidelines
1. **Atomic commits** - One logical change per commit with descriptive messages
2. **No secrets** - Never commit keys, credentials, or `.env` files
3. **Version pinning** - Never use `latest` or `*` for dependency versions
4. **Tests first** - Add or update tests before implementing features
5. **Minimal changes** - Make the smallest possible changes to achieve the goal

### Rust/Anchor Conventions
- Follow Rust naming conventions (snake_case for functions/variables, PascalCase for types)
- Use explicit error types, not generic `Error`
- Document public APIs with doc comments (`///`)
- Validate all account constraints in Anchor instructions
- Use PDA derivation consistently: `["seed", key1, key2]`

### TypeScript Test Conventions
- Use descriptive test names that explain the scenario
- Group related tests with `describe` blocks
- Clean up test state (close accounts, return funds)
- Test both success and error cases

## Architecture Guidelines

### Program Interactions
- `session_escrow` can CPI to `collateral_vault` for reserve/release/slash operations
- `gateway` uses `pyth_helpers` for price validation
- All programs read from `mode_registry` for configuration
- Never bypass escrow or vault account ownership checks

### Account Security
- All PDAs must use consistent seeds across the codebase
- Verify signer requirements on all state-changing instructions
- Check token account ownership before transfers
- Validate discriminators on deserialized accounts

### Insurance and Collateral
- Insurance formula: `coverage_p = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)`
- Collateral reservation: `reserve_r = ceil(coverage_p * cr_bps / 10_000)`
- Invariant: `reserved <= total` in collateral vault
- Claims only paid from reserved collateral

## Safe Zones for Autonomous Changes

You MAY freely modify these areas:
- `programs/gateway/` - Currently stub-only until pyth_helpers tests pass
- `programs/pyth_helpers/` - Price utilities and tests
- `tests/` - All test files
- `*.md` - Documentation files
- `.github/` - CI/CD workflows (verify they run correctly)
- `Cargo.toml` - Dependency management (with version verification)

## Testing Requirements

### pyth_helpers Must Have Coverage For
- Stale price rejection (age checks)
- Confidence ratio rejection (bounds checks)
- Conservative min_out calculations

### gateway Program
- Do NOT implement actual swap CPI until pyth_helpers test suite is complete and passing
- Keep as stub-only for now

### Before Committing
- Ensure all existing tests still pass
- Build must complete without errors
- Run `anchor test` to validate changes
- Only fix test failures related to your changes

## Common Tasks

### Adding a New Instruction
1. Define instruction in program's `lib.rs`
2. Add context struct with account validation
3. Implement instruction logic
4. Add TypeScript test coverage
5. Update documentation if public API

### Updating Dependencies
1. Check version compatibility with pinned toolchain
2. Update `Cargo.toml` or `package.json`
3. Run full test suite to verify compatibility
4. Document any breaking changes

### Price Oracle Integration
1. Always use `pyth_helpers` for price validation
2. Check staleness (max_age parameter)
3. Verify confidence bounds
4. Use conservative calculations for slippage protection

## Error Handling

- Use descriptive error enums in Anchor programs
- Provide helpful error messages for common failures
- Handle all Result types explicitly (no `.unwrap()` in production code)
- Test error conditions, not just happy paths

## Documentation

- Update `README.md` for user-facing changes
- Update `BUILD.md` for toolchain/build process changes
- Keep program-level documentation in sync with code
- Document complex algorithms inline with comments

## When in Doubt

If you're unsure whether a change is security-sensitive or appropriate:
1. **Assume it requires review** - Err on the side of caution
2. **Ask for clarification** - Request human guidance
3. **Document your uncertainty** - Note concerns in PR description
4. **Reference AGENTS.md** - Check against the security rules

## CI/CD

The repository uses GitHub Actions for:
- `anchor.yml` - Anchor-specific builds and tests
- `ci.yml` - General CI checks
- `rust.yml` - Rust-specific linting and checks

Ensure all workflows pass before requesting review.

## Resources

- [Anchor Documentation](https://www.anchor-lang.com/)
- [Solana Documentation](https://docs.solana.com/)
- [Pyth Network Documentation](https://docs.pyth.network/)
- Repository-specific rules: `/AGENTS.md`
- Build instructions: `/BUILD.md`
- Architecture overview: `/README.md`
