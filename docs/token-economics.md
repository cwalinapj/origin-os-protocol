# Token Economics

## $ORIGIN Token

The $ORIGIN token is the native token of the Origin OS Protocol, designed to incentivize provider participation, reward active collateral usage, and enable protocol governance.

## Supply & Distribution

**Total Supply**: 1,000,000,000 $ORIGIN (1 billion tokens)

### Distribution Breakdown

| Allocation | Amount | Percentage | Purpose |
|------------|--------|------------|---------|
| **Community Rewards** | 400,000,000 | 40% | Staking emissions for providers |
| **Development** | 200,000,000 | 20% | Core team and ongoing development |
| **Early Contributors** | 150,000,000 | 15% | Early supporters and advisors |
| **Liquidity** | 150,000,000 | 15% | DEX liquidity and market making |
| **Treasury** | 100,000,000 | 10% | Protocol reserves and emergency fund |

## Emission Schedule

### Community Rewards (Staking)

The 400M community rewards allocation is distributed over time through staking emissions:

**Emission Rate**: Configurable by governance, initially set to a linear decay model.

**Target Schedule** (illustrative):
- **Year 1**: 120M tokens (30% of allocation)
- **Year 2**: 80M tokens (20% of allocation)
- **Year 3**: 60M tokens (15% of allocation)
- **Year 4**: 60M tokens (15% of allocation)
- **Year 5+**: 80M tokens (20% of allocation) - long tail distribution

This front-loaded schedule incentivizes early provider participation while ensuring long-term sustainability.

## Staking Mechanics

### Position NFT Staking

Providers stake their Position NFTs (representing collateral deposits) to earn $ORIGIN emissions.

**Reward Weighting**:
```
weight = 0.80 × reserved_collateral + 0.20 × free_collateral
```

**Why This Formula?**
- **80% weight on reserved collateral**: Rewards providers actively securing sessions
- **20% weight on free collateral**: Rewards providers with available capacity
- **Anti-farming**: Prevents "deposit and forget" behavior by heavily weighting active usage

### Reward Distribution

Rewards are distributed proportionally based on each staker's weight relative to total staked weight:

```
user_share = user_weight / total_staked_weight
user_rewards = emission_rate × blocks_elapsed × user_share
```

**Example**:
- Provider A: 1000 USDC reserved, 500 USDC free → weight = 900
- Provider B: 500 USDC reserved, 2000 USDC free → weight = 800
- Total weight = 1700
- Provider A share = 900/1700 = 52.9%
- Provider B share = 800/1700 = 47.1%

If 1000 $ORIGIN tokens are emitted in an epoch:
- Provider A receives: 529 $ORIGIN
- Provider B receives: 471 $ORIGIN

### Dynamic Weight Updates

Provider weights update automatically when:
- New sessions open (reserved amount increases)
- Sessions close (reserved amount decreases)
- Collateral is deposited (total increases)
- Collateral is withdrawn (total decreases)

Providers should call `update_stake_weight()` after collateral changes to reflect updated weights.

## Token Utility

### 1. Staking Rewards
- Primary utility: Stake to earn emissions
- Higher rewards for active providers (more reserved collateral)

### 2. Protocol Fees (Future)
- Potential small protocol fee on session settlements
- Fee paid in session mint (USDC/wSOL/WBTC)
- Revenue used to buy back and burn $ORIGIN or distribute to stakers

### 3. Governance (Future)
- Vote on protocol parameters:
  - Emission rates
  - Mode additions/removals
  - Collateralization ratio adjustments
  - Insurance formula parameters

### 4. Gateway Swaps
- Users can swap $ORIGIN for session mints (USDC/wSOL/WBTC)
- Providers can swap $ORIGIN for collateral deposits
- Creates natural demand for $ORIGIN when users/providers prefer native token

## Vesting Schedules

### Development (20%)
- 1-year cliff
- 3-year linear vesting thereafter
- Ensures long-term alignment with protocol success

### Early Contributors (15%)
- 6-month cliff
- 2-year linear vesting thereafter
- Rewards early supporters while preventing immediate dumps

### Liquidity (15%)
- 50% unlocked at launch (for initial DEX liquidity)
- 50% vested over 1 year (for market making and additional liquidity)

### Treasury (10%)
- Fully unlocked but controlled by governance
- Used for:
  - Emergency insurance fund
  - Protocol development grants
  - Strategic partnerships
  - Market operations during extreme volatility

## Economic Incentives

### For Providers

**Revenue Sources**:
1. Session payments via permits (in USDC/wSOL/WBTC)
2. $ORIGIN staking emissions

**Costs**:
1. Collateral opportunity cost (locked capital)
2. Infrastructure costs (storage, bandwidth)
3. Slashing risk (if failing to deliver service)

**Profit Maximization**:
- Maximize reserved collateral to earn higher staking rewards
- Deliver reliable service to avoid slashing
- Optimize collateral efficiency across multiple sessions

### For Users

**Benefits**:
1. Pay-per-use pricing (via permits)
2. Insurance coverage from reserved collateral
3. Objective claim mechanisms (no disputes needed)

**Costs**:
1. Session escrow (prepayment)
2. Insurance premium (built into session cost)

