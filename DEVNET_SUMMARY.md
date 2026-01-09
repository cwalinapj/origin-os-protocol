# Devnet Readiness - Quick Summary

**Date:** 2026-01-09  
**Status:** ❌ NOT READY (2 blockers)  
**Time to Fix:** 3-4 days (fast track)

---

## TL;DR

The Origin OS Protocol has a solid foundation with 6/7 programs feature-complete, but **cannot be deployed to devnet yet** due to:

1. **Build is broken** - Rust version mismatch
2. **Gateway is incomplete** - Swap functionality stubbed

**Fastest path:** Fix Rust version, deploy without gateway (3-4 days)

---

## Critical Blockers

### 🔴 Blocker #1: Can't Build
- **Problem:** rust-toolchain.toml says 1.85.0, but docs/CI say 1.79.0
- **Symptom:** `blake3` dependency needs edition2024 (Rust 1.85+)
- **Fix:** Update BUILD.md, README.md, and 3 CI workflow files to use 1.85.0
- **Time:** 30 minutes

### 🔴 Blocker #2: Gateway Incomplete  
- **Problem:** `swap_and_fund_session()` has 3 TODOs, no swap CPI
- **Impact:** Users can't do atomic native→mode swaps
- **Option A:** Complete gateway (7 days)
- **Option B:** Launch without gateway, users swap manually (0 days)
- **Recommendation:** Option B

---

## What's Ready ✅

| Component | Status | Notes |
|-----------|--------|-------|
| mode_registry | ✅ Complete | 527 LOC, all functions implemented |
| collateral_vault | ✅ Complete | 464 LOC, CPI-ready |
| session_escrow | ✅ Complete | 2,174 LOC, permits + claims |
| staking_rewards | ✅ Complete | 920 LOC, emissions working |
| naked_staking | ✅ Complete | 839 LOC |
| pyth_helpers | ✅ Complete | 631 LOC, price validation |
| gateway | ⚠️ Partial | 512 LOC, config OK, swaps TODO |
| Tests | ⚠️ Unknown | 356 LOC, blocked by build |
| Anchor.toml | ✅ Ready | Devnet IDs configured |
| Dependencies | ✅ Ready | Anchor 0.30.1, Solana 1.18.26 |

---

## Fast Track Plan

### Phase 1: Fix Build (1 hour)
```bash
# Update these files to use Rust 1.85.0:
- BUILD.md (line 7, 107)
- README.md (line 217)
- .github/workflows/anchor.yml (line 26)
- .github/workflows/ci.yml (line 10)
- .github/workflows/rust.yml (line 19)

# Then build
anchor build
```

### Phase 2: Test (4-8 hours)
```bash
yarn install
anchor test
# Fix any failing tests
```

### Phase 3: Deploy (4 hours)
```bash
solana config set --url devnet
solana airdrop 10
anchor deploy --provider.cluster devnet
# Update Anchor.toml with real program IDs
# Rebuild and redeploy
```

### Phase 4: Initialize (2 hours)
```bash
# Initialize mode registry
# Add USDC/wSOL/WBTC modes
# Initialize staking pool
```

### Phase 5: Smoke Test (4 hours)
```bash
# Test provider deposit
# Test session creation
# Test permit redemption
# Test basic claim
```

**Total: 3-4 days**

---

## Decision Points

### 1. Rust Version
**Question:** Upgrade to 1.85.0 or downgrade to 1.79.0?  
**Recommendation:** Upgrade to 1.85.0 (already in toolchain file)  
**Rationale:** Future-proof, less work, edition2024 features available

### 2. Gateway Deployment
**Question:** Deploy with or without gateway?  
**Recommendation:** Deploy without gateway initially  
**Rationale:** 
- Core protocol works fine without it
- Gateway is convenience feature, not essential
- Reduces deployment risk
- Can add gateway later after thorough testing

### 3. Security Audit
**Question:** Need external audit before devnet?  
**Recommendation:** Internal review sufficient for devnet, external audit before mainnet  
**Rationale:** Devnet is for testing with play money

---

## What Users Can Do (Without Gateway)

✅ **Providers:**
1. Deposit collateral in any allowed mode (USDC/wSOL/WBTC)
2. Receive position NFT
3. Stake NFT for $ORIGIN rewards
4. Accept sessions
5. Redeem permits for payment

✅ **Users:**
1. Swap native→mode mint on Jupiter/Orca
2. Fund session escrow
3. Monitor session progress
4. File objective claims if provider fails

❌ **Cannot do (without gateway):**
- Atomic native→mode swap + escrow funding in single tx

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Build fails after Rust upgrade | Low | High | Test before commit |
| Tests reveal critical bugs | Medium | High | Fix before deploy |
| Gateway needed for UX | Low | Medium | Document manual workflow |
| Devnet instability | High | Low | Expected, handle gracefully |
| Program bugs in production | Medium | High | Thorough testing + audit |

---

## Success Metrics

Devnet deployment successful when:

- [ ] All 6 programs deployed and callable
- [ ] Mode registry initialized with 3 modes
- [ ] At least 1 provider successfully deposits collateral
- [ ] At least 1 user successfully creates session
- [ ] At least 1 permit successfully redeemed
- [ ] Documentation published
- [ ] No critical security issues

---

## Files to Review

1. **DEVNET_READINESS.md** - Full assessment (20 pages)
2. **DEVNET_ACTION_PLAN.md** - Step-by-step deployment guide (15 pages)
3. This file - Quick summary (you are here)

---

## Questions?

Open an issue in the repo with:
- Tag: `devnet-deployment`
- Include: Which blocker or phase you need help with

---

**Next Action:** Review with team, get approval, execute Phase 1
