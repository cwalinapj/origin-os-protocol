use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount, Mint};
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

declare_id!("GateWay1111111111111111111111111111111111111");

/// Gateway Program
/// 
/// Bridges external DEX swaps to session escrow and collateral vault flows.
/// Validates pricing via Pyth oracles, enforces slippage limits.
/// 
/// SECURITY:
/// - Only allowlisted swap programs can be invoked
/// - Only allowlisted pools can be used
/// - Pyth price freshness and confidence enforced
/// - Max trade size limits
#[program]
pub mod gateway {
    use super::*;

    /// Initialize gateway configuration
    pub fn init_gateway_config(
        ctx: Context<InitGatewayConfig>,
        max_slippage_bps: u16,
        max_trade_size: u64,
        pyth_max_age_seconds: u64,
        pyth_max_conf_ratio_bps: u16,
        native_feed_id: [u8; 32],
    ) -> Result<()> {
        // Capture key BEFORE mutable borrow
        let config_key = ctx.accounts.config.key();
        
        let config = &mut ctx.accounts.config;
        
        config.authority = ctx.accounts.authority.key();
        config.max_slippage_bps = max_slippage_bps;
        config.max_trade_size = max_trade_size;
        config.pyth_max_age_seconds = pyth_max_age_seconds;
        config.pyth_max_conf_ratio_bps = pyth_max_conf_ratio_bps;
        config.native_feed_id = native_feed_id;
        config.swap_program_count = 0;
        config.pool_count = 0;
        config.mode_feed_count = 0;
        config.bump = ctx.bumps.config;
        
        let authority = config.authority;
        
        emit!(GatewayConfigInitialized {
            config: config_key,
            authority,
            max_slippage_bps,
            max_trade_size,
        });
        
        Ok(())
    }

    /// Add a swap program to the allowlist
    pub fn add_swap_program(
        ctx: Context<ModifyConfig>,
        program_id: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        let count = config.swap_program_count as usize;
        
        require!(
            count < MAX_SWAP_PROGRAMS,
            GatewayError::MaxSwapProgramsReached
        );
        
        // Check not already added
        for i in 0..count {
            require!(
                config.allowlisted_swap_programs[i] != program_id,
                GatewayError::AlreadyAllowlisted
            );
        }
        
        config.allowlisted_swap_programs[count] = program_id;
        config.swap_program_count += 1;
        
        emit!(SwapProgramAdded { program_id });
        
        Ok(())
    }

    /// Remove a swap program from the allowlist
    pub fn remove_swap_program(
        ctx: Context<ModifyConfig>,
        program_id: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        let count = config.swap_program_count as usize;
        let mut found_idx: Option<usize> = None;
        
        for i in 0..count {
            if config.allowlisted_swap_programs[i] == program_id {
                found_idx = Some(i);
                break;
            }
        }
        
        let idx = found_idx.ok_or(GatewayError::NotAllowlisted)?;
        
        // Shift remaining elements
        for i in idx..(count - 1) {
            config.allowlisted_swap_programs[i] = config.allowlisted_swap_programs[i + 1];
        }
        config.swap_program_count -= 1;
        
        emit!(SwapProgramRemoved { program_id });
        
        Ok(())
    }

    /// Add a pool to the allowlist
    pub fn add_pool(
        ctx: Context<ModifyConfig>,
        pool: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        let count = config.pool_count as usize;
        
        require!(
            count < MAX_POOLS,
            GatewayError::MaxPoolsReached
        );
        
        for i in 0..count {
            require!(
                config.allowlisted_pools[i] != pool,
                GatewayError::AlreadyAllowlisted
            );
        }
        
        config.allowlisted_pools[count] = pool;
        config.pool_count += 1;
        
        emit!(PoolAdded { pool });
        
        Ok(())
    }

    /// Add Pyth feed for a mode's mint
    pub fn add_mode_feed(
        ctx: Context<ModifyConfig>,
        mint: Pubkey,
        feed_id: [u8; 32],
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        
        let count = config.mode_feed_count as usize;
        
        require!(
            count < MAX_MODE_FEEDS,
            GatewayError::MaxModeFeedsReached
        );
        
        // Check not already added
        for i in 0..count {
            require!(
                config.mode_feeds[i].mint != mint,
                GatewayError::AlreadyAllowlisted
            );
        }
        
        config.mode_feeds[count] = ModeFeed { mint, feed_id };
        config.mode_feed_count += 1;
        
        emit!(ModeFeedAdded { mint, feed_id });
        
        Ok(())
    }

