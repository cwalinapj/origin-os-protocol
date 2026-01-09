use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, MintTo};
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

declare_id!("NakedStk1111111111111111111111111111111111");

/// Naked Staking Program
/// 
/// Stake protocol native token (ORIGIN) without provider NFT.
/// Rewards weighted by USD value via Pyth oracle with configurable discount.
/// Naked stakers earn less than NFT stakers to incentivize providing compute.
#[program]
pub mod naked_staking {
    use super::*;

    // ========================================================================
    // Constants
    // ========================================================================
    
    /// Precision multiplier for reward_per_share (1e12)
    pub const PRECISION: u128 = 1_000_000_000_000;
    
    /// Basis points denominator
    pub const BPS_DENOMINATOR: u16 = 10_000;
    
    /// Default discount: 80% (naked stakers earn 80% of equivalent USD weight)
    pub const DEFAULT_DISCOUNT_BPS: u16 = 8_000;
    
    /// Min discount floor (50%)
    pub const MIN_DISCOUNT_FLOOR: u16 = 5_000;
    
    /// Max discount ceiling (95%)
    pub const MAX_DISCOUNT_CEILING: u16 = 9_500;
    
    /// Default staleness threshold (~24 seconds)
    pub const DEFAULT_STALENESS_SLOTS: u64 = 60;
    
    /// Default min stake duration (~2 minutes, flash loan protection)
    pub const DEFAULT_MIN_STAKE_DURATION: u64 = 300;

