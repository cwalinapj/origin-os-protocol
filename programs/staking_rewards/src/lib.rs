use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("StakeRwd11111111111111111111111111111111111");

/// Staking Rewards Program
/// 
/// Stake Provider Position NFTs to earn protocol native token emissions.
/// Rewards weighted primarily by reserved collateral-time to prevent deposit-only farming.
#[program]
pub mod staking_rewards {
    use super::*;

    pub const EMISSION_RATE_PER_SLOT: u64 = 1_000_000;
    pub const RESERVED_WEIGHT_BPS: u64 = 8000;
    pub const FREE_WEIGHT_BPS: u64 = 2000;
    pub const MAX_BPS: u16 = 10_000;

    /// Initialize the emission controller (must be called first)
    pub fn init_emission_controller(
        ctx: Context<InitEmissionController>,
        global_rate_per_slot: u64,
        nft_pool_weight_bps: u16,
        native_pool_weight_bps: u16,
        emission_cap: u128,
    ) -> Result<()> {
        // Validate weights sum to ≤10000 bps
        let total_weight = nft_pool_weight_bps
            .checked_add(native_pool_weight_bps)
            .ok_or(ErrorCode::Overflow)?;
        require!(total_weight <= MAX_BPS, ErrorCode::InvalidWeights);

        let controller = &mut ctx.accounts.emission_controller;
        let clock = Clock::get()?;

        controller.authority = ctx.accounts.authority.key();
        controller.pending_authority = Pubkey::default();
        controller.reward_mint = ctx.accounts.reward_mint.key();
        controller.global_rate_per_slot = global_rate_per_slot;
        controller.nft_pool_weight_bps = nft_pool_weight_bps;
        controller.native_pool_weight_bps = native_pool_weight_bps;
        controller.total_emitted = 0;
        controller.emission_cap = emission_cap;
        controller.last_update_slot = clock.slot;
        controller.last_rate_change_slot = clock.slot;
        controller.paused = false;
        controller.bump = ctx.bumps.emission_controller;

        emit!(EmissionControllerInitialized {
            authority: controller.authority,
            reward_mint: controller.reward_mint,
            global_rate_per_slot,
            nft_pool_weight_bps,
            native_pool_weight_bps,
        });

        Ok(())
    }

    /// Update emission weights (authority only)
    pub fn update_emission_weights(
        ctx: Context<UpdateEmissionController>,
        nft_pool_weight_bps: u16,
        native_pool_weight_bps: u16,
    ) -> Result<()> {
        let total_weight = nft_pool_weight_bps
            .checked_add(native_pool_weight_bps)
            .ok_or(ErrorCode::Overflow)?;
        require!(total_weight <= MAX_BPS, ErrorCode::InvalidWeights);

        let controller = &mut ctx.accounts.emission_controller;
        let clock = Clock::get()?;

        controller.nft_pool_weight_bps = nft_pool_weight_bps;
        controller.native_pool_weight_bps = native_pool_weight_bps;
        controller.last_rate_change_slot = clock.slot;

        emit!(EmissionWeightsUpdated {
            nft_pool_weight_bps,
            native_pool_weight_bps,
        });

        Ok(())
    }

    /// Pause/unpause emissions (authority only)
    pub fn set_emission_paused(
        ctx: Context<UpdateEmissionController>,
        paused: bool,
    ) -> Result<()> {
        ctx.accounts.emission_controller.paused = paused;
        emit!(EmissionPausedUpdated { paused });
        Ok(())
    }

    /// Initialize the staking pool
    pub fn initialize_pool(ctx: Context<InitializePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.authority = ctx.accounts.authority.key();
        pool.reward_mint = ctx.accounts.reward_mint.key();
        pool.total_staked_weight = 0;
        pool.reward_per_weight_accumulated = 0;
        pool.last_update_slot = Clock::get()?.slot;
        pool.total_rewards_distributed = 0;
        pool.bump = ctx.bumps.pool;
        
        emit!(PoolInitialized {
            authority: pool.authority,
            reward_mint: pool.reward_mint,
        });
        
        Ok(())
    }