    /// Swap tokens and fund a session escrow (STUB)
    pub fn swap_and_fund_session(
        ctx: Context<SwapAndFundSession>,
        amount_in: u64,
        _min_amount_out: u64,
    ) -> Result<()> {
        let config = &ctx.accounts.config;
        
        require!(
            amount_in <= config.max_trade_size,
            GatewayError::TradeTooLarge
        );
        
        // Validate swap program is allowlisted
        let swap_program = ctx.accounts.swap_program.key();
        let swap_count = config.swap_program_count as usize;
        let mut swap_allowed = false;
        for i in 0..swap_count {
            if config.allowlisted_swap_programs[i] == swap_program {
                swap_allowed = true;
                break;
            }
        }
        require!(swap_allowed, GatewayError::SwapProgramNotAllowlisted);
        
        // Load and validate prices
        let _price_in = pyth_helpers::validate_price(
            &ctx.accounts.input_price_update,
            &config.native_feed_id,
            config.pyth_max_age_seconds,
            config.pyth_max_conf_ratio_bps,
        )?;
        
        let _price_out = pyth_helpers::validate_price(
            &ctx.accounts.output_price_update,
            &config.native_feed_id,
            config.pyth_max_age_seconds,
            config.pyth_max_conf_ratio_bps,
        )?;
        
        // TODO: Calculate conservative_min_out
        // TODO: Execute swap CPI
        // TODO: Fund session CPI
        
        emit!(SwapAndFundStubbed {
            user: ctx.accounts.user.key(),
            amount_in,
            session: ctx.accounts.session.key(),
        });
        
        Ok(())
    }

    /// Swap tokens and deposit as collateral (STUB)
    pub fn swap_and_deposit_collateral(
        ctx: Context<SwapAndDepositCollateral>,
        amount_in: u64,
        mode_id: u32,
        _min_amount_out: u64,
    ) -> Result<()> {
        let config = &ctx.accounts.config;
        
        require!(
            amount_in <= config.max_trade_size,
            GatewayError::TradeTooLarge
        );
        
        // Validate swap program
        let swap_program = ctx.accounts.swap_program.key();
        let swap_count = config.swap_program_count as usize;
        let mut swap_allowed = false;
        for i in 0..swap_count {
            if config.allowlisted_swap_programs[i] == swap_program {
                swap_allowed = true;
                break;
            }
        }
        require!(swap_allowed, GatewayError::SwapProgramNotAllowlisted);
        
        // Load prices
        let _price_in = pyth_helpers::validate_price(
            &ctx.accounts.input_price_update,
            &config.native_feed_id,
            config.pyth_max_age_seconds,
            config.pyth_max_conf_ratio_bps,
        )?;
        
        let _price_out = pyth_helpers::validate_price(
            &ctx.accounts.output_price_update,
            &config.native_feed_id,
            config.pyth_max_age_seconds,
            config.pyth_max_conf_ratio_bps,
        )?;
        
        emit!(SwapAndDepositStubbed {
            provider: ctx.accounts.provider.key(),
            amount_in,
            mode_id,
        });
        
        Ok(())
    }
}

// ============================================================================
// Constants
// ============================================================================

