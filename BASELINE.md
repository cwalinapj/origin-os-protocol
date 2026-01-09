# Baseline Rules

This document defines the immutable security baseline for Origin OS Protocol.

## Security-Critical Components

The following components are **security-critical** and require elevated review processes:

| Component | Path | Description |
|-----------|------|-------------|
| **Session Escrow** | `programs/session_escrow/` | User fund custody, permit verification, claims |
| **Collateral Vault** | `programs/collateral_vault/` | Provider collateral management, slashing |
| **Mode Registry** | `programs/mode_registry/` | Mode configuration, verifier allowlist |

## Immutability Rules

### 1. Escrow Logic

**DO NOT MODIFY** without an approval-only Security PR:
- `open_session()` fund custody logic
- `redeem_permit()` signature verification
- `claim_*()` payout calculations
- Reserve accounting (`reserve_r`, `reserve_base`, `reserve_bid`)

### 2. Vault Logic

**DO NOT MODIFY** without an approval-only Security PR:
- `deposit_collateral()` / `withdraw_collateral()` balance tracking
- `slash()` amount calculations
- Insurance fund accounting

### 3. Permit Verification

**DO NOT MODIFY** without an approval-only Security PR:
- Ed25519 signature verification
- Nonce validation
- Permit message encoding

### 4. Claims Processing

**DO NOT MODIFY** without an approval-only Security PR:
- SLA failure payout calculations
- Coverage percentage math
- Reserve deduction logic

## What Requires a Security PR

A **Security PR** is required for any change that:

1. Modifies fund transfer logic
2. Changes signature or permit verification
3. Alters slashing or payout calculations
4. Modifies account validation constraints
5. Changes PDA derivation seeds
6. Adds new authorities or signers
7. Modifies the verifier allowlist logic

## Security PR Process

1. **Label**: PR must have `security` label
2. **Reviewers**: Minimum 2 approvals from security-designated reviewers
3. **No force merge**: Must not bypass required reviews
4. **Audit trail**: All discussions must be resolved before merge

## Safe Modification Zones

The following can be modified with standard PR review:

- `programs/gateway/` - Swap routing (currently stub-only)
- `programs/pyth_helpers/` - Price feed utilities
- Test files (`tests/`)
- Documentation (`*.md`)
- CI/CD workflows (`.github/`)

## Version Constraints

All dependency versions are pinned. Version bumps require:

1. Compatibility verification
2. Security advisory check
3. Full test suite pass

See [BUILD.md](./BUILD.md) for current pinned versions.