    /// Stake a provider position NFT
    pub fn stake_position(ctx: Context<StakePosition>) -> Result<()> {
        update_pool_rewards(&mut ctx.accounts.pool)?;
        
        let clock = Clock::get()?;
        
        // Transfer NFT to staking custody
        let cpi_accounts = Transfer {
            from: ctx.accounts.provider_nft_account.to_account_info(),
            to: ctx.accounts.staking_nft_custody.to_account_info(),
            authority: ctx.accounts.provider.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, 1)?;
        
        let reserved = ctx.accounts.collateral_position.reserved;
        let total = ctx.accounts.collateral_position.total;
        let free = total.saturating_sub(reserved);
        let stake_weight = compute_stake_weight(reserved, free);
        
        let reward_per_weight = ctx.accounts.pool.reward_per_weight_accumulated;
        
        // Initialize stake account
        let stake_account = &mut ctx.accounts.stake_account;
        stake_account.owner = ctx.accounts.provider.key();
        stake_account.position = ctx.accounts.collateral_position.key();
        stake_account.position_nft_mint = ctx.accounts.position_nft_mint.key();
        stake_account.staked_at_slot = clock.slot;
        stake_account.stake_weight = stake_weight;
        stake_account.reward_debt = stake_weight
            .checked_mul(reward_per_weight)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(PRECISION)
            .ok_or(ErrorCode::Overflow)?;
        stake_account.pending_rewards = 0;
        stake_account.bump = ctx.bumps.stake_account;
        
        // Update pool total
        let pool = &mut ctx.accounts.pool;
        pool.total_staked_weight = pool.total_staked_weight
            .checked_add(stake_weight)
            .ok_or(ErrorCode::Overflow)?;
        
        emit!(PositionStaked {
            owner: ctx.accounts.provider.key(),
            position: ctx.accounts.collateral_position.key(),
            stake_weight,
            staked_at_slot: clock.slot,
        });
        
        Ok(())
    }

    /// Update stake weight based on current collateral
    pub fn update_stake_weight(ctx: Context<UpdateStakeWeight>) -> Result<()> {
        update_pool_rewards(&mut ctx.accounts.pool)?;
        
        let pool = &mut ctx.accounts.pool;
        let stake_account = &mut ctx.accounts.stake_account;
        
        let pending = calculate_pending_rewards(pool, stake_account)?;
        stake_account.pending_rewards = stake_account.pending_rewards
            .checked_add(pending)
            .ok_or(ErrorCode::Overflow)?;
        
        let reserved = ctx.accounts.collateral_position.reserved;
        let total = ctx.accounts.collateral_position.total;
        let free = total.saturating_sub(reserved);
        let new_weight = compute_stake_weight(reserved, free);
        
        let old_weight = stake_account.stake_weight;
        
        pool.total_staked_weight = pool.total_staked_weight
            .saturating_sub(old_weight)
            .checked_add(new_weight)
            .ok_or(ErrorCode::Overflow)?;
        
        stake_account.stake_weight = new_weight;
        stake_account.reward_debt = new_weight
            .checked_mul(pool.reward_per_weight_accumulated)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(PRECISION)
            .ok_or(ErrorCode::Overflow)?;
        
        emit!(StakeWeightUpdated {
            owner: stake_account.owner,
            old_weight,
            new_weight,
        });
        
        Ok(())
    }

    /// Claim accumulated rewards
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        update_pool_rewards(&mut ctx.accounts.pool)?;
        
        // Capture values before mutable borrows
        let pool_info = ctx.accounts.pool.to_account_info();
        let reward_mint_info = ctx.accounts.reward_mint.to_account_info();
        let provider_reward_info = ctx.accounts.provider_reward_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        
        let pool = &mut ctx.accounts.pool;
        let stake_account = &mut ctx.accounts.stake_account;
        