pub const MAX_SWAP_PROGRAMS: usize = 8;
pub const MAX_POOLS: usize = 16;
pub const MAX_MODE_FEEDS: usize = 16;

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
pub struct InitGatewayConfig<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + GatewayConfig::INIT_SPACE,
        seeds = [b"gateway_config"],
        bump
    )]
    pub config: Account<'info, GatewayConfig>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ModifyConfig<'info> {
    #[account(
        mut,
        seeds = [b"gateway_config"],
        bump = config.bump,
        has_one = authority @ GatewayError::Unauthorized
    )]
    pub config: Account<'info, GatewayConfig>,
    
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct SwapAndFundSession<'info> {
    #[account(
        seeds = [b"gateway_config"],
        bump = config.bump
    )]
    pub config: Account<'info, GatewayConfig>,
    
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// CHECK: Validated by session_escrow program
    #[account(mut)]
    pub session: AccountInfo<'info>,
    
    #[account(mut)]
    pub user_input_token: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub escrow_token_account: Account<'info, TokenAccount>,
    
    pub input_price_update: Account<'info, PriceUpdateV2>,
    pub output_price_update: Account<'info, PriceUpdateV2>,
    
    /// CHECK: Validated against allowlist
    pub swap_program: AccountInfo<'info>,
    
    /// CHECK: Passed to swap program
    pub pool: AccountInfo<'info>,
    
    pub input_mint: Account<'info, Mint>,
    pub output_mint: Account<'info, Mint>,
    
    pub token_program: Program<'info, Token>,
    
    /// CHECK: session_escrow program
    pub session_escrow_program: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct SwapAndDepositCollateral<'info> {
    #[account(
        seeds = [b"gateway_config"],
        bump = config.bump
    )]
    pub config: Account<'info, GatewayConfig>,
    
    #[account(mut)]
    pub provider: Signer<'info>,
    
    /// CHECK: Validated by collateral_vault program
    #[account(mut)]
    pub position: AccountInfo<'info>,
    
    #[account(mut)]
    pub provider_input_token: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    pub input_price_update: Account<'info, PriceUpdateV2>,
    pub output_price_update: Account<'info, PriceUpdateV2>,
    
    /// CHECK: Validated against allowlist
    pub swap_program: AccountInfo<'info>,
    
    /// CHECK: Passed to swap program
    pub pool: AccountInfo<'info>,
    
    pub input_mint: Account<'info, Mint>,
    pub collateral_mint: Account<'info, Mint>,
    
    pub token_program: Program<'info, Token>,
    
    /// CHECK: collateral_vault program
    pub collateral_vault_program: AccountInfo<'info>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct GatewayConfig {
    pub authority: Pubkey,
    pub max_slippage_bps: u16,
    pub max_trade_size: u64,
    pub pyth_max_age_seconds: u64,
    pub pyth_max_conf_ratio_bps: u16,
    pub native_feed_id: [u8; 32],
    
    #[max_len(8)]
    pub allowlisted_swap_programs: [Pubkey; MAX_SWAP_PROGRAMS],
    pub swap_program_count: u8,
    
    #[max_len(16)]
    pub allowlisted_pools: [Pubkey; MAX_POOLS],
    pub pool_count: u8,
    
    #[max_len(16)]
    pub mode_feeds: [ModeFeed; MAX_MODE_FEEDS],
    pub mode_feed_count: u8,
    
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, InitSpace)]
pub struct ModeFeed {
    pub mint: Pubkey,
    pub feed_id: [u8; 32],
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct GatewayConfigInitialized {
    pub config: Pubkey,
    pub authority: Pubkey,
    pub max_slippage_bps: u16,
    pub max_trade_size: u64,
}

#[event]
pub struct SwapProgramAdded {
    pub program_id: Pubkey,
}

#[event]
pub struct SwapProgramRemoved {
    pub program_id: Pubkey,
}

#[event]
pub struct PoolAdded {
    pub pool: Pubkey,
}

#[event]
pub struct ModeFeedAdded {
    pub mint: Pubkey,
    pub feed_id: [u8; 32],
}

#[event]
pub struct SwapAndFundStubbed {
    pub user: Pubkey,
    pub amount_in: u64,
    pub session: Pubkey,
}

#[event]
pub struct SwapAndDepositStubbed {
    pub provider: Pubkey,
    pub amount_in: u64,
    pub mode_id: u32,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum GatewayError {
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Maximum swap programs reached")]
    MaxSwapProgramsReached,
    #[msg("Maximum pools reached")]
    MaxPoolsReached,
    #[msg("Maximum mode feeds reached")]
    MaxModeFeedsReached,
    #[msg("Already allowlisted")]
    AlreadyAllowlisted,
    #[msg("Not allowlisted")]
    NotAllowlisted,
    #[msg("Swap program not allowlisted")]
    SwapProgramNotAllowlisted,
    #[msg("Pool not allowlisted")]
    PoolNotAllowlisted,
    #[msg("Trade size exceeds maximum")]
    TradeTooLarge,
    #[msg("Slippage exceeds maximum")]
    SlippageExceeded,
    #[msg("Price feed not found for mint")]
    PriceFeedNotFound,
}
