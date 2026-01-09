# Architecture

## System Overview

```
┌────────────────────────────────ORIGIN OS PROTOCOL─────────────────────────────┐
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
```

## Programs

### 1. mode_registry (Upgradeable)

**Purpose**: Manages the allowlist of collateral/payment mints with per-mint parameters.

**Accounts**:
- `Registry` - Admin authority + mode count
- `Mode` - Per-mode config (mint, CR ratio, caps, activation)

**Instructions**:
- `initialize()` - Create registry with admin authority
- `add_mode(mint, cr_bps, caps)` - Add new collateral mode with 24h timelock
- `activate_mode(mode_id)` - Activate mode after timelock expires
- `disable_mode(mode_id)` - Block new activity (doesn't seize existing funds)
- `update_mode_params(mode_id, params)` - Update parameters (can only tighten, not loosen)

**Security**:
- Only admin can add/modify modes
- 24-hour timelock on new mode additions
- Immutable mode_id assignments
- Cannot seize or modify existing positions when disabling a mode

---

### 2. collateral_vault (Immutable)

**Purpose**: Custody provider collateral, track free vs reserved amounts, pay claims.

**Accounts**:
- `ProviderPosition` - PDA: `["pos", provider, mode_id]`
  - `provider: Pubkey`
  - `mode_id: u16`
  - `mint: Pubkey`
  - `total_amount: u64`
  - `reserved_amount: u64`
  - `position_nft_mint: Pubkey`
  - `bump: u8`
- `VaultTokenAccount` - SPL token account holding the actual collateral

**Instructions**:

#### `deposit(amount: u64)`
- Transfer tokens from provider to vault
- Update `total_amount`
- Mint Position NFT on first deposit
- **Checks**: Valid mode, mint matches mode

#### `withdraw(amount: u64)`
- Transfer tokens from vault to provider
- Reduce `total_amount`
- **Checks**: 
  - `total_amount - amount >= reserved_amount` (cannot withdraw reserved)
  - Provider owns Position NFT

#### `reserve(session: Pubkey, amount: u64)` (CPI only)
- Called by `session_escrow` when opening a session
- Increase `reserved_amount`
- **Checks**:
  - `reserved_amount + amount <= total_amount`
  - Caller is session_escrow program

#### `release(session: Pubkey, amount: u64)` (CPI only)
- Called by `session_escrow` when session completes normally
- Decrease `reserved_amount`
- **Checks**:
  - Session exists and is authorized
  - Amount matches reserved amount for that session

#### `slash_and_pay(session: Pubkey, amount: u64, beneficiary: Pubkey)` (CPI only)
- Called by `session_escrow` when claim is validated
- Decrease `reserved_amount` and `total_amount`
- Transfer tokens to beneficiary (user)
- **Checks**:
  - Valid claim from session_escrow
  - `amount <= reserved_amount`

**Invariants**:
- `reserved_amount <= total_amount` (always maintained)
- Cannot withdraw below reserved threshold
- Claims only paid from reserved collateral
- Position NFT represents active position

---

### 3. session_escrow (Immutable)

**Purpose**: User escrow, insurance computation, permit redemption, objective claims.

**Accounts**:
- `Session` - PDA: `["sess", user, nonce]`
  - `user: Pubkey`
  - `provider: Pubkey`
  - `mode_id: u16`
  - `nonce: u64`
  - `escrow_balance: u64`
  - `max_spend: u64`
  - `coverage_amount: u64`
  - `reserved_collateral: u64`
  - `start_deadline_slot: u64`
  - `last_activity_slot: u64`
  - `permit_nonce: u64`
  - `status: SessionStatus` (Opened, Started, Closing, Closed)
- `EscrowTokenAccount` - SPL token account holding user's prepaid balance

**Instructions**:

#### `open_session(params: SessionParams)`
- Create Session PDA
- Compute insurance: `coverage = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)`
- Compute required collateral: `reserve = ceil(coverage * cr_bps / 10_000)`
- CPI to `collateral_vault.reserve(session, reserve)`
- Set `start_deadline_slot = current_slot + START_WINDOW`
- **Checks**:
  - Mode is active
  - Provider has sufficient collateral
  - User funds escrow with at least `max_spend`

#### `fund_session(amount: u64)`
- Top up `escrow_balance`
- Transfer tokens from user to escrow account

#### `ack_start()`
- Provider acknowledges session start
- Update `status = Started`
- Update `last_activity_slot`
- **Checks**:
  - `current_slot <= start_deadline_slot`
  - Caller is the designated provider

#### `redeem_permit(permit: SignedPermit)`
- Verify Ed25519 signature on permit
- Check permit fields: `(session, provider, amount, nonce, expiry_slot)`
- Transfer `amount` from escrow to provider
- Increment `permit_nonce` (prevents replay)
- Update `last_activity_slot`
- **Checks**:
  - `permit.nonce == session.permit_nonce`
  - `current_slot <= permit.expiry_slot`
  - `escrow_balance >= amount`
  - Signature valid

#### `close_session()` / `finalize_close()`
- User initiates session close
- Release reserved collateral via CPI to `collateral_vault.release()`
- Return remaining escrow balance to user
- Mark session as Closed

#### `claim_no_start()`
- **Objective claim**: Provider never called `ack_start()` before deadline
- **Checks**: `current_slot > start_deadline_slot && status == Opened`
- Slash reserved collateral, pay user coverage via `collateral_vault.slash_and_pay()`

#### `claim_stall()`
- **Objective claim**: Provider stopped responding (no permit redeemed in STALL_WINDOW)
- **Checks**: `current_slot > last_activity_slot + STALL_WINDOW`
- Slash reserved collateral, pay user coverage via `collateral_vault.slash_and_pay()`

**Insurance Formula**:
```
P_min = 100 USDC
P_cap = 10,000 USDC
a = 0.10  # 10% of max spend
b = 2.00  # 2 USDC per chunk

coverage = clamp(P_min, P_cap, a * max_spend + b * price_per_chunk)
reserve = ceil(coverage * mode.cr_bps / 10_000)
```

**Permit Model**:
- Ed25519 signed permits issued off-chain by user
- Bound to: `(session, provider, amount, nonce, expiry_slot)`
- One-time use enforced by nonce increment
- Permits cannot be replayed across sessions

---

### 4. staking_rewards

**Purpose**: Native token emissions for staked Position NFTs.

**Accounts**:
- `StakingPool` - Global reward accumulator
  - `total_staked_weight: u128`
  - `reward_per_weight_accumulator: u128`
  - `emission_rate: u64` (tokens per slot)
  - `last_update_slot: u64`
- `StakeAccount` - Per-position stake
  - `position: Pubkey`
  - `owner: Pubkey`
  - `weight: u64`
  - `reward_debt: u128`
  - `pending_rewards: u64`

**Instructions**:

#### `initialize_pool(emission_rate: u64)`
- Create global StakingPool
- Set initial emission parameters

#### `stake_position(position: Pubkey)`
- Lock Position NFT in staking contract
- Calculate initial weight from collateral
- Create StakeAccount

#### `update_stake_weight()`
- Recalculate weight based on current collateral state
- Used after deposits/withdrawals to update rewards

#### `claim_rewards()`
- Calculate pending rewards from accumulator
- Transfer $ORIGIN tokens to user

#### `unstake_position()`
- Claim remaining rewards
- Return Position NFT to owner

**Reward Weighting**:
```
weight = 0.80 * reserved_collateral + 0.20 * free_collateral
```

This formula:
- 80% weight for reserved collateral (actively securing sessions)
- 20% weight for free collateral (available capacity)
- Prevents "deposit-only farming" by heavily rewarding active usage

---

### 5. pyth_helpers

**Purpose**: Shared utilities for Pyth pull-oracle integrations.

**Functions**:
- `check_staleness(price_account, max_age_slots)` - Verify price is recent
- `check_confidence(price_account, max_confidence_bps)` - Verify price confidence interval
- `get_conservative_price(price_account, is_buy)` - Get worst-case price for slippage protection
  - For buys: use high end of confidence interval
  - For sells: use low end of confidence interval
- `calculate_min_out(amount_in, price, slippage_bps)` - Calculate minimum output for swaps

**Used by**:
- `gateway` for swap price validation
- Future USD-value weighting logic for cross-mint comparisons

---

### 6. gateway

**Purpose**: Atomic "gateway" flows for native token swaps and funding.

**Instructions** (initially skeleton/stubs):

#### `swap_and_fund_session(params: SwapParams)`
- Swap native $ORIGIN for session mint (USDC/wSOL/WBTC) via allowlisted DEX
- Fund `session_escrow` in same transaction
- Enforce min-out using `pyth_helpers`
- **Checks**:
  - Price oracle not stale
  - Slippage within bounds
  - DEX pool is allowlisted

#### `swap_and_deposit_collateral(params: SwapParams)`
- Swap native $ORIGIN for collateral mint
- Deposit to `collateral_vault` in same transaction
- Enforce min-out using `pyth_helpers`

**Security**:
- Only allowlisted DEX pools
- Oracle price bounds strictly enforced
- Slippage tolerance configurable

---

## Modes (Launch Configuration)

| Mode | Mint | CR (bps) | Insurance Params | Description |
|------|------|----------|------------------|-------------|
| USDC | USDC SPL | 15000 (150%) | P_min=100, P_cap=10000 | Stablecoin, lowest CR |
| wSOL | Wrapped SOL | 17500 (175%) | P_min=150, P_cap=12000 | Native Solana token |
| WBTC | Wormhole BTC | 20000 (200%) | P_min=200, P_cap=15000 | Bridged Bitcoin, highest CR |

**Collateralization Ratios (CR)**:
- Higher volatility mints require higher CR
- CR protects users against price fluctuations during sessions
- Example: 150% CR means provider locks $150 to secure $100 in coverage

---

## Security Considerations

### Immutability
- `collateral_vault` and `session_escrow` are **immutable** (cannot be upgraded)
- This ensures users and providers can trust the core settlement logic
- Only `mode_registry` is upgradeable (for adding new mints)

### Objective Claims
- All claim conditions are objectively verifiable on-chain
- No need for external dispute resolution or subjective judgments
- Claims: no-start (provider didn't acknowledge), stall (provider stopped responding)

### Permit Security
- Ed25519 signatures prevent forgery
- One-time nonce prevents replay attacks
- Session-bound prevents cross-session reuse
- Expiry slot prevents indefinite validity

### Collateral Safety
- Over-collateralization protects against price volatility
- Reserved vs free tracking prevents double-spending collateral
- Slash-and-pay only affects reserved amounts
- Withdrawals blocked if would go below reserved threshold

### Timelock Protection
- New modes require 24-hour timelock before activation
- Allows community review of new collateral types
- Prevents hasty addition of risky mints

---

## Flow Examples

### Provider Onboarding
1. Provider deposits collateral: `collateral_vault.deposit(1000 USDC)`
2. Receive Position NFT (minted on first deposit)
3. Optionally stake NFT: `staking_rewards.stake_position(nft)`
4. Now eligible to accept sessions

### Session Lifecycle (Happy Path)
1. User calls `session_escrow.open_session(provider, mode_id, max_spend=100)`
   - Insurance computed: coverage=110 USDC
   - Collateral reserved: 165 USDC (150% of 110)
2. Provider calls `session_escrow.ack_start()` within deadline
3. User issues signed permits off-chain as service is delivered
4. Provider redeems permits: `session_escrow.redeem_permit(signed_permit)`
5. User calls `session_escrow.close_session()` when done
   - Remaining escrow returned to user
   - Reserved collateral released to provider

### Session with Claim (Provider No-Start)
1. User calls `session_escrow.open_session(...)`
2. Provider **never** calls `ack_start()`
3. After deadline passes, user calls `session_escrow.claim_no_start()`
4. Reserved collateral slashed, insurance coverage paid to user
5. User receives coverage amount as compensation

### Session with Claim (Provider Stall)
1. Session running normally, provider redeeming permits
2. Provider stops responding (doesn't redeem permits for STALL_WINDOW)
3. User calls `session_escrow.claim_stall()`
4. Reserved collateral slashed, insurance coverage paid to user

---

## Future Enhancements

### Planned
- **Naked staking**: Allow users to stake $ORIGIN directly (not just Position NFTs)
- **Slashing governance**: Community proposals for additional slashing conditions
- **Multi-session positions**: Allow providers to serve multiple concurrent sessions
- **Dynamic insurance**: Adjust insurance formulas based on provider reputation

### Under Consideration
- **Cross-chain collateral**: Accept collateral on other chains via bridges
- **Partial withdrawals**: Allow providers to withdraw while maintaining reserved amounts
- **Insurance pools**: User-contributed insurance pools for additional coverage
- **Reputation system**: On-chain provider reputation based on claim history
