# Devnet Deployment: Decision Matrix

This document helps stakeholders make informed decisions about the deployment approach.

---

## Option A: Fast Track (Without Gateway)

### Deployment Scope
- ✅ mode_registry
- ✅ collateral_vault
- ✅ session_escrow
- ✅ staking_rewards
- ✅ naked_staking
- ✅ pyth_helpers
- ❌ gateway (deferred)

### Timeline
| Phase | Duration |
|-------|----------|
| Fix Rust version | 1 hour |
| Build & test | 4-8 hours |
| Deploy | 4 hours |
| Initialize | 2 hours |
| Smoke test | 4 hours |
| Documentation | 4 hours |
| **Total** | **3-4 days** |

### Pros
- ✅ Fast time to market (3-4 days)
- ✅ Lower risk (simpler deployment)
- ✅ Core functionality 100% working
- ✅ Users can still swap + fund (just not atomic)
- ✅ Gateway can be added later after thorough testing
- ✅ Easier to troubleshoot with fewer moving parts

### Cons
- ❌ Users must perform 2 separate transactions
- ❌ Slightly worse UX (manual swap then fund)
- ❌ No atomic native→mode conversion
- ❌ Requires documentation of manual workflow

### User Experience
```
Without Gateway:
1. User swaps $ORIGIN → USDC on Jupiter (1 tx)
2. User calls session_escrow.fund_session() (1 tx)
Total: 2 transactions

Manual steps but simple and reliable.
```

### Risk Level: 🟢 LOW
- Deploying battle-tested core programs only
- No experimental swap integration
- Easy rollback if issues found

---

## Option B: Full Deployment (With Gateway)

### Deployment Scope
- ✅ mode_registry
- ✅ collateral_vault
- ✅ session_escrow
- ✅ staking_rewards
- ✅ naked_staking
- ✅ pyth_helpers
- ✅ gateway (completed first)

### Timeline
| Phase | Duration |
|-------|----------|
| Fix Rust version | 1 hour |
| Complete gateway implementation | 5-7 days |
| Gateway testing | 2 days |
| Build & test all | 4-8 hours |
| Code review | 1 day |
| Deploy | 4 hours |
| Initialize | 2 hours |
| Smoke test | 6 hours |
| Documentation | 4 hours |
| **Total** | **10-12 days** |

### Pros
- ✅ Full featured from day 1
- ✅ Best possible UX (atomic swaps)
- ✅ One transaction for native→mode + fund
- ✅ Complete product showcase

### Cons
- ❌ Longer development time (10-12 days)
- ❌ Higher complexity
- ❌ More potential for bugs
- ❌ Harder to troubleshoot multi-program issues
- ❌ Gateway needs integration with external DEX (Jupiter/Orca)
- ❌ More thorough testing required

### User Experience
```
With Gateway:
1. User calls gateway.swap_and_fund_session() (1 tx)
   - Swaps $ORIGIN → USDC (internal)
   - Funds escrow (CPI to session_escrow)
Total: 1 transaction

Seamless experience, zero manual steps.
```

### Risk Level: 🟡 MEDIUM
- New code needs testing
- DEX integration points of failure
- Pyth oracle dependency
- More complex rollback if issues

---

## Comparison Matrix

| Factor | Fast Track | Full Deploy |
|--------|-----------|-------------|
| **Time to Devnet** | 3-4 days | 10-12 days |
| **Development Effort** | Minimal | Significant |
| **Risk** | Low | Medium |
| **Core Features** | 100% | 100% |
| **UX Quality** | Good | Excellent |
| **Testing Complexity** | Simple | Complex |
| **Rollback Difficulty** | Easy | Hard |
| **Future Gateway Addition** | Yes, anytime | N/A |
| **User Tx Count** | 2 | 1 |
| **Bug Surface Area** | Small | Large |

---

## Gateway Implementation Details

### What Needs to Be Built
1. **Conservative Min-Out Calculation**
   - Use Pyth price feeds
   - Apply slippage tolerance
   - Account for fees
   - Code: ~50 lines

2. **DEX Swap CPI**
   - Jupiter Aggregator integration
   - Or Orca/Raydium direct integration
   - Account validation
   - Code: ~100 lines

3. **Post-Swap Routing**
   - CPI to session_escrow.fund_session()
   - Or CPI to collateral_vault.deposit()
   - Error handling
   - Code: ~50 lines

4. **Comprehensive Testing**
   - Unit tests for price calculation
   - Integration tests with mock DEX
   - Devnet tests with real Jupiter
   - Edge case handling
   - Code: ~200 lines

**Total New Code:** ~400 lines  
**Development Time:** 5-7 days  
**Testing Time:** 2 days  

---

## Technical Debt Comparison

### Option A (Fast Track)
**Debt Incurred:**
- Gateway deferred to later
- Manual workflow documentation needed

**Debt Payoff:**
- Gateway can be added anytime
- No impact on existing deployments
- Can be tested thoroughly in isolation

**Debt Level:** 🟢 LOW

### Option B (Full Deploy)
**Debt Incurred:**
- Rushed gateway implementation risk
- Less time for thorough testing
- Potential bugs in production