        let pending = calculate_pending_rewards(pool, stake_account)?;
        let total_rewards = stake_account.pending_rewards
            .checked_add(pending)
            .ok_or(ErrorCode::Overflow)?;
        
        require!(total_rewards > 0, ErrorCode::NoRewardsToClaim);
        
        let owner = stake_account.owner;
        let stake_weight = stake_account.stake_weight;
        let bump = pool.bump;
        
        stake_account.pending_rewards = 0;
        stake_account.reward_debt = stake_weight
            .checked_mul(pool.reward_per_weight_accumulated)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(PRECISION)
            .ok_or(ErrorCode::Overflow)?;
        
        pool.total_rewards_distributed = pool.total_rewards_distributed
            .checked_add(total_rewards)
            .ok_or(ErrorCode::Overflow)?;
        
        let _ = pool;
        drop(stake_account);
        
        // Mint rewards
        let bump_slice = [bump];
        let seeds: &[&[u8]] = &[b"pool", &bump_slice];
        let signer_seeds = &[seeds];
        
        let cpi_accounts = MintTo {
            mint: reward_mint_info,
            to: provider_reward_info,
            authority: pool_info,
        };
        let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
        token::mint_to(cpi_ctx, total_rewards)?;
        
        emit!(RewardsClaimed {
            owner,
            amount: total_rewards,
        });
        
        Ok(())
    }

    /// Unstake position NFT
    pub fn unstake_position(ctx: Context<UnstakePosition>) -> Result<()> {
        update_pool_rewards(&mut ctx.accounts.pool)?;
        
        // Capture account infos
        let pool_info = ctx.accounts.pool.to_account_info();
        let stake_account_info = ctx.accounts.stake_account.to_account_info();
        let reward_mint_info = ctx.accounts.reward_mint.to_account_info();
        let provider_reward_info = ctx.accounts.provider_reward_account.to_account_info();
        let staking_nft_info = ctx.accounts.staking_nft_custody.to_account_info();
        let provider_nft_info = ctx.accounts.provider_nft_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        
        let pool = &mut ctx.accounts.pool;
        let stake_account = &ctx.accounts.stake_account;
        
        let pending = calculate_pending_rewards(pool, stake_account)?;
        let total_rewards = stake_account.pending_rewards
            .checked_add(pending)
            .ok_or(ErrorCode::Overflow)?;
        
        let owner = stake_account.owner;
        let position = stake_account.position;
        let stake_weight = stake_account.stake_weight;
        let pool_bump = pool.bump;
        let stake_bump = stake_account.bump;
        
        pool.total_staked_weight = pool.total_staked_weight.saturating_sub(stake_weight);
        
        if total_rewards > 0 {
            pool.total_rewards_distributed = pool.total_rewards_distributed
                .checked_add(total_rewards)
                .ok_or(ErrorCode::Overflow)?;
        }
        
        let _ = pool;
        
        // Mint rewards if any
        if total_rewards > 0 {
            let pool_bump_slice = [pool_bump];
            let pool_seeds: &[&[u8]] = &[b"pool", &pool_bump_slice];
            let pool_signer = &[pool_seeds];
            
            let cpi_accounts = MintTo {
                mint: reward_mint_info,
                to: provider_reward_info,
                authority: pool_info,
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info.clone(), cpi_accounts, pool_signer);
            token::mint_to(cpi_ctx, total_rewards)?;
        }
        
        // Transfer NFT back
        let stake_bump_slice = [stake_bump];
        let stake_seeds: &[&[u8]] = &[
            b"stake",
            owner.as_ref(),
            position.as_ref(),
            &stake_bump_slice,
        ];
        let stake_signer = &[stake_seeds];
        
        let cpi_accounts = Transfer {
            from: staking_nft_info,
            to: provider_nft_info,
            authority: stake_account_info,
        };
        let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, stake_signer);
        token::transfer(cpi_ctx, 1)?;
        
        emit!(PositionUnstaked {
            owner,
            position,
            rewards_claimed: total_rewards,
        });
        
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

