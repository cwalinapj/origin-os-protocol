# Build Instructions

## Toolchain Requirements

| Component | Version | Notes |
|-----------|---------|-------|
| **Rust** | 1.85.0 | Pinned in `rust-toolchain.toml` |
| **Anchor** | 0.30.1 | CLI and framework |
| **Solana CLI** | 1.18.26 | Must match program dependencies |
| **Node.js** | 18+ | For TypeScript tests |
| **Yarn** | 1.x or 3.x | Package manager |

## Prerequisites

### 1. Install Rust

Rust will be automatically installed at the correct version via `rust-toolchain.toml`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Install Solana CLI (v1.18.26)

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/v1.18.26/install)"
```

Add to your PATH:
```bash
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
```

Verify installation:
```bash
solana --version
# solana-cli 1.18.26
```

### 3. Install Anchor CLI (v0.30.1)

```bash
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install 0.30.1
avm use 0.30.1
```

Verify installation:
```bash
anchor --version
# anchor-cli 0.30.1
```

### 4. Install Node.js Dependencies

```bash
yarn install
```

## Build

Build all programs:

```bash
anchor build
```

Build outputs are placed in `target/deploy/`.

## Test

Run the full test suite:

```bash
anchor test
```

This starts a local validator, deploys programs, and runs TypeScript tests.

## Deploy

### Devnet

```bash
# Configure for devnet
solana config set --url devnet

# Fund your wallet
solana airdrop 2

# Deploy
anchor deploy --provider.cluster devnet
```

### Mainnet

```bash
solana config set --url mainnet-beta
anchor deploy --provider.cluster mainnet
```

## Verify Versions

Check that your toolchain matches the pinned versions:

```bash
rustc --version        # rustc 1.85.0
solana --version       # solana-cli 1.18.26
anchor --version       # anchor-cli 0.30.1
```

## Troubleshooting

### Rust version mismatch

If you see Rust version errors, ensure `rustup` is using the project's toolchain:

```bash
cd /path/to/origin-os-protocol
rustup show
```

The output should show `1.85.0` as the active toolchain.

### Solana version mismatch

Anchor 0.30.1 is built against Solana 1.18.x. Using a different Solana CLI version may cause compatibility issues. Install the exact version:

```bash
solana-install init 1.18.26
```

### Build cache issues

If builds fail unexpectedly, clean and rebuild:

```bash
anchor clean
anchor build
```
