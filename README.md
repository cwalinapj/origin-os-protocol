# Origin OS Protocol

Trustless tokenized rewards system for decentralized AI-powered **encrypted hosting + CDN**.

This protocol is built around an **escrow-only** settlement model:
- Users pre-fund a per-session escrow in an allowed mint (USDC / wSOL / WBTC, etc.)
- Providers lock collateral in the same mint
- Providers can only withdraw via **one-time signed permits**
- Objective, on-chain claim conditions can slash reserved collateral to pay insurance
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
│  ┌──────────────────┐            uses             ┌──────────────────┐       │
│  │   PYTH_HELPERS   │◄──────────────────────────►│      GATEWAY      │       │
│  │  ──────────────  │  (price checks + bounds)   │  ──────────────   │       │
│  │  • staleness     │                            │  • native→mode    │       │
│  │  • confidence    │                            │    mint swap      │       │
│  │  • conservative  │                            │  • fund escrow    │       │
│  │    min_out math  │                            │  • deposit collat │       │
│  └──────────────────┘                            └──────────────────┘       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘

---

## Programs

### 1) `mode_registry` (Upgradeable)
Manages allowlist of collateral/payment mints with per-mint parameters.

**Accounts**
- `Registry` - Admin authority + mode count
- `Mode` - Per-mode config (mint, CR ratio, caps, activation)

**Instructions**
- `initialize()` - Create registry
- `add_mode()` - Add new collateral mode with timelock
- `activate_mode()` - Activate after timelock
- `disable_mode()` - Block new activity (doesn't seize funds)
- `update_mode_params()` - Tighten parameters only

---

### 2) `collateral_vault` (Immutable)
Custody provider collateral, track free vs reserved, pay claims.

**Accounts**
- `ProviderPosition` - PDA: `["pos", provider, mode_id]`
- `VaultTokenAccount` - SPL token account for collateral

**Instructions**
- `deposit()` - Add collateral, mint Position NFT on first deposit
- `withdraw()` - Withdraw free (unreserved) collateral
- `reserve()` - Lock collateral for session (CPI from `session_escrow`)
- `release()` - Unlock after successful session
- `slash_and_pay()` - Pay claim from reserved collateral

**Invariants**
- `reserved <= total`
- Withdrawals cannot reduce total below reserved
- Claims only paid from reserved

---

### 3) `session_escrow` (Immutable)
User escrow, insurance computation, permits, objective claims.

**Accounts**
- `Session` - PDA: `["sess", user, nonce]`
- `EscrowTokenAccount` - User’s prepaid balance

**Instructions**
- `open_session()` - Create session, compute insurance, reserve collateral
- `fund_session()` - Top up user escrow
- `ack_start()` - Provider acknowledges (before deadline)
- `redeem_permit()` - Provider withdraws via signed permit
- `close_session()` / `finalize_close()` - User-initiated close
- `claim_no_start()` - Objective claim: provider didn’t start
- `claim_stall()` - Objective claim: provider stopped responding

**Insurance Formula**
- `coverage_p = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)`
- `reserve_r = ceil(coverage_p * cr_bps / 10_000)`

**Permit Model**
- Ed25519 signed permits
- One-time use (nonce tracking)
- Bound to `(session, provider, amount, nonce, expiry_slot)`

---

### 4) `staking_rewards`
Native token emissions for staked Position NFTs (and later: optional naked/native staking).

**Accounts**
- `StakingPool` - Global reward accumulator
- `StakeAccount` - Per-position stake

**Instructions**
- `initialize_pool()` - Create staking pool
- `stake_position()` - Stake Position NFT
- `update_stake_weight()` - Recalculate based on collateral
- `claim_rewards()` - Claim $ORIGIN emissions
- `unstake_position()` - Unstake and claim

**Reward Weighting**
- 80% weight: Reserved collateral (actively securing sessions)
- 20% weight: Free collateral (capacity)
This prevents "deposit-only farming."

---

### 5) `pyth_helpers`
Shared utilities for Pyth pull-oracle integrations:
- staleness checks / max age
- confidence checks / bounds
- conservative pricing helpers for min-out / slippage enforcement

Used by `gateway` and any future USD-value weighting logic.

---

### 6) `gateway`
Atomic “gateway” flows (initially skeleton/stubs):
- convert native $ORIGIN into the session mint (e.g., USDC/wSOL/WBTC) via allowlisted DEX pools
- fund `session_escrow` or deposit `collateral_vault` in the same transaction
- enforce oracle-based min-out using `pyth_helpers`

---

## Modes (Launch)
| Mode | Mint | CR (bps) | Description |
|------|------|----------|-------------|
| USDC | USDC SPL | 15000 | Stablecoin, 150% collateral |
| wSOL | Wrapped SOL | 17500 | Native, 175% collateral |
| WBTC | Wormhole BTC | 20000 | Bridged BTC, 200% collateral |

---

## Token Economics ($ORIGIN)
- `total_supply: 1_000_000_000`
- distribution:
  - `community_rewards: 40%`  # Staking emissions
  - `development: 20%`
  - `early_contributors: 15%`
  - `liquidity: 15%`
  - `treasury: 10%`

---

## Apps + LAM (How users interact)

### What the LAM is
The “LAM” is the AI helper users talk to inside the apps. It guides users through:
- finding hosts
- quoting escrow + insurance requirements
- building transactions for funding/staking
- monitoring sessions
- filing objective claims (when eligible)

### How users use it (ChatGPT Apps)
Users interact by chatting in ChatGPT while your app is enabled:
1) User enables the app (or their workspace admin enables it)
2) User chats naturally (“Find the lowest latency host near me”)
3) The model calls your app’s **MCP tools** (server-side) to fetch data or construct tx payloads
4) The embedded UI widget renders results (tables, buttons, next steps)
5) Any action that moves funds requires explicit wallet signing

### Technical implementation (for builders)
Each app consists of:
- a **UI widget** (React bundle) rendered in ChatGPT
- an **MCP server** exposing tools/resources over `/mcp`

The MCP server returns structured JSON outputs and (optionally) widget templates; the model decides when to call tools.

---

## Off-Chain Components
- **Client App**: encrypts data, issues permits, monitors sessions, can request objective claims
- **Provider App**: stores encrypted chunks, serves retrieval, redeems permits
- **Coordinator (optional)**: matches users ↔ providers, runs watchers/verifiers
All coordination is non-custodial; funds only move per contract rules.

---

## Toolchain Requirements
| Component | Version |
|----------|---------|
| Rust | 1.79.0 (pinned in rust-toolchain.toml) |
| Anchor | 0.30.1 |
| Solana CLI | 1.18.26 |

See `BUILD.md` for detailed setup and installation instructions.

---

## Quick Start

```bash
# Build all programs
anchor build

# Run tests
anchor test

# Deploy to devnet
anchor deploy --provider.cluster devnet

