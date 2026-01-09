# Continuous Integration (CI)

## Overview

The Origin OS Protocol uses GitHub Actions for continuous integration. The CI workflow runs on every push to `main` and on all pull requests.

## CI Strategy Decision

**Decision**: The Rust CI workflow runs **both** `cargo test` and `anchor test`.

### Rationale

This project is a Solana program suite built with the Anchor framework. While it contains Rust code, the primary test suite is written in TypeScript using Anchor's testing framework. Here's why we run both:

#### 1. **Cargo Test** (Rust Unit Tests)
- **What it tests**: Pure Rust unit tests within individual programs
- **Current coverage**: Only `pyth_helpers` program has unit tests (`#[cfg(test)]` modules)
- **Purpose**: Fast unit tests for utility functions, mathematical calculations, and pure Rust logic
- **Example**: Testing `PriceData::conf_ratio_bps()` and `price_in_decimals()` functions

#### 2. **Anchor Test** (Integration Tests)
- **What it tests**: End-to-end integration tests of all programs on a local Solana validator
- **Coverage**: The main test suite in `tests/protocol.ts`
- **Purpose**: Tests program interactions, CPI calls, account validation, and business logic
- **Requirements**: 
  - Local Solana validator
  - Deployed programs
  - Node.js + TypeScript runtime
  - SPL token interactions

### Why Both?

1. **Comprehensive Testing**: Rust unit tests validate low-level logic, while Anchor tests validate on-chain behavior
2. **Fast Feedback**: `cargo test` runs quickly for Rust-only changes
3. **Real-World Validation**: `anchor test` catches integration issues, account constraints, and cross-program interactions
4. **Standard Practice**: This follows Solana/Anchor best practices as documented in Solana's tooling docs

## CI Workflow Steps

The workflow (`.github/workflows/rust.yml`) performs these steps:

### 1. Setup Phase
- Check out code
- Install Rust 1.79.0 (via `rust-toolchain.toml`)
- Install Node.js 18+
- Install Yarn package manager
- Install Node dependencies (`yarn install`)
- Install Solana CLI v1.18.26 (with caching)
- Install Anchor CLI v0.30.1 (with caching)

### 2. Build Phase
- Build Rust code: `cargo build --verbose`
- Build Anchor programs: `anchor build`

### 3. Test Phase
- Run Rust unit tests: `cargo test --verbose`
- Run Anchor integration tests: `anchor test`

## Running Tests Locally

### Rust Unit Tests Only
```bash
cargo test --verbose
```

### Full Integration Test Suite
```bash
anchor test
```

### Both (CI-equivalent)
```bash
cargo build --verbose && \
cargo test --verbose && \
anchor build && \
anchor test
```

## Caching Strategy

To speed up CI runs, we cache:
- **Solana CLI**: `~/.local/share/solana/install` (keyed by version)
- **Anchor CLI**: `~/.cargo/bin/anchor` (keyed by version)
- **Rust dependencies**: Handled automatically by `actions-rust-lang/setup-rust-toolchain`

## Version Pinning

All toolchain versions are pinned for reproducibility:
- **Rust**: 1.79.0 (in `rust-toolchain.toml`)
- **Solana CLI**: 1.18.26 (in workflow env vars)
- **Anchor**: 0.30.1 (in workflow env vars and `Anchor.toml`)

## Future Considerations

As the project evolves, consider:
1. Adding more Rust unit tests to programs (especially for complex calculations)
2. Splitting workflows for faster feedback (e.g., quick Rust tests on every commit, full Anchor tests on PR)
3. Adding security scanning (e.g., cargo-audit, cargo-deny)
4. Adding code coverage reporting
5. Parallelizing test execution if test suite grows large

## Troubleshooting

### CI Fails on Anchor Install
- Check that Anchor version matches in both workflow and `Anchor.toml`
- Verify Solana version compatibility (Anchor 0.30.1 requires Solana 1.18.x)

### CI Fails on Anchor Test
- Ensure all Node dependencies are properly specified in `package.json`
- Check that test file paths in `Anchor.toml` are correct
- Verify TypeScript compilation succeeds

### CI Times Out
- Anchor tests start a local validator which can be slow
- Consider increasing timeout limits in workflow
- Ensure caching is working properly for Solana/Anchor installations
