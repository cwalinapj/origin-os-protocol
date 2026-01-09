# Devnet Deployment Action Plan

**Goal:** Deploy Origin OS Protocol to Solana devnet  
**Strategy:** Fast track (exclude gateway initially)  
**Timeline:** 3-4 days

---

## Phase 1: Fix Build Environment (Day 1)

### Step 1.1: Update Rust Version Documentation
Update all documentation to reflect Rust 1.85.0:

- [ ] Edit `BUILD.md` line 7: `1.79.0` → `1.85.0`
- [ ] Edit `BUILD.md` line 107: `rustc 1.79.0` → `rustc 1.85.0`
- [ ] Edit `README.md` line 217: `1.79.0` → `1.85.0`

### Step 1.2: Update CI Workflows
Update all GitHub Actions workflows:

- [ ] `.github/workflows/anchor.yml` line 26: `@1.79.0` → `@1.85.0`
- [ ] `.github/workflows/ci.yml` line 10: `"1.79.0"` → `"1.85.0"`
- [ ] `.github/workflows/rust.yml` line 19: `@1.79.0` → `@1.85.0`

### Step 1.3: Verify Build
```bash
# Clean previous build artifacts
anchor clean
rm -rf target/

# Build all programs
anchor build

# Verify output
ls -la target/deploy/*.so
```

Expected output: 7 compiled `.so` files

---

## Phase 2: Testing (Day 1-2)

### Step 2.1: Install Dependencies
```bash
# Install Node dependencies
yarn install --frozen-lockfile

# Verify Anchor version
anchor --version  # Should show 0.30.1
```

### Step 2.2: Run Test Suite
```bash
# Run all tests
anchor test

# If tests fail, investigate and fix
# Log test results for review
```

### Step 2.3: Manual Program Verification
For each program, verify key instructions exist:

**mode_registry:**
- [x] `initialize()`
- [x] `add_mode()`
- [x] `activate_mode()`
- [x] `update_mode_params()`

**collateral_vault:**
- [x] `deposit_collateral()`
- [x] `withdraw_collateral()`
- [x] `reserve()` (CPI)
- [x] `release()` (CPI)
- [x] `slash_and_pay()` (CPI)

**session_escrow:**
- [x] `open_session()`
- [x] `fund_session()`
- [x] `redeem_permit()`
- [x] `claim_no_start()`
- [x] `claim_stall()`

**staking_rewards:**
- [x] `initialize_pool()`
- [x] `stake_position()`
- [x] `claim_rewards()`
- [x] `unstake_position()`

---

## Phase 3: Deployment (Day 3)

### Step 3.1: Configure Solana CLI
```bash
# Set devnet cluster
solana config set --url devnet

# Generate a new keypair for deployment (or use existing)
solana-keygen new --outfile ~/.config/solana/devnet-deployer.json

# Set as default
solana config set --keypair ~/.config/solana/devnet-deployer.json

# Airdrop devnet SOL
solana airdrop 10
solana balance
```

### Step 3.2: Update Anchor.toml for Devnet
```toml
[provider]
cluster = "devnet"
wallet = "~/.config/solana/devnet-deployer.json"
```

### Step 3.3: Deploy Programs
```bash
# Deploy all programs to devnet
anchor deploy --provider.cluster devnet

# Note: First deployment will use placeholder program IDs
# This is expected and will be fixed in next step
```

### Step 3.4: Update Program IDs
After deployment, update `Anchor.toml` with actual deployed program IDs:

```bash
# Get deployed program IDs
anchor keys list

# Manually copy each program ID to Anchor.toml [programs.devnet] section
# Example:
# mode_registry = "7xKX...actual_id_here"
# collateral_vault = "8yLM...actual_id_here"
# etc.
```

### Step 3.5: Rebuild and Redeploy
```bash
# Rebuild with correct program IDs embedded
anchor build

# Redeploy (this ensures program IDs match)
anchor deploy --provider.cluster devnet
```

---

## Phase 4: Initialize On-Chain State (Day 3-4)

### Step 4.1: Prepare Initialization Script
Create `scripts/initialize-devnet.ts`:

```typescript
import * as anchor from "@coral-xyz/anchor";

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  
  // Load programs
  const modeRegistry = anchor.workspace.ModeRegistry;
  const stakingRewards = anchor.workspace.StakingRewards;
  
  // 1. Initialize mode registry
  console.log("Initializing mode registry...");
  await modeRegistry.methods.initialize()
    .accounts({ /* ... */ })
    .rpc();
  
  // 2. Add USDC mode
  console.log("Adding USDC mode...");
  await modeRegistry.methods.addMode({
    mint: DEVNET_USDC_MINT,
    crBps: 15000,
    // ... other params
  }).rpc();
  
  // 3. Activate USDC mode (after timelock)
  // ... 
  
  // 4. Initialize staking pool
  console.log("Initializing staking pool...");
  await stakingRewards.methods.initializePool()
    .accounts({ /* ... */ })
    .rpc();
  
  console.log("✅ Initialization complete!");
}

main();
```