const PRECISION: u64 = 1_000_000_000_000;

fn compute_stake_weight(reserved: u64, free: u64) -> u64 {
    let reserved_weighted = reserved
        .saturating_mul(staking_rewards::RESERVED_WEIGHT_BPS)
        .saturating_div(10000);
    
    let free_weighted = free
        .saturating_mul(staking_rewards::FREE_WEIGHT_BPS)
        .saturating_div(10000);
    
    reserved_weighted.saturating_add(free_weighted)
}

fn update_pool_rewards(pool: &mut Account<StakingPool>) -> Result<()> {
    let clock = Clock::get()?;
    let current_slot = clock.slot;
    
    if pool.total_staked_weight == 0 {
        pool.last_update_slot = current_slot;
        return Ok(());
    }
    
    let slots_elapsed = current_slot.saturating_sub(pool.last_update_slot);
    if slots_elapsed == 0 {
        return Ok(());
    }
    
    let rewards_this_period = slots_elapsed
        .checked_mul(staking_rewards::EMISSION_RATE_PER_SLOT)
        .ok_or(ErrorCode::Overflow)?;
    
    let increment = rewards_this_period
        .checked_mul(PRECISION)
        .ok_or(ErrorCode::Overflow)?
        .checked_div(pool.total_staked_weight)
        .ok_or(ErrorCode::Overflow)?;
    
    pool.reward_per_weight_accumulated = pool.reward_per_weight_accumulated
        .checked_add(increment)
        .ok_or(ErrorCode::Overflow)?;
    
    pool.last_update_slot = current_slot;
    
    Ok(())
}