**Value Proposition**:
- Trustless encrypted hosting without relying on centralized providers
- Automatic insurance and objective dispute resolution

## Token Flows

### Primary Market
```
$ORIGIN emissions → Staked providers → Service delivery → Users pay session mint
                                                            ↓
                                          Session mint → Providers (revenue)
```

### Secondary Market
```
$ORIGIN → DEX pools → USDC/wSOL/WBTC
       ↗            ↘
Providers        Users (gateway swaps)
```

### Buyback & Burn (Future)
```
Protocol fees (in session mints) → DEX swap → $ORIGIN → Burn
```

## Deflationary Mechanisms (Planned)

While the initial model is inflationary (due to emissions), future deflationary mechanisms may include:

1. **Fee Burns**: Protocol fees used to buy back and burn $ORIGIN
2. **Slashing Burns**: Slashed $ORIGIN collateral (if $ORIGIN mode added) could be burned instead of redistributed
3. **Governance Burns**: Community votes to burn treasury tokens

These mechanisms would be activated after sufficient liquidity and network effect are established.

## Collateral Modes Economics

### Why Multiple Mints?

Supporting multiple collateral mints provides:
- **User choice**: Users pay in their preferred stablecoin or wrapped asset
- **Provider flexibility**: Providers can choose their collateral asset based on risk tolerance
- **Market efficiency**: Arbitrage opportunities between modes keep them competitive

### Mode Selection Dynamics

**USDC Mode (150% CR)**:
- **Lower collateral requirement** → Providers can serve more users with same capital
- **Stable value** → Low slashing risk from price volatility
- **Ideal for**: Risk-averse providers, users preferring stablecoins

**wSOL Mode (175% CR)**:
- **Medium collateral requirement** → Balance between efficiency and risk
- **Native Solana token** → No bridging risk
- **Ideal for**: SOL-native users and providers

**WBTC Mode (200% CR)**:
- **Higher collateral requirement** → More conservative
- **Bridged asset** → Additional bridge risk
- **Premium pricing** → Providers can charge more to offset higher CR
- **Ideal for**: Users preferring Bitcoin exposure, providers with existing BTC holdings

## Launch Strategy

### Phase 1: Initial Distribution (Month 1-3)
1. Airdrop to early testers and contributors
2. Initial DEX offering (IDO) for liquidity
3. Team and advisor allocations begin vesting
4. Staking program launches with higher initial emission rate

### Phase 2: Network Growth (Month 4-12)
1. Community incentives for provider onboarding
2. User acquisition campaigns
3. Emission rate remains high to bootstrap supply side
4. Protocol fee mechanism introduced (if needed)

### Phase 3: Maturity (Year 2+)
1. Emission rate decreases according to schedule
2. Governance activation
3. Buyback and burn mechanisms (if protocol is profitable)
4. Additional token utility features based on community feedback

## Risk Mitigation

### Inflationary Pressure
- Front-loaded emissions incentivize early participation but create selling pressure
- **Mitigation**: Strong product-market fit drives utility demand to offset emissions

### Centralization Risk
- Large token holders (development, early contributors) could dominate governance
- **Mitigation**: Vesting schedules, community allocation >50% of total supply

### Slashing Risk
- Providers risk losing reserved collateral if they fail to deliver
- **Mitigation**: Clear service requirements, generous time windows, objective claim conditions

### Price Volatility
- Volatile collateral assets (wSOL, WBTC) require higher CR
- **Mitigation**: Conservative CR ratios, Pyth oracle integration, option to use stablecoins

## Metrics & KPIs

Key metrics for evaluating token economics health:

1. **Total Value Locked (TVL)**: Sum of all collateral across modes
2. **Active Sessions**: Number of concurrent sessions
3. **Utilization Rate**: Reserved / Total collateral ratio
4. **Staking Participation**: % of $ORIGIN staked
5. **Claim Rate**: % of sessions ending in slashing events
6. **Provider ROI**: Earnings (permits + emissions) vs collateral opportunity cost

## Future Considerations

### Cross-Chain Expansion
If the protocol expands to other chains:
- $ORIGIN could be bridged (canonical token on Solana)
- Each chain maintains its own collateral vaults
- Staking rewards distributed proportionally across chains

### Reputation System
A future reputation system could:
- Boost rewards for providers with low claim rates
- Reduce required collateral for trusted providers
- Create tiered service levels

### Dynamic Emission Rates
Governance could implement dynamic emissions based on:
- Network utilization (increase rewards during low utilization)
- TVL targets (adjust to hit TVL goals)
- Competitive landscape (respond to competing protocols)

---

## Conclusion

The $ORIGIN token economics are designed to:
1. **Bootstrap supply**: High initial emissions attract providers
2. **Reward activity**: Weighting formula favors active providers over passive stakers
3. **Sustain long-term**: Gradual emission decay ensures decades of incentives
4. **Enable governance**: Token holders can guide protocol evolution
5. **Create value**: Multiple utility sinks (staking, swaps, governance, future fees)

The ultimate success depends on achieving product-market fit: if the protocol provides valuable hosting services, demand for $ORIGIN (for swaps and staking) should grow alongside the token supply from emissions.