**Debt Payoff:**
- May need emergency patches
- Harder to fix with users on platform
- Could delay mainnet

**Debt Level:** 🟡 MEDIUM

---

## Security Implications

### Option A (Fast Track)
**Attack Surface:**
- 6 programs, all well-understood
- No complex cross-program flows beyond existing CPIs
- Pyth helpers used for validation only

**Security Review Needed:**
- ✅ mode_registry (governance logic)
- ✅ collateral_vault (custody + slashing)
- ✅ session_escrow (permits + claims)
- ⏸️ gateway (deferred)

**Risk:** 🟢 LOW - battle-tested core only

### Option B (Full Deploy)
**Attack Surface:**
- 7 programs with new gateway integration
- Complex swap routing logic
- External DEX dependency
- Price oracle manipulation risk

**Security Review Needed:**
- ✅ mode_registry
- ✅ collateral_vault
- ✅ session_escrow
- ❗ gateway (NEW - needs thorough audit)

**Risk:** 🟡 MEDIUM - new code, more integrations

---

## Cost Analysis

### Development Cost

| Task | Fast Track | Full Deploy |
|------|-----------|-------------|
| Rust version fix | 1 hour | 1 hour |
| Testing existing code | 4-8 hours | 4-8 hours |
| Gateway implementation | 0 | 40-56 hours |
| Gateway testing | 0 | 16 hours |
| Integration testing | 4 hours | 6 hours |
| Documentation | 4 hours | 6 hours |
| **Total Dev Hours** | **13-17 hours** | **67-77 hours** |
| **Total Calendar Days** | **3-4 days** | **10-12 days** |

### Deployment Cost (Devnet)
Both options: ~FREE (devnet SOL from faucet)

### Opportunity Cost
- **Fast Track:** Get user feedback 1 week sooner
- **Full Deploy:** Delayed feedback, but better initial impression

---

## Recommendation Matrix

### Choose Fast Track IF:
- ✅ Speed to market is priority
- ✅ Core functionality validation needed ASAP
- ✅ Team bandwidth is limited
- ✅ Risk tolerance is low
- ✅ Gateway can be added later without disruption

### Choose Full Deploy IF:
- ✅ UX perfection is critical for devnet
- ✅ Team has 10+ days available
- ✅ Gateway is essential for testing thesis
- ✅ Risk tolerance is higher
- ✅ Complete feature set needed for demo/partners

---

## Hybrid Option C: Phased Deployment

### Phase 1 (Days 1-4): Core Programs
Deploy 6 core programs, document manual workflow.

### Phase 2 (Days 5-7): Gateway Development  
Build and test gateway in parallel to devnet usage.

### Phase 3 (Days 8-9): Gateway Deployment
Deploy gateway as upgrade once ready.

### Benefits
- ✅ Get devnet live quickly
- ✅ Users can start testing immediately
- ✅ Gateway development not rushed
- ✅ Can gather feedback before gateway launch
- ✅ Lower risk approach

### Drawbacks
- ❌ Requires 2 deployment cycles
- ❌ Documentation needs updating twice
- ❌ Users experience 2 different UX patterns

**Risk Level:** 🟢 LOW  
**Recommended:** ✅ YES

---

## Final Recommendation

### Recommended Approach: **Hybrid Option C**

**Rationale:**
1. Deploy core programs first (days 1-4)
2. Get users testing immediately
3. Develop gateway properly (days 5-7)
4. Deploy gateway when ready (days 8-9)
5. Best of both worlds: speed + quality

### Execution Steps

**Week 1:**
- Fix Rust version (day 1)
- Test and deploy core 6 programs (days 2-4)
- Users start testing with manual workflow
- Begin gateway development (days 5-7)

**Week 2:**
- Complete gateway implementation (days 8-9)
- Test gateway thoroughly
- Deploy gateway to devnet (day 10)
- Update documentation
- Announce gateway availability

**Benefits:**
- ✅ Devnet live in 4 days
- ✅ Gateway ready in 10 days (but not blocking)
- ✅ Lower risk throughout
- ✅ Iterative feedback loop
- ✅ No compromises on quality

---

## Stakeholder Sign-Off

| Role | Fast Track | Full Deploy | Hybrid | Signature |
|------|-----------|-------------|---------|-----------|
| Technical Lead | ☐ | ☐ | ☐ | _________ |
| Product Manager | ☐ | ☐ | ☐ | _________ |
| Security Lead | ☐ | ☐ | ☐ | _________ |
| Project Manager | ☐ | ☐ | ☐ | _________ |

**Decision Date:** _________  
**Selected Option:** _________  
**Expected Completion:** _________

---

## Next Actions After Decision

Once option is selected:

1. [ ] Update project board with selected approach
2. [ ] Assign team members to tasks
3. [ ] Create detailed task breakdown
4. [ ] Set up monitoring for devnet
5. [ ] Begin Phase 1 execution
6. [ ] Schedule daily standups during deployment
7. [ ] Prepare rollback procedures

---

**Document Version:** 1.0  
**Last Updated:** 2026-01-09  
**Status:** Awaiting decision