fn calculate_pending_rewards(pool: &StakingPool, stake: &StakeAccount) -> Result<u64> {
    let accumulated_reward = stake.stake_weight
        .checked_mul(pool.reward_per_weight_accumulated)
        .ok_or(ErrorCode::Overflow)?
        .checked_div(PRECISION)
        .ok_or(ErrorCode::Overflow)?;
    
    Ok(accumulated_reward.saturating_sub(stake.reward_debt))
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
pub struct InitEmissionController<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + EmissionController::INIT_SPACE,
        seeds = [b"emission_controller"],
        bump
    )]
    pub emission_controller: Account<'info, EmissionController>,

    pub reward_mint: Account<'info, Mint>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateEmissionController<'info> {
    #[account(
        mut,
        seeds = [b"emission_controller"],
        bump = emission_controller.bump,
        has_one = authority @ ErrorCode::Unauthorized
    )]
    pub emission_controller: Account<'info, EmissionController>,

    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + StakingPool::INIT_SPACE,
        seeds = [b"pool"],
        bump
    )]
    pub pool: Account<'info, StakingPool>,
    
    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StakePosition<'info> {
    #[account(
        mut,
        seeds = [b"pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, StakingPool>,
    
    #[account(
        init,
        payer = provider,
        space = 8 + StakeAccount::INIT_SPACE,
        seeds = [b"stake", provider.key().as_ref(), collateral_position.key().as_ref()],
        bump
    )]
    pub stake_account: Account<'info, StakeAccount>,
    
    pub collateral_position: Account<'info, CollateralPosition>,
    
    pub position_nft_mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub provider_nft_account: Account<'info, TokenAccount>,
    
    #[account(
        init,
        payer = provider,
        associated_token::mint = position_nft_mint,
        associated_token::authority = stake_account
    )]
    pub staking_nft_custody: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub provider: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateStakeWeight<'info> {
    #[account(
        mut,
        seeds = [b"pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, StakingPool>,
    
    #[account(
        mut,
        seeds = [b"stake", stake_account.owner.as_ref(), stake_account.position.as_ref()],
        bump = stake_account.bump
    )]
    pub stake_account: Account<'info, StakeAccount>,
    
    pub collateral_position: Account<'info, CollateralPosition>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(
        mut,
        seeds = [b"pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, StakingPool>,
    
    #[account(
        mut,
        seeds = [b"stake", provider.key().as_ref(), stake_account.position.as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == provider.key() @ ErrorCode::WrongOwner
    )]
    pub stake_account: Account<'info, StakeAccount>,
    
    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub provider_reward_account: Account<'info, TokenAccount>,
    
    pub provider: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UnstakePosition<'info> {
    #[account(
        mut,
        seeds = [b"pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, StakingPool>,
    
    #[account(
        mut,
        seeds = [b"stake", provider.key().as_ref(), stake_account.position.as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == provider.key() @ ErrorCode::WrongOwner,
        close = provider
    )]
    pub stake_account: Account<'info, StakeAccount>,
    
    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub provider_reward_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub provider_nft_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        associated_token::mint = stake_account.position_nft_mint,
        associated_token::authority = stake_account
    )]
    pub staking_nft_custody: Account<'info, TokenAccount>,
    
    pub provider: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct EmissionController {
    pub authority: Pubkey,
    pub pending_authority: Pubkey,      // 2-step authority transfer
    pub reward_mint: Pubkey,
    pub global_rate_per_slot: u64,      // Total emissions per slot
    pub nft_pool_weight_bps: u16,       // e.g. 7000 = 70%
    pub native_pool_weight_bps: u16,    // e.g. 3000 = 30%
    pub total_emitted: u128,            // Running total
    pub emission_cap: u128,             // Max total supply
    pub last_update_slot: u64,
    pub last_rate_change_slot: u64,     // When rate was last changed
    pub paused: bool,                   // Emergency stop
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct StakingPool {
    pub authority: Pubkey,
    pub reward_mint: Pubkey,
    pub total_staked_weight: u64,
    pub reward_per_weight_accumulated: u64,
    pub last_update_slot: u64,
    pub total_rewards_distributed: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct StakeAccount {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub position_nft_mint: Pubkey,
    pub staked_at_slot: u64,
    pub stake_weight: u64,
    pub reward_debt: u64,
    pub pending_rewards: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct CollateralPosition {
    pub provider: Pubkey,
    pub mode_id: u32,
    pub mint: Pubkey,
    pub total: u64,
    pub reserved: u64,
    pub position_nft_mint: Pubkey,
    pub bump: u8,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct EmissionControllerInitialized {
    pub authority: Pubkey,
    pub reward_mint: Pubkey,
    pub global_rate_per_slot: u64,
    pub nft_pool_weight_bps: u16,
    pub native_pool_weight_bps: u16,
}

#[event]
pub struct EmissionWeightsUpdated {
    pub nft_pool_weight_bps: u16,
    pub native_pool_weight_bps: u16,
}

#[event]
pub struct EmissionPausedUpdated {
    pub paused: bool,
}

#[event]
pub struct PoolInitialized {
    pub authority: Pubkey,
    pub reward_mint: Pubkey,
}

#[event]
pub struct PositionStaked {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub stake_weight: u64,
    pub staked_at_slot: u64,
}

#[event]
pub struct StakeWeightUpdated {
    pub owner: Pubkey,
    pub old_weight: u64,
    pub new_weight: u64,
}

#[event]
pub struct RewardsClaimed {
    pub owner: Pubkey,
    pub amount: u64,
}

#[event]
pub struct PositionUnstaked {
    pub owner: Pubkey,
    pub position: Pubkey,
    pub rewards_claimed: u64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum ErrorCode {
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("No rewards to claim")]
    NoRewardsToClaim,
    #[msg("Wrong owner")]
    WrongOwner,
    #[msg("Invalid weights: must sum to <= 10000 bps")]
    InvalidWeights,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Emissions paused")]
    EmissionsPaused,
}
