use anchor_lang::prelude::*;

declare_id!("ModeReg111111111111111111111111111111111111");

/// Maximum number of verifiers in the allowlist
pub const MAX_VERIFIERS: usize = 10;

/// Mode Registry Program
///
/// Manages allowlist of collateral/payment mints with per-mint parameters.
/// Also manages the verifier allowlist for SLA attestations.
/// This is the ONLY upgradeable program in the suite.
///
/// INVARIANT: Registry changes must NEVER allow admin to move user/provider escrows.
#[program]
pub mod mode_registry {
    use super::*;

    /// Initialize the registry with an admin authority
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        registry.admin = ctx.accounts.admin.key();
        registry.mode_count = 0;
        registry.verifier_count = 0;
        registry.verifiers = [Pubkey::default(); MAX_VERIFIERS];
        registry.bump = ctx.bumps.registry;

        emit!(RegistryInitialized {
            admin: registry.admin,
        });

        Ok(())
    }

    /// Add a new collateral mode (admin only)
    ///
    /// # Arguments
    /// * `mode_id` - Unique identifier for this mode
    /// * `cr_bps` - Collateral ratio in basis points (e.g., 15000 = 150%)
    /// * `per_provider_cap` - Maximum collateral per provider in this mode
    /// * `global_cap` - Maximum total collateral across all providers
    /// * `activation_slot` - Slot after which this mode becomes active (timelock)
    pub fn add_mode(
        ctx: Context<AddMode>,
        mode_id: u32,
        cr_bps: u16,
        per_provider_cap: u64,
        global_cap: u64,
        activation_slot: u64,
    ) -> Result<()> {
        require!(cr_bps >= 10000, ErrorCode::CollateralRatioTooLow); // Min 100%
        require!(cr_bps <= 50000, ErrorCode::CollateralRatioTooHigh); // Max 500%

        let mode = &mut ctx.accounts.mode;
        mode.mode_id = mode_id;
        mode.mint = ctx.accounts.mint.key();
        mode.cr_bps = cr_bps;
        mode.per_provider_cap = per_provider_cap;
        mode.global_cap = global_cap;
        mode.global_deposited = 0;
        mode.activation_slot = activation_slot;
        mode.is_active = false; // Must be activated after timelock
        mode.is_disabled = false;
        mode.bump = ctx.bumps.mode;

        let registry = &mut ctx.accounts.registry;
        registry.mode_count = registry.mode_count.checked_add(1).unwrap();

        emit!(ModeAdded {
            mode_id,
            mint: mode.mint,
            cr_bps,
            activation_slot,
        });

        Ok(())
    }

    /// Activate a mode after its timelock has passed
    pub fn activate_mode(ctx: Context<ActivateMode>) -> Result<()> {
        let mode = &mut ctx.accounts.mode;
        let clock = Clock::get()?;

        require!(!mode.is_active, ErrorCode::ModeAlreadyActive);
        require!(!mode.is_disabled, ErrorCode::ModeDisabled);
        require!(
            clock.slot >= mode.activation_slot,
            ErrorCode::TimelockNotPassed
        );

        mode.is_active = true;

        emit!(ModeActivated {
            mode_id: mode.mode_id,
            activated_at_slot: clock.slot,
        });

        Ok(())
    }

    /// Disable a mode (admin only)
    ///
    /// This only blocks new sessions/deposits. Existing funds remain accessible.
    /// INVARIANT: Cannot seize any user or provider funds.
    pub fn disable_mode(ctx: Context<DisableMode>) -> Result<()> {
        let mode = &mut ctx.accounts.mode;

        require!(!mode.is_disabled, ErrorCode::ModeAlreadyDisabled);

        mode.is_disabled = true;
        mode.is_active = false;

        emit!(ModeDisabled {
            mode_id: mode.mode_id,
        });

        Ok(())
    }

    /// Update mode parameters (admin only, with restrictions)
    ///
    /// Can only tighten parameters (increase CR, decrease caps)
    pub fn update_mode_params(
        ctx: Context<UpdateModeParams>,
        new_cr_bps: Option<u16>,
        new_per_provider_cap: Option<u64>,
        new_global_cap: Option<u64>,
    ) -> Result<()> {
        let mode = &mut ctx.accounts.mode;

        // Can only increase CR (more conservative)
        if let Some(cr) = new_cr_bps {
            require!(cr >= mode.cr_bps, ErrorCode::CannotReduceCollateralRatio);
            require!(cr <= 50000, ErrorCode::CollateralRatioTooHigh);
            mode.cr_bps = cr;
        }

        // Can only decrease caps (more restrictive)
        if let Some(cap) = new_per_provider_cap {
            require!(cap <= mode.per_provider_cap, ErrorCode::CannotIncreaseCap);
            mode.per_provider_cap = cap;
        }

        if let Some(cap) = new_global_cap {
            require!(cap <= mode.global_cap, ErrorCode::CannotIncreaseCap);
            require!(cap >= mode.global_deposited, ErrorCode::CapBelowDeposited);
            mode.global_cap = cap;
        }

        emit!(ModeParamsUpdated {
            mode_id: mode.mode_id,
            cr_bps: mode.cr_bps,
            per_provider_cap: mode.per_provider_cap,
            global_cap: mode.global_cap,
        });

        Ok(())
    }

    /// Add a verifier to the allowlist (admin only)
    ///
    /// Verifiers are trusted parties that can submit latency attestations
    /// for SLA evaluation in bid sessions.
    pub fn add_verifier(ctx: Context<AddVerifier>, verifier: Pubkey) -> Result<()> {
        let registry = &mut ctx.accounts.registry;

        require!(
            (registry.verifier_count as usize) < MAX_VERIFIERS,
            ErrorCode::MaxVerifiersReached
        );

        // Check if verifier already exists
        for i in 0..registry.verifier_count as usize {
            require!(
                registry.verifiers[i] != verifier,
                ErrorCode::VerifierAlreadyExists
            );
        }

        let idx = registry.verifier_count as usize;
        registry.verifiers[idx] = verifier;
        registry.verifier_count = registry.verifier_count.checked_add(1).unwrap();

        emit!(VerifierAdded { verifier });

        Ok(())
    }

    /// Remove a verifier from the allowlist (admin only)
    pub fn remove_verifier(ctx: Context<RemoveVerifier>, verifier: Pubkey) -> Result<()> {
        let registry = &mut ctx.accounts.registry;

        let mut found_index: Option<usize> = None;
        for i in 0..registry.verifier_count as usize {
            if registry.verifiers[i] == verifier {
                found_index = Some(i);
                break;
            }
        }

        let index = found_index.ok_or(ErrorCode::VerifierNotFound)?;

        // Shift remaining verifiers down
        for i in index..(registry.verifier_count as usize - 1) {
            registry.verifiers[i] = registry.verifiers[i + 1];
        }

        // Clear the last slot
        let idx = registry.verifier_count as usize - 1;
        registry.verifiers[idx] = Pubkey::default();
        registry.verifier_count = registry.verifier_count.checked_sub(1).unwrap();

        emit!(VerifierRemoved { verifier });

        Ok(())
    }

    /// Check if a pubkey is an authorized verifier
    pub fn is_verifier(ctx: Context<IsVerifier>, verifier: Pubkey) -> Result<bool> {
        let registry = &ctx.accounts.registry;

        for i in 0..registry.verifier_count as usize {
            if registry.verifiers[i] == verifier {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Transfer admin authority to new address
    pub fn transfer_admin(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
        let registry = &mut ctx.accounts.registry;
        let old_admin = registry.admin;
        registry.admin = new_admin;

        emit!(AdminTransferred {
            old_admin,
            new_admin,
        });

        Ok(())
    }
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + Registry::INIT_SPACE,
        seeds = [b"registry"],
        bump
    )]
    pub registry: Account<'info, Registry>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(mode_id: u32)]
pub struct AddMode<'info> {
    #[account(
        mut,
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        init,
        payer = admin,
        space = 8 + Mode::INIT_SPACE,
        seeds = [b"mode", &mode_id.to_le_bytes()],
        bump
    )]
    pub mode: Account<'info, Mode>,

    /// The SPL token mint for this mode
    pub mint: Account<'info, anchor_spl::token::Mint>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ActivateMode<'info> {
    #[account(
        seeds = [b"registry"],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        mut,
        seeds = [b"mode", &mode.mode_id.to_le_bytes()],
        bump = mode.bump
    )]
    pub mode: Account<'info, Mode>,
}