    // ========================================================================
    // Section 2: Initialize Pool
    // ========================================================================
    
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        discount_bps: u16,
        max_staleness_slots: u64,
        min_stake_duration_slots: u64,
        deposit_cap: u64,
        reward_rate_per_slot: u64,
    ) -> Result<()> {
        // Validate discount bounds
        require!(
            discount_bps >= MIN_DISCOUNT_FLOOR && discount_bps <= MAX_DISCOUNT_CEILING,
            NakedStakingError::InvalidDiscountBps
        );
        
        let pool = &mut ctx.accounts.pool;
        let clock = Clock::get()?;
        
        // Version & Authority
        pool.version = 1;
        pool.authority = ctx.accounts.authority.key();
        pool.bump = ctx.bumps.pool;
        
        // Token Config
        pool.native_mint = ctx.accounts.native_mint.key();
        pool.reward_mint = ctx.accounts.reward_mint.key();
        pool.vault = ctx.accounts.vault.key();
        
        // Oracle Config
        pool.pyth_feed = ctx.accounts.pyth_feed.key();
        pool.max_staleness_slots = if max_staleness_slots == 0 { 
            DEFAULT_STALENESS_SLOTS 
        } else { 
            max_staleness_slots 
        };
        
        // Discount Config
        pool.discount_bps = discount_bps;
        pool.min_discount_bps = MIN_DISCOUNT_FLOOR;
        pool.max_discount_bps = MAX_DISCOUNT_CEILING;
        
        // Accumulator
        pool.reward_rate_per_slot = reward_rate_per_slot;
        pool.reward_per_share = 0;
        pool.last_update_slot = clock.slot;
        
        // Totals
        pool.total_staked = 0;
        pool.total_weight = 0;
        
        // Safety
        pool.min_stake_duration_slots = if min_stake_duration_slots == 0 {
            DEFAULT_MIN_STAKE_DURATION
        } else {
            min_stake_duration_slots
        };
        pool.deposit_cap = deposit_cap; // 0 = unlimited
        pool.paused = false;
        
        // Reserved (zero-initialized by default)
        pool.reserved = [0u8; 32];
        
        emit!(NativePoolInitialized {
            authority: pool.authority,
            native_mint: pool.native_mint,
            discount_bps,
        });
        
        Ok(())
    }

    // ========================================================================
    // Section 3: Stake Native Token
    // ========================================================================
    
    pub fn stake_native(ctx: Context<StakeNative>, amount: u64) -> Result<()> {
        require!(amount > 0, NakedStakingError::ZeroAmount);
        
        let pool = &ctx.accounts.pool;
        require!(!pool.paused, NakedStakingError::PoolPaused);
        
        // Check deposit cap (0 = unlimited)
        if pool.deposit_cap > 0 {
            require!(
                pool.total_staked.checked_add(amount).ok_or(NakedStakingError::Overflow)? 
                    <= pool.deposit_cap,
                NakedStakingError::DepositCapExceeded
            );
        }
        
        let clock = Clock::get()?;
        
        // Load and validate Pyth price
        let price_data = &ctx.accounts.pyth_feed;
        let price = get_validated_price(price_data, pool.max_staleness_slots, &clock)?;
        
        // Transfer tokens to vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        
        // Update pool rewards accumulator BEFORE changing weights
        let pool = &mut ctx.accounts.pool;
        update_reward_accumulator(pool, &clock)?;
        
        // Get or create position
        let position = &mut ctx.accounts.position;
        let is_new_position = position.amount == 0;
        
        if is_new_position {
            position.owner = ctx.accounts.user.key();
            position.pool = pool.key();
            position.bump = ctx.bumps.position;
            position.staked_at_slot = clock.slot;
            position.pending_rewards = 0;
            position.reserved = [0u8; 16];
        } else {
            // Accrue pending rewards before weight change
            accrue_position_rewards(pool, position)?;
        }
        
        // Calculate new weight using current price
        let old_weight = position.weight;
        let new_amount = position.amount
            .checked_add(amount)
            .ok_or(NakedStakingError::Overflow)?;
        let new_weight = calculate_usd_weight(new_amount, price, pool.discount_bps)?;
        
        // Update position
        position.amount = new_amount;
        position.weight = new_weight;
        position.last_stake_slot = clock.slot;
        position.last_claim_slot = clock.slot;
        position.reward_debt = new_weight
            .checked_mul(pool.reward_per_share)
            .ok_or(NakedStakingError::Overflow)?
            .checked_div(PRECISION)
            .ok_or(NakedStakingError::Overflow)?;
        
        // Update pool totals
        pool.total_staked = pool.total_staked
            .checked_add(amount)
            .ok_or(NakedStakingError::Overflow)?;
        pool.total_weight = pool.total_weight
            .checked_sub(old_weight)
            .ok_or(NakedStakingError::Overflow)?
            .checked_add(new_weight)
            .ok_or(NakedStakingError::Overflow)?;
        
        emit!(NativeStaked {
            user: ctx.accounts.user.key(),
            amount,
            new_total: position.amount,
            weight: new_weight,
            price_used: price,
        });
        
        Ok(())
    }

    // ========================================================================
    // Section 4: Unstake Native Token
    // ========================================================================
    
    pub fn unstake_native(ctx: Context<UnstakeNative>, amount: u64) -> Result<()> {
        require!(amount > 0, NakedStakingError::ZeroAmount);
        
        let clock = Clock::get()?;
        let position = &ctx.accounts.position;
        
        // Check min stake duration (flash loan protection)
        let slots_since_stake = clock.slot.saturating_sub(position.last_stake_slot);
        require!(
            slots_since_stake >= ctx.accounts.pool.min_stake_duration_slots,
            NakedStakingError::MinStakeDurationNotMet
        );
        
        require!(
            amount <= position.amount,
            NakedStakingError::InsufficientStake
        );
        
        // Load and validate Pyth price
        let price_data = &ctx.accounts.pyth_feed;
        let pool = &ctx.accounts.pool;
        let price = get_validated_price(price_data, pool.max_staleness_slots, &clock)?;
        
        // Update pool rewards accumulator
        let pool = &mut ctx.accounts.pool;
        update_reward_accumulator(pool, &clock)?;
        
        // Accrue pending rewards
        let position = &mut ctx.accounts.position;
        accrue_position_rewards(pool, position)?;
        
        // Calculate new weight
        let old_weight = position.weight;
        let new_amount = position.amount
            .checked_sub(amount)
            .ok_or(NakedStakingError::Overflow)?;
        let new_weight = if new_amount > 0 {
            calculate_usd_weight(new_amount, price, pool.discount_bps)?
        } else {
            0
        };
        
        // Update position
        position.amount = new_amount;
        position.weight = new_weight;
        position.reward_debt = new_weight
            .checked_mul(pool.reward_per_share)
            .ok_or(NakedStakingError::Overflow)?
            .checked_div(PRECISION)
            .ok_or(NakedStakingError::Overflow)?;
        
        // Update pool totals
        pool.total_staked = pool.total_staked
            .saturating_sub(amount);
        pool.total_weight = pool.total_weight
            .checked_sub(old_weight)
            .ok_or(NakedStakingError::Overflow)?
            .checked_add(new_weight)
            .ok_or(NakedStakingError::Overflow)?;
        
        // Transfer tokens back to user
        let bump = pool.bump;
        let seeds = &[b"native_pool".as_ref(), &[bump]];
        let signer_seeds = &[&seeds[..]];
        
        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.pool.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;
        
        emit!(NativeUnstaked {
            user: ctx.accounts.user.key(),
            amount,
            remaining: position.amount,
            weight: new_weight,
        });
        
        Ok(())
    }

    // ========================================================================
    // Section 5: Claim Rewards
    // ========================================================================
    
    pub fn claim_native_rewards(ctx: Context<ClaimNativeRewards>) -> Result<()> {
        let clock = Clock::get()?;
        
        // Update pool rewards accumulator
        let pool = &mut ctx.accounts.pool;
        update_reward_accumulator(pool, &clock)?;
        
        // Accrue and claim
        let position = &mut ctx.accounts.position;
        accrue_position_rewards(pool, position)?;
        
        let rewards_to_claim = position.pending_rewards;
        require!(rewards_to_claim > 0, NakedStakingError::NoRewardsToClaim);
        
        // Reset pending
        position.pending_rewards = 0;
        position.last_claim_slot = clock.slot;
        
        // Update reward debt to current accumulator
        position.reward_debt = position.weight
            .checked_mul(pool.reward_per_share)
            .ok_or(NakedStakingError::Overflow)?
            .checked_div(PRECISION)
            .ok_or(NakedStakingError::Overflow)?;
        
        // Mint rewards to user (assumes pool has mint authority)
        let bump = pool.bump;
        let seeds = &[b"native_pool".as_ref(), &[bump]];
        let signer_seeds = &[&seeds[..]];
        
        // Convert u128 to u64 for mint (check overflow)
        let rewards_u64: u64 = rewards_to_claim
            .try_into()
            .map_err(|_| NakedStakingError::Overflow)?;
        
        let cpi_accounts = MintTo {
            mint: ctx.accounts.reward_mint.to_account_info(),
            to: ctx.accounts.user_reward_account.to_account_info(),
            authority: ctx.accounts.pool.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::mint_to(cpi_ctx, rewards_u64)?;
        
        emit!(NativeRewardsClaimed {
            user: ctx.accounts.user.key(),
            amount: rewards_u64,
        });
        
        Ok(())
    }

    // ========================================================================
    // Section 6: Admin Instructions
    // ========================================================================
    
    pub fn update_discount(ctx: Context<AdminUpdate>, new_discount_bps: u16) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        
        require!(
            new_discount_bps >= pool.min_discount_bps && 
            new_discount_bps <= pool.max_discount_bps,
            NakedStakingError::InvalidDiscountBps
        );
        
        let old_discount = pool.discount_bps;
        pool.discount_bps = new_discount_bps;
        
        emit!(DiscountUpdated {
            old_discount_bps: old_discount,
            new_discount_bps,
        });
        
        Ok(())
    }
    
    pub fn pause_pool(ctx: Context<AdminUpdate>) -> Result<()> {
        ctx.accounts.pool.paused = true;
        emit!(PoolPaused {});
        Ok(())
    }
    
    pub fn unpause_pool(ctx: Context<AdminUpdate>) -> Result<()> {
        ctx.accounts.pool.paused = false;
        emit!(PoolUnpaused {});
        Ok(())
    }
    
    pub fn update_deposit_cap(ctx: Context<AdminUpdate>, new_cap: u64) -> Result<()> {
        ctx.accounts.pool.deposit_cap = new_cap;
        emit!(DepositCapUpdated { new_cap });
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Update the global reward accumulator based on slots elapsed
fn update_reward_accumulator(pool: &mut Account<NativeStakePool>, clock: &Clock) -> Result<()> {
    let current_slot = clock.slot;
    
    if pool.total_weight == 0 {
        pool.last_update_slot = current_slot;
        return Ok(());
    }
    
    let slots_elapsed = current_slot.saturating_sub(pool.last_update_slot);
    if slots_elapsed == 0 {
        return Ok(());
    }
    
    // Calculate new rewards: slots * rate
    let new_rewards = (slots_elapsed as u128)
        .checked_mul(pool.reward_rate_per_slot as u128)
        .ok_or(NakedStakingError::Overflow)?;
    
    // Increment per-share: (new_rewards * PRECISION) / total_weight
    let increment = new_rewards
        .checked_mul(naked_staking::PRECISION)
        .ok_or(NakedStakingError::Overflow)?
        .checked_div(pool.total_weight)
        .ok_or(NakedStakingError::Overflow)?;
    
    pool.reward_per_share = pool.reward_per_share
        .checked_add(increment)
        .ok_or(NakedStakingError::Overflow)?;
    
    pool.last_update_slot = current_slot;
    
    Ok(())
}

/// Accrue pending rewards for a position
fn accrue_position_rewards(
    pool: &NativeStakePool, 
    position: &mut Account<NativeStakePosition>
) -> Result<()> {
    if position.weight == 0 {
        return Ok(());
    }
    
    // Calculate: (weight * reward_per_share / PRECISION) - reward_debt
    let accumulated = position.weight
        .checked_mul(pool.reward_per_share)
        .ok_or(NakedStakingError::Overflow)?
        .checked_div(naked_staking::PRECISION)
        .ok_or(NakedStakingError::Overflow)?;
    
    let pending = accumulated
        .checked_sub(position.reward_debt)
        .unwrap_or(0); // Handle edge case where debt > accumulated
    
    position.pending_rewards = position.pending_rewards
        .checked_add(pending)
        .ok_or(NakedStakingError::Overflow)?;
    
    Ok(())
}

/// Calculate USD-weighted stake with discount
/// weight = (amount * price * discount_bps) / BPS_DENOMINATOR
fn calculate_usd_weight(amount: u64, price: i64, discount_bps: u16) -> Result<u128> {
    require!(price > 0, NakedStakingError::InvalidOraclePrice);
    
    let weight = (amount as u128)
        .checked_mul(price as u128)
        .ok_or(NakedStakingError::Overflow)?
        .checked_mul(discount_bps as u128)
        .ok_or(NakedStakingError::Overflow)?
        .checked_div(naked_staking::BPS_DENOMINATOR as u128)
        .ok_or(NakedStakingError::Overflow)?;
    
    Ok(weight)
}

/// Load and validate Pyth price feed
fn get_validated_price(
    price_feed: &Account<PriceUpdateV2>,
    max_staleness_slots: u64,
    clock: &Clock,
) -> Result<i64> {
    // Get price from Pyth
    let price_data = price_feed.get_price_no_older_than(
        clock,
        max_staleness_slots,
        None, // feed_id checked via account constraint
    ).map_err(|_| NakedStakingError::StaleOraclePrice)?;
    
    require!(price_data.price > 0, NakedStakingError::InvalidOraclePrice);
    
    // Check confidence (price ± conf) - reject if too wide (>5%)
    let confidence_ratio = if price_data.price > 0 {
        (price_data.conf as f64) / (price_data.price as f64)
    } else {
        1.0
    };
    require!(confidence_ratio < 0.05, NakedStakingError::OracleConfidenceTooWide);
    
    Ok(price_data.price)
}

// ============================================================================
// Section 1: Account Structs
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct NativeStakePool {
    // Version & Authority
    pub version: u8,
    pub authority: Pubkey,
    pub bump: u8,
    
    // Token Config
    pub native_mint: Pubkey,
    pub reward_mint: Pubkey,
    pub vault: Pubkey,
    
    // Oracle Config
    pub pyth_feed: Pubkey,
    pub max_staleness_slots: u64,
    
    // Discount Config
    pub discount_bps: u16,
    pub min_discount_bps: u16,
    pub max_discount_bps: u16,
    
    // Accumulator (MasterChef pattern)
    pub reward_rate_per_slot: u64,
    pub reward_per_share: u128,
    pub last_update_slot: u64,
    
    // Totals
    pub total_staked: u64,
    pub total_weight: u128,
    
    // Safety
    pub min_stake_duration_slots: u64,
    pub deposit_cap: u64,
    pub paused: bool,
    
    // Future expansion
    #[max_len(32)]
    pub reserved: [u8; 32],
}

#[account]
#[derive(InitSpace)]
pub struct NativeStakePosition {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub bump: u8,
    
    // Staking State
    pub amount: u64,
    pub weight: u128,
    
    // Rewards (MasterChef)
    pub reward_debt: u128,
    pub pending_rewards: u128,
    
    // Timestamps
    pub staked_at_slot: u64,
    pub last_stake_slot: u64,
    pub last_claim_slot: u64,
    
    // Future expansion
    #[max_len(16)]
    pub reserved: [u8; 16],
}

// ============================================================================
// Context Structs
// ============================================================================

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + NativeStakePool::INIT_SPACE,
        seeds = [b"native_pool"],
        bump
    )]
    pub pool: Account<'info, NativeStakePool>,
    
    pub native_mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = authority,
        token::mint = native_mint,
        token::authority = pool,
        seeds = [b"native_vault"],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,
    
    /// CHECK: Validated as Pyth price feed
    pub pyth_feed: Account<'info, PriceUpdateV2>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StakeNative<'info> {
    #[account(
        mut,
        seeds = [b"native_pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, NativeStakePool>,
    
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + NativeStakePosition::INIT_SPACE,
        seeds = [b"native_pos", pool.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub position: Account<'info, NativeStakePosition>,
    
    #[account(
        mut,
        seeds = [b"native_vault"],
        bump,
        constraint = vault.key() == pool.vault
    )]
    pub vault: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_token_account.mint == pool.native_mint,
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        constraint = pyth_feed.key() == pool.pyth_feed
    )]
    pub pyth_feed: Account<'info, PriceUpdateV2>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UnstakeNative<'info> {
    #[account(
        mut,
        seeds = [b"native_pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, NativeStakePool>,
    
    #[account(
        mut,
        seeds = [b"native_pos", pool.key().as_ref(), user.key().as_ref()],
        bump = position.bump,
        constraint = position.owner == user.key() @ NakedStakingError::WrongOwner
    )]
    pub position: Account<'info, NativeStakePosition>,
    
    #[account(
        mut,
        seeds = [b"native_vault"],
        bump,
        constraint = vault.key() == pool.vault
    )]
    pub vault: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_token_account.mint == pool.native_mint,
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    #[account(
        constraint = pyth_feed.key() == pool.pyth_feed
    )]
    pub pyth_feed: Account<'info, PriceUpdateV2>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimNativeRewards<'info> {
    #[account(
        mut,
        seeds = [b"native_pool"],
        bump = pool.bump
    )]
    pub pool: Account<'info, NativeStakePool>,
    
    #[account(
        mut,
        seeds = [b"native_pos", pool.key().as_ref(), user.key().as_ref()],
        bump = position.bump,
        constraint = position.owner == user.key() @ NakedStakingError::WrongOwner
    )]
    pub position: Account<'info, NativeStakePosition>,
    
    #[account(
        mut,
        constraint = reward_mint.key() == pool.reward_mint
    )]
    pub reward_mint: Account<'info, Mint>,
    
    #[account(
        mut,
        constraint = user_reward_account.mint == pool.reward_mint,
        constraint = user_reward_account.owner == user.key()
    )]
    pub user_reward_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminUpdate<'info> {
    #[account(
        mut,
        seeds = [b"native_pool"],
        bump = pool.bump,
        has_one = authority @ NakedStakingError::Unauthorized
    )]
    pub pool: Account<'info, NativeStakePool>,
    
    pub authority: Signer<'info>,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct NativePoolInitialized {
    pub authority: Pubkey,
    pub native_mint: Pubkey,
    pub discount_bps: u16,
}

