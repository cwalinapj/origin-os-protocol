# Deployment Guide

This guide covers deploying the Origin OS Protocol programs to Solana networks (devnet, testnet, mainnet-beta).

## Prerequisites

Ensure you have completed the setup described in [BUILD.md](../BUILD.md):
- Rust 1.79.0
- Anchor CLI 0.30.1
- Solana CLI 1.18.26
- Successful local build (`anchor build`)

## Pre-Deployment Checklist

- [ ] All programs build successfully
- [ ] All tests pass (`anchor test`)
- [ ] Program keypairs generated or imported
- [ ] Deployment wallet funded with sufficient SOL
- [ ] Network configuration confirmed
- [ ] Program sizes within limits (10MB max per program)

## Network Configuration

### Devnet

```bash
# Configure Solana CLI
solana config set --url devnet

# Verify configuration
solana config get
# Expected: RPC URL: https://api.devnet.solana.com

# Fund your wallet
solana airdrop 2
solana balance
```

### Testnet

```bash
solana config set --url testnet
solana airdrop 2
```

### Mainnet-Beta

```bash
solana config set --url mainnet-beta

# Fund your wallet (real SOL required)
# Transfer SOL from an exchange or another wallet
solana balance
```

## Program Keypairs

Program keypairs define the on-chain addresses of your programs.

### Option 1: Generate New Keypairs

```bash
# Generate keypairs for all programs
solana-keygen new -o target/deploy/mode_registry-keypair.json
solana-keygen new -o target/deploy/collateral_vault-keypair.json
solana-keygen new -o target/deploy/session_escrow-keypair.json
solana-keygen new -o target/deploy/staking_rewards-keypair.json
solana-keygen new -o target/deploy/pyth_helpers-keypair.json
solana-keygen new -o target/deploy/gateway-keypair.json
```

### Option 2: Use Existing Keypairs

If you have existing program keypairs (e.g., from a previous deployment), place them in `target/deploy/` with the naming convention: `<program_name>-keypair.json`.

### Update Anchor.toml

Edit `Anchor.toml` to reference your program IDs:

```toml
[programs.localnet]
mode_registry = "YOUR_MODE_REGISTRY_PUBKEY"
collateral_vault = "YOUR_COLLATERAL_VAULT_PUBKEY"
session_escrow = "YOUR_SESSION_ESCROW_PUBKEY"
staking_rewards = "YOUR_STAKING_REWARDS_PUBKEY"
pyth_helpers = "YOUR_PYTH_HELPERS_PUBKEY"
gateway = "YOUR_GATEWAY_PUBKEY"

[programs.devnet]
# Same as above for devnet deployment

[programs.mainnet]
# Same as above for mainnet deployment
```

To get pubkeys from keypairs:

```bash
solana-keygen pubkey target/deploy/mode_registry-keypair.json
```

## Deployment Steps

### 1. Build Programs

```bash
anchor build
```

Verify program sizes:

```bash
ls -lh target/deploy/*.so
```

Each program must be ≤10MB.

### 2. Deploy to Devnet

```bash
anchor deploy --provider.cluster devnet
```

This will:
1. Upload each program's `.so` file to the network
2. Create or upgrade program accounts
3. Set the upgrade authority to your wallet

**Expected Output**:
```
Deploying workspace: https://api.devnet.solana.com
Upgrade authority: <YOUR_WALLET_PUBKEY>
Deploying program "mode_registry"...
Program Id: <PROGRAM_ID>
...
Deploy success
```

### 3. Verify Deployment

```bash
# Check program accounts exist
solana program show <PROGRAM_ID> --url devnet

# Verify upgrade authority
solana program show <PROGRAM_ID> --url devnet | grep "Authority"
```

## Program Initialization

After deploying the programs, they must be initialized with their configuration.

### 1. Initialize Mode Registry

The `mode_registry` must be initialized before other programs can interact with modes.

**Using Anchor CLI** (adjust for your setup):

```bash
anchor run initialize-registry
```

Or via TypeScript client:

```typescript
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ModeRegistry } from "../target/types/mode_registry";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.ModeRegistry as Program<ModeRegistry>;

await program.methods
  .initialize()
  .accounts({
    registry: registryPda,
    admin: provider.wallet.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .rpc();
```

