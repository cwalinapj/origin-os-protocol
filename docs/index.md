# Origin OS Protocol

**Trustless tokenized rewards system for decentralized AI-powered encrypted hosting + CDN**

## Overview

The Origin OS Protocol implements an **escrow-only** settlement model for decentralized encrypted hosting and content delivery:

- **Users** pre-fund a per-session escrow in an allowed mint (USDC / wSOL / WBTC, etc.)
- **Providers** lock collateral in the same mint
- **Providers** can only withdraw via **one-time signed permits**
- **Objective**, on-chain claim conditions can slash reserved collateral to pay insurance

This design ensures trustless operation without relying on trusted intermediaries or oracles for dispute resolution.

## Key Features

### Escrow-Based Payment System
- Per-session escrow accounts for user prepayment
- Provider withdrawal only through cryptographically signed permits
- One-time use permits with bound parameters (session, provider, amount, nonce, expiry)

### Collateral Management
- Providers deposit collateral in supported mints
- Collateral is split into "free" and "reserved" amounts
- Reserved collateral backs active sessions and can be slashed for objective claims
- Position NFTs represent provider collateral stakes

### Multi-Mint Support
The protocol supports multiple collateral/payment mints through a mode registry:
- **USDC mode**: Stablecoin with 150% collateral requirement
- **wSOL mode**: Wrapped SOL with 175% collateral requirement  
- **WBTC mode**: Bridged Bitcoin with 200% collateral requirement

### Insurance & Claims
- Automatic insurance calculation based on session parameters
- Objective claim conditions (no-start, stall detection)
- Slashing from reserved collateral to compensate users
- No subjective dispute resolution required

### Staking & Rewards
- Stake Position NFTs to earn $ORIGIN token emissions
- Reward weighting: 80% for reserved collateral, 20% for free collateral
- Prevents "deposit-only farming" by rewarding active participation

## Architecture

The protocol consists of 6 Solana programs:

1. **mode_registry** (Upgradeable) - Manages allowlist of collateral mints
2. **collateral_vault** (Immutable) - Custody provider collateral
3. **session_escrow** (Immutable) - User escrow and permit redemption
4. **staking_rewards** - Native token emissions for staked positions
5. **pyth_helpers** - Price oracle utilities
6. **gateway** - Atomic swap and funding flows

See [architecture.md](architecture.md) for detailed specifications.

## Token Economics

**$ORIGIN** is the native protocol token with a total supply of 1 billion tokens.

Distribution:
- 40% Community rewards (staking emissions)
- 20% Development
- 15% Early contributors
- 15% Liquidity
- 10% Treasury

See [token-economics.md](token-economics.md) for details.

## Getting Started

### For Developers

See the [BUILD.md](../BUILD.md) in the root directory for:
- Toolchain requirements (Rust 1.79.0, Anchor 0.30.1, Solana CLI 1.18.26)
- Installation instructions
- Build and test commands

### For Deployers

See [deployment.md](deployment.md) for:
- Deployment procedures
- Network configuration
- Program initialization

### For App Builders

See [apps-and-lam.md](apps-and-lam.md) for:
- LAM (AI helper) integration
- MCP server implementation
- UI widget development

## Off-Chain Components

The protocol requires off-chain components for full operation:

- **Client App**: Encrypts data, issues permits, monitors sessions, requests claims
- **Provider App**: Stores encrypted chunks, serves retrieval, redeems permits
- **Coordinator (optional)**: Matches users ↔ providers, runs watchers/verifiers

All coordination is non-custodial; funds only move per contract rules.

## Security Model

The protocol prioritizes security through:

- **Immutable core programs**: `collateral_vault` and `session_escrow` cannot be upgraded
- **Objective claims only**: No subjective dispute resolution required
- **Conservative collateralization**: Over-collateralization protects users
- **One-time permits**: Prevent double-spending and replay attacks
- **Timelock on mode additions**: 24-hour delay before new modes activate

## Resources

- [Main README](../README.md)
- [Build Instructions](../BUILD.md)
- [Architecture Documentation](architecture.md)
- [Token Economics](token-economics.md)
- [Deployment Guide](deployment.md)
- [Apps & LAM Integration](apps-and-lam.md)

## License

See the LICENSE file in the repository root.