### Step 4.2: Run Initialization
```bash
# Run initialization script
ts-node scripts/initialize-devnet.ts

# Verify state
solana account <mode_registry_pda>
solana account <staking_pool_pda>
```

---

## Phase 5: Smoke Testing (Day 4)

### Test Case 1: Mode Registry
```bash
# Read mode config
# Use CLI or TypeScript script to call view methods
# Verify USDC mode is configured correctly
```

### Test Case 2: Collateral Deposit
```bash
# As provider:
# 1. Approve vault to spend collateral
# 2. Call deposit_collateral()
# 3. Verify position created
# 4. Verify NFT minted
```

### Test Case 3: Session Creation
```bash
# As user:
# 1. Approve escrow to spend payment tokens
# 2. Call open_session()
# 3. Verify escrow created
# 4. Verify collateral reserved
```

### Test Case 4: Permit Redemption
```bash
# As provider:
# 1. Generate signed permit
# 2. Call redeem_permit()
# 3. Verify provider received payment
# 4. Verify escrow balance decreased
```

### Test Case 5: Basic Claim
```bash
# As user:
# 1. Wait for no_start deadline
# 2. Call claim_no_start()
# 3. Verify insurance paid
# 4. Verify collateral slashed
```

---

## Phase 6: Documentation (Day 4)

### Update Docs
- [ ] Create `docs/DEVNET_GUIDE.md` with:
  - Deployed program addresses
  - Devnet token mint addresses
  - Example transactions
  - Faucet links for test tokens
  
- [ ] Update main README with devnet info:
  - Add "Try on Devnet" section
  - Link to devnet guide
  - List known limitations

- [ ] Create `MANUAL_FUNDING.md`:
  - Document workflow without gateway
  - Show how to swap on Jupiter
  - Show how to fund escrow manually

---

## Phase 7: Monitoring & Iteration (Ongoing)

### Set Up Monitoring
- [ ] Block explorer links for each program
- [ ] Transaction monitoring script
- [ ] Error logging and alerts

### Collect Feedback
- [ ] Set up feedback form
- [ ] Monitor Discord/Telegram for issues
- [ ] Document common problems

### Prepare for Mainnet
- [ ] Security audit of core programs
- [ ] Stress testing on devnet
- [ ] Economic parameter validation
- [ ] Mainnet deployment plan

---

## Rollback Plan

If critical issues are discovered after devnet deployment:

1. **Pause new sessions:** Call admin function to disable session creation
2. **Allow existing sessions to complete:** Don't interrupt active users
3. **Fix issue:** Update code, test thoroughly
4. **Redeploy:** Deploy fixed version
5. **Re-initialize:** Migrate state if needed
6. **Resume operations:** Re-enable session creation

---

## Success Criteria

Devnet deployment is considered successful when:

- ✅ All 6 core programs deployed (excluding gateway)
- ✅ Programs can be called without errors
- ✅ State initialization complete (registry + staking pool)
- ✅ At least 3 complete user flows tested successfully:
  - Provider deposits collateral
  - User creates session
  - Provider redeems permit
- ✅ Documentation published
- ✅ No critical vulnerabilities identified

---

## Known Limitations (Initial Devnet)

Document these clearly for users:

1. **No Gateway:** Users must swap on external DEX, then fund escrow manually
2. **Test Tokens Only:** Use devnet USDC/wSOL, not real value
3. **Frequent Resets:** Devnet state may be wiped, losing test data
4. **Limited Monitoring:** Basic transaction monitoring, not production-grade
5. **No UI Yet:** Command-line or script interaction only

---

## Timeline Summary

| Phase | Duration | Key Deliverable |
|-------|----------|----------------|
| 1. Fix Build | 2-4 hours | Programs compile |
| 2. Testing | 4-8 hours | Tests pass |
| 3. Deployment | 2-3 hours | Programs on devnet |
| 4. Initialize | 1-2 hours | State ready |
| 5. Smoke Testing | 2-4 hours | Verified working |
| 6. Documentation | 2-4 hours | User guide |
| **Total** | **3-4 days** | **Live on devnet** |

---

## Questions to Answer

Before starting:

- [ ] Who has authority to deploy to devnet?
- [ ] Which wallet will be the admin/authority?
- [ ] What are the exact economic parameters for each mode?
- [ ] Do we need external security review before devnet?
- [ ] Who will maintain devnet deployment?

---

## Next Steps

1. Review this plan with team
2. Get approval to proceed
3. Fix Rust version (Phase 1)
4. Execute remaining phases
5. Iterate based on feedback

---

**Status:** ⏸️ Awaiting approval to proceed  
**Owner:** TBD  
**Last Updated:** 2026-01-09