### 2. Add Modes

Add each collateral mode (USDC, wSOL, WBTC):

```typescript
// Example: Add USDC mode
const usdcMint = new PublicKey("USDC_MINT_ADDRESS");

await program.methods
  .addMode({
    mint: usdcMint,
    crBps: 15000, // 150%
    pMin: new anchor.BN(100_000000), // 100 USDC (6 decimals)
    pCap: new anchor.BN(10000_000000), // 10,000 USDC
    a: new anchor.BN(1000), // 0.10 in basis points (1000/10000)
    b: new anchor.BN(2_000000), // 2 USDC
  })
  .accounts({
    registry: registryPda,
    mode: modePda,
    admin: provider.wallet.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .rpc();

// Wait 24 hours for timelock...

// Activate mode
await program.methods
  .activateMode(modeId)
  .accounts({
    registry: registryPda,
    mode: modePda,
    admin: provider.wallet.publicKey,
  })
  .rpc();
```

**Note**: New modes have a 24-hour timelock before activation.

### 3. Initialize Staking Pool

```typescript
const stakingProgram = anchor.workspace.StakingRewards as Program<StakingRewards>;

await stakingProgram.methods
  .initializePool({
    emissionRate: new anchor.BN(1000), // 1000 tokens per slot (adjust as needed)
  })
  .accounts({
    pool: poolPda,
    originMint: originMintPubkey,
    authority: provider.wallet.publicKey,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .rpc();
```

## Post-Deployment Verification

### Test Basic Flows

1. **Provider Deposit**:
   ```typescript
   // Test collateral deposit
   await collateralVaultProgram.methods
     .deposit(new anchor.BN(1000_000000)) // 1000 USDC
     .accounts({
       providerPosition: positionPda,
       provider: provider.wallet.publicKey,
       // ... other accounts
     })
     .rpc();
   ```

2. **Open Session**:
   ```typescript
   // Test session opening
   await sessionEscrowProgram.methods
     .openSession({
       provider: providerPubkey,
       modeId: 0, // USDC
       maxSpend: new anchor.BN(100_000000), // 100 USDC
       // ... other params
     })
     .accounts({
       session: sessionPda,
       user: provider.wallet.publicKey,
       // ... other accounts
     })
     .rpc();
   ```

3. **Stake Position**:
   ```typescript
   // Test position staking
   await stakingRewardsProgram.methods
     .stakePosition()
     .accounts({
       stakeAccount: stakeAccountPda,
       position: positionPda,
       owner: provider.wallet.publicKey,
       // ... other accounts
     })
     .rpc();
   ```

### Monitor On-Chain State

Use Solana explorers to verify:
- **Devnet**: https://explorer.solana.com/?cluster=devnet
- **Mainnet**: https://explorer.solana.com/

Search for your program IDs and verify:
- Program accounts exist
- Upgrade authority is correct
- Initial state is as expected

## Upgrading Programs

### Upgradeable Programs

Only `mode_registry` is upgradeable. To upgrade:

```bash
# Build new version
anchor build

# Upgrade specific program
anchor upgrade target/deploy/mode_registry.so \
  --program-id <MODE_REGISTRY_PROGRAM_ID> \
  --provider.cluster devnet
```

### Immutable Programs

`collateral_vault`, `session_escrow`, `staking_rewards`, `pyth_helpers`, and `gateway` are immutable and cannot be upgraded.

To make a program immutable after deployment:

```bash
solana program set-upgrade-authority <PROGRAM_ID> --final --url devnet
```

**Warning**: This is irreversible. Ensure the program is thoroughly tested before making it immutable.

## Cost Estimation

### Deployment Costs (Devnet)

Devnet uses airdropped SOL (free). Actual costs:
- Program deployment: ~0.01 SOL per MB
- Account rent: Varies by account size
- Transaction fees: ~0.000005 SOL per transaction

### Mainnet Costs (Estimated)

