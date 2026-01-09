# Agent Rules

Rules for AI agents (Claude, Copilot, etc.) working on Origin OS Protocol.

## Critical Rule: Security-Sensitive Code

**DO NOT modify the following without explicit human approval via a Security PR:**

- `programs/session_escrow/` - Escrow, vault interactions, claims, permit verification
- `programs/collateral_vault/` - Collateral custody, slashing
- `programs/mode_registry/` - Mode config, verifier allowlist

### What This Means

1. **No autonomous changes** to fund custody logic
2. **No autonomous changes** to signature verification
3. **No autonomous changes** to payout/slashing calculations
4. **No autonomous changes** to account constraints or PDA seeds

### If Asked to Modify Security Code

When asked to modify security-critical code:

1. **STOP** - Do not make the change autonomously
2. **EXPLAIN** - Describe what change is needed and why
3. **DRAFT** - Provide the code as a suggestion only
4. **REQUIRE** - Request human review via Security PR process

## Safe Zones

Agents MAY freely modify:

| Component | Notes |
|-----------|-------|
| `programs/gateway/` | Stub-only until pyth_helpers tests pass |
| `programs/pyth_helpers/` | Price utilities and tests |
| `tests/` | All test files |
| `*.md` | Documentation |
| `.github/` | CI/CD workflows |
| `Cargo.toml` | Dependency management (version bumps need verification) |

## Code Quality Rules

### 1. Tests First

- Add tests before implementing features
- Ensure existing tests pass before committing
- pyth_helpers must have coverage for:
  - Stale price rejection
  - Confidence ratio rejection
  - Conservative min_out calculations

### 2. No Stub Removal Without Tests

The gateway program is **stub-only**. Do not implement actual swap CPI until:
- pyth_helpers test suite is complete and passing
- Price validation is thoroughly tested

### 3. Version Pinning

- Never use `latest` or `*` for versions
- All toolchain versions are pinned (see BUILD.md)
- Dependency updates require compatibility check

## Commit Guidelines

1. **Atomic commits** - One logical change per commit
2. **Descriptive messages** - Explain what and why
3. **No secrets** - Never commit keys, credentials, or .env files
4. **Run checks** - Build must pass before commit

## When in Doubt

If unsure whether a change is security-sensitive:

1. **Assume it is** - Err on the side of caution
2. **Ask** - Request clarification from humans
3. **Document** - Note your uncertainty in the PR

## Security PR Checklist

For any security-sensitive change:

- [ ] Change is necessary and minimal
- [ ] No new attack vectors introduced
- [ ] Fund flows are preserved correctly
- [ ] Signature verification unchanged or strengthened
- [ ] All edge cases handled
- [ ] Tests cover the change
- [ ] Two security reviewers assigned