#[event]
pub struct NativeStaked {
    pub user: Pubkey,
    pub amount: u64,
    pub new_total: u64,
    pub weight: u128,
    pub price_used: i64,
}

#[event]
pub struct NativeUnstaked {
    pub user: Pubkey,
    pub amount: u64,
    pub remaining: u64,
    pub weight: u128,
}

#[event]
pub struct NativeRewardsClaimed {
    pub user: Pubkey,
    pub amount: u64,
}

#[event]
pub struct DiscountUpdated {
    pub old_discount_bps: u16,
    pub new_discount_bps: u16,
}

#[event]
pub struct DepositCapUpdated {
    pub new_cap: u64,
}

#[event]
pub struct PoolPaused {}

#[event]
pub struct PoolUnpaused {}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum NakedStakingError {
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Invalid discount bps - must be between min and max")]
    InvalidDiscountBps,
    #[msg("Stale oracle price")]
    StaleOraclePrice,
    #[msg("Invalid oracle price")]
    InvalidOraclePrice,
    #[msg("Oracle confidence interval too wide")]
    OracleConfidenceTooWide,
    #[msg("Pool is paused")]
    PoolPaused,
    #[msg("Deposit cap exceeded")]
    DepositCapExceeded,
    #[msg("Minimum stake duration not met")]
    MinStakeDurationNotMet,
    #[msg("Insufficient stake balance")]
    InsufficientStake,
    #[msg("No rewards to claim")]
    NoRewardsToClaim,
    #[msg("Wrong owner")]
    WrongOwner,
    #[msg("Unauthorized")]
    Unauthorized,
}