Based on typical program sizes:
- Deploying 6 programs (~5MB total): ~0.05-0.1 SOL
- Mode registry initialization: ~0.001 SOL
- Adding modes (3 modes): ~0.003 SOL
- Staking pool initialization: ~0.001 SOL

**Total estimated**: ~0.1-0.2 SOL + buffer for rent and fees

**Recommendation**: Have at least 0.5 SOL in your deployment wallet for mainnet.

## Security Considerations

### Before Mainnet Deployment

- [ ] Complete security audit of all programs
- [ ] Test all program interactions on devnet/testnet extensively
- [ ] Verify upgrade authorities are correctly set
- [ ] Set immutable programs to `--final` upgrade authority
- [ ] Test emergency procedures (mode disabling, parameter updates)
- [ ] Document all program addresses and authorities

### Upgrade Authority Management

**Best Practices**:
1. Use a multisig wallet for upgrade authority (e.g., Squads Protocol)
2. Never store upgrade authority keypairs in plain text
3. Implement timelocks for upgrades (using DAO or multisig with delay)
4. Communicate upgrade plans to community in advance

### Mode Registry Admin

The `mode_registry` admin has significant power:
- Add new modes
- Disable modes
- Update mode parameters

**Recommendations**:
1. Transfer admin to a DAO or multisig immediately after initial setup
2. Implement governance proposals for mode changes
3. Require community approval for risky parameter changes

## Troubleshooting

### Deployment Fails: "Insufficient funds"

```bash
# Check balance
solana balance

# Request airdrop (devnet only)
solana airdrop 2

# Transfer funds (mainnet)
solana transfer <YOUR_WALLET> <AMOUNT>
```

### Deployment Fails: "Program data too large"

Program exceeds 10MB limit. Optimize:
- Remove debug symbols: Use `--release` build
- Remove unused dependencies
- Split large programs into smaller modules

### Transaction Timeout

Network congestion or RPC issues. Solutions:
- Retry with higher priority fee: `--with-compute-unit-price`
- Use a different RPC endpoint
- Try at a less congested time

### Program Not Upgrading

Ensure you're the upgrade authority:

```bash
solana program show <PROGRAM_ID> | grep "Authority"
```

If authority is different, you cannot upgrade. Contact the authority holder.

## Mainnet Deployment Checklist

Before deploying to mainnet:

- [ ] All devnet testing completed successfully
- [ ] Security audit completed and issues resolved
- [ ] All program sizes optimized and within limits
- [ ] Program keypairs securely generated and backed up
- [ ] Deployment wallet funded with sufficient SOL (0.5+ SOL recommended)
- [ ] Upgrade authorities configured (multisig for mode_registry)
- [ ] Emergency procedures documented and tested
- [ ] Community notified of upcoming deployment
- [ ] Immutable programs set to `--final` after verification
- [ ] Initial modes added and activated
- [ ] Staking pool initialized with correct emission rate
- [ ] Integration tests run against mainnet programs
- [ ] Monitoring and alerting set up for program activity
- [ ] Documentation updated with mainnet addresses

## Monitoring & Maintenance

### On-Chain Monitoring

Set up monitoring for:
- Program invocations (sessions opened, collateral deposited, etc.)
- Claims (slashing events)
- Staking activity
- Mode additions/changes

**Tools**:
- Solana Explorer
- Custom indexers (e.g., using Geyser plugin)
- The Graph subgraphs

### Incident Response

**If a vulnerability is discovered**:
1. Assess severity and exploitability
2. Disable affected modes if possible (`disable_mode`)
3. Communicate with users and providers
4. Prepare and test a fix
5. Deploy upgrade (mode_registry only) or deploy new program versions
6. Migrate users to new programs if needed

**For immutable programs**: Vulnerabilities cannot be patched. Options:
- Deploy new program versions with fixes
- Implement mitigation strategies in upgradeable programs
- Coordinate community migration to fixed versions

## References

- [Anchor Documentation](https://www.anchor-lang.com/)
- [Solana Cookbook](https://solanacookbook.com/)
- [Solana Program Deployment Guide](https://docs.solana.com/cli/deploy-a-program)
- [Squads Protocol (Multisig)](https://squads.so/)

---

For questions or issues, please open an issue on the GitHub repository.