#[derive(Accounts)]
pub struct DisableMode<'info> {
    #[account(
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        mut,
        seeds = [b"mode", &mode.mode_id.to_le_bytes()],
        bump = mode.bump
    )]
    pub mode: Account<'info, Mode>,

    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct UpdateModeParams<'info> {
    #[account(
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    #[account(
        mut,
        seeds = [b"mode", &mode.mode_id.to_le_bytes()],
        bump = mode.bump
    )]
    pub mode: Account<'info, Mode>,

    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct AddVerifier<'info> {
    #[account(
        mut,
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct RemoveVerifier<'info> {
    #[account(
        mut,
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct IsVerifier<'info> {
    #[account(
        seeds = [b"registry"],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,
}

#[derive(Accounts)]
pub struct TransferAdmin<'info> {
    #[account(
        mut,
        seeds = [b"registry"],
        bump = registry.bump,
        has_one = admin @ ErrorCode::Unauthorized
    )]
    pub registry: Account<'info, Registry>,

    pub admin: Signer<'info>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct Registry {
    /// Admin authority for registry operations
    pub admin: Pubkey,
    /// Total number of modes registered
    pub mode_count: u32,
    /// Number of active verifiers
    pub verifier_count: u8,
    /// Allowlist of verifier pubkeys for SLA attestations
    #[max_len(10)]
    pub verifiers: [Pubkey; MAX_VERIFIERS],
    /// PDA bump
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Mode {
    /// Unique mode identifier
    pub mode_id: u32,
    /// SPL token mint for this mode (payment + collateral + insurance)
    pub mint: Pubkey,
    /// Collateral ratio in basis points (e.g., 15000 = 150%)
    pub cr_bps: u16,
    /// Maximum collateral per provider
    pub per_provider_cap: u64,
    /// Maximum total collateral across all providers
    pub global_cap: u64,
    /// Current total deposited across all providers
    pub global_deposited: u64,
    /// Slot after which mode can be activated
    pub activation_slot: u64,
    /// Whether mode is currently active
    pub is_active: bool,
    /// Whether mode has been disabled (blocks new activity)
    pub is_disabled: bool,
    /// PDA bump
    pub bump: u8,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct RegistryInitialized {
    pub admin: Pubkey,
}

#[event]
pub struct ModeAdded {
    pub mode_id: u32,
    pub mint: Pubkey,
    pub cr_bps: u16,
    pub activation_slot: u64,
}

#[event]
pub struct ModeActivated {
    pub mode_id: u32,
    pub activated_at_slot: u64,
}

#[event]
pub struct ModeDisabled {
    pub mode_id: u32,
}

#[event]
pub struct ModeParamsUpdated {
    pub mode_id: u32,
    pub cr_bps: u16,
    pub per_provider_cap: u64,
    pub global_cap: u64,
}

#[event]
pub struct VerifierAdded {
    pub verifier: Pubkey,
}

#[event]
pub struct VerifierRemoved {
    pub verifier: Pubkey,
}

#[event]
pub struct AdminTransferred {
    pub old_admin: Pubkey,
    pub new_admin: Pubkey,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum ErrorCode {
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Collateral ratio too low (min 100%)")]
    CollateralRatioTooLow,
    #[msg("Collateral ratio too high (max 500%)")]
    CollateralRatioTooHigh,
    #[msg("Mode already active")]
    ModeAlreadyActive,
    #[msg("Mode is disabled")]
    ModeDisabled,
    #[msg("Timelock has not passed")]
    TimelockNotPassed,
    #[msg("Mode already disabled")]
    ModeAlreadyDisabled,
    #[msg("Cannot reduce collateral ratio")]
    CannotReduceCollateralRatio,
    #[msg("Cannot increase cap")]
    CannotIncreaseCap,
    #[msg("Cap cannot be below current deposited amount")]
    CapBelowDeposited,
    #[msg("Maximum verifiers reached")]
    MaxVerifiersReached,
    #[msg("Verifier already exists")]
    VerifierAlreadyExists,
    #[msg("Verifier not found")]
    VerifierNotFound,
}
