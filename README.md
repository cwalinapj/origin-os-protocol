# Origin OS Protocol

Trustless tokenized rewards system for decentralized AI-powered encrypted hosting.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ORIGIN OS PROTOCOL                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────┐                                                        │
│  │  MODE_REGISTRY   │  (Upgradeable)                                         │
│  │  ──────────────  │                                                        │
│  │  • USDC mode     │                                                        │
│  │  • wSOL mode     │                                                        │
│  │  • WBTC mode     │                                                        │
│  └────────┬─────────┘                                                        │
│           │ reads                                                            │
│           ▼                                                                  │
│  ┌──────────────────┐      CPI       ┌──────────────────┐                   │
│  │ COLLATERAL_VAULT │◄──────────────►│  SESSION_ESCROW  │                   │
│  │  ──────────────  │  reserve/      │  ──────────────  │                   │
│  │  • Provider      │  release/      │  • User escrow   │                   │
│  │    positions     │  slash_pay     │  • Permits       │                   │
│  │  • Position NFT  │                │  • Claims        │                   │
│  │  • free/reserved │                │  • Insurance     │                   │
│  └────────┬─────────┘                └──────────────────┘                   │
│           │                                                                  │
│           │ stake NFT                                                        │
│           ▼                                                                  │
│  ┌──────────────────┐                                                        │
│  │ STAKING_REWARDS  │                                                        │
│  │  ──────────────  │                                                        │
│  │  • $ORIGIN       │                                                        │
│  │    emissions     │                                                        │
│  │  • Weighted by   │                                                        │
│  │    reserved      │                                                        │
│  │    collateral    │                                                        │
│  └──────────────────┘                                                        │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Programs

### 1. mode_registry (Upgradeable)

Manages allowlist of collateral/payment mints with per-mint parameters.

**Accounts:**
- `Registry` - Admin authority + mode count
- `Mode` - Per-mode config (mint, CR ratio, caps, activation)

**Instructions:**
- `initialize()` - Create registry
- `add_mode()` - Add new collateral mode with timelock
- `activate_mode()` - Activate after timelock
- `disable_mode()` - Block new activity (doesn't seize funds)
- `update_mode_params()` - Tighten parameters only

### 2. collateral_vault (Immutable)

Custody provider collateral, track free vs reserved, pay claims.

**Accounts:**
- `ProviderPosition` - PDA: `["pos", provider, mode_id]`
- `VaultTokenAccount` - SPL token account for collateral

**Instructions:**
- `deposit()` - Add collateral, mint Position NFT on first deposit
- `withdraw()` - Withdraw free (unreserved) collateral
- `reserve()` - Lock collateral for session (CPI from session_escrow)
- `release()` - Unlock after successful session
- `slash_and_pay()` - Pay claim from reserved collateral

**Invariants:**
- `reserved <= total`
- Withdrawals cannot reduce `total` below `reserved`
- Claims only paid from `reserved`

### 3. session_escrow (Immutable)

User escrow, insurance computation, permits, objective claims.

**Accounts:**
- `Session` - PDA: `["sess", user, nonce]`
- `EscrowTokenAccount` - User's prepaid balance

**Instructions:**
- `open_session()` - Create session, compute insurance, reserve collateral
- `fund_session()` - Top up user escrow
- `ack_start()` - Provider acknowledges (before deadline)
- `redeem_permit()` - Provider withdraws via signed permit
- `close_session()` / `finalize_close()` - User-initiated close
- `claim_no_start()` - Objective claim: provider didn't start
- `claim_stall()` - Objective claim: provider stopped responding

**Insurance Formula:**
```
coverage_p = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)
reserve_r = ceil(coverage_p * cr_bps / 10_000)
```

**Permit Model:**
- Ed25519 signed permits
- One-time use (nonce tracking)
- Bound to (session, provider, amount, nonce, expiry_slot)

### 4. staking_rewards

Native token emissions for staked Position NFTs.

**Accounts:**
- `StakingPool` - Global reward accumulator
- `StakeAccount` - Per-position stake

**Instructions:**
- `initialize_pool()` - Create staking pool
- `stake_position()` - Stake Position NFT
- `update_stake_weight()` - Recalculate based on collateral
- `claim_rewards()` - Claim $ORIGIN emissions
- `unstake_position()` - Unstake and claim

**Reward Weighting:**
- 80% weight: Reserved collateral (actively securing sessions)
- 20% weight: Free collateral (capacity)

This prevents "deposit-only farming."

## Modes (Launch)

| Mode | Mint | CR (bps) | Description |
|------|------|----------|-------------|
| USDC | USDC SPL | 15000 | Stablecoin, 150% collateral |
| wSOL | Wrapped SOL | 17500 | Native, 175% collateral |
| WBTC | Wormhole BTC | 20000 | Bridged BTC, 200% collateral |

## Token Economics ($ORIGIN)

```yaml
total_supply: 1_000_000_000
distribution:
  community_rewards: 40%  # Staking emissions
  development: 20%
  early_contributors: 15%
  liquidity: 15%
  treasury: 10%
```

## Build

```bash
# Install Anchor
cargo install --git https://github.com/coral-xyz/anchor anchor-cli

# Build all programs
anchor build

# Run tests
anchor test

# Deploy to devnet
anchor deploy --provider.cluster devnet
```

## Security Invariants

1. **mode_registry changes cannot move funds** - Only blocks new activity
2. **Immutable money core** - collateral_vault and session_escrow are not upgradeable
3. **Objective claims only** - No subjective adjudication in v1
4. **Payment mint == collateral mint == insurance mint** per session
5. **Provider cannot withdraw without valid permit**
6. **Permits are one-time** (nonce tracking)
7. **Reserved collateral backs all active sessions**

## Off-Chain Components

- **Client App**: Encrypts data, issues permits, monitors sessions
- **Provider App**: Stores encrypted chunks, redeems permits
- **Coordinator** (optional): Matches users ↔ providers, runs watchers

All coordination is non-custodial; funds only move per contract rules.

## License

Apache 2.0
