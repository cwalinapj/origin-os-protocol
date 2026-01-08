use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, Transfer},
};

declare_id!("CoVau1t111111111111111111111111111111111111");

/// Collateral Vault Program (IMMUTABLE)
/// 
/// Custody provider collateral, track free vs reserved, pay claims.
/// 
/// INVARIANTS:
/// - reserved <= total
/// - withdrawals cannot reduce total below reserved
/// - claim payouts only come from reserved
#[program]
pub mod collateral_vault {
    use super::*;

    /// Deposit collateral and create position (mints NFT on first deposit)
    pub fn deposit(ctx: Context<Deposit>, mode_id: u32, amount: u64) -> Result<()> {
        require!(amount > 0, ErrorCode::ZeroAmount);
        
        // Capture values BEFORE mutable borrow
        let provider_key = ctx.accounts.provider.key();
        let mint_key = ctx.accounts.collateral_mint.key();
        let nft_mint_key = ctx.accounts.position_nft_mint.key();
        let position_bump = ctx.bumps.position;
        
        let position = &mut ctx.accounts.position;
        let is_new = position.total == 0 && position.provider == Pubkey::default();
        
        if is_new {
            position.provider = provider_key;
            position.mode_id = mode_id;
            position.mint = mint_key;
            position.total = 0;
            position.reserved = 0;
            position.position_nft_mint = nft_mint_key;
            position.bump = position_bump;
        }
        
        // Update total first (we'll do NFT mint and transfer after releasing mutable borrow)
        position.total = position.total.checked_add(amount).ok_or(ErrorCode::Overflow)?;
        let new_total = position.total;
        
        // Release mutable borrow by dropping position reference
        let _ = position;
        
        // Transfer collateral to vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.provider_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.provider.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;
        
        // Mint position NFT on first deposit
        if is_new {
            let mode_id_bytes = mode_id.to_le_bytes();
            let seeds: &[&[u8]] = &[
                b"pos",
                provider_key.as_ref(),
                &mode_id_bytes,
                &[position_bump],
            ];
            let signer_seeds = &[seeds];
            
            let mint_accounts = token::MintTo {
                mint: ctx.accounts.position_nft_mint.to_account_info(),
                to: ctx.accounts.provider_nft_account.to_account_info(),
                authority: ctx.accounts.position.to_account_info(),
            };
            let mint_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                mint_accounts,
                signer_seeds,
            );
            token::mint_to(mint_ctx, 1)?;
        }
        
        emit!(CollateralDeposited {
            provider: provider_key,
            mode_id,
            amount,
            new_total,
        });
        
        Ok(())
    }

    /// Withdraw free (unreserved) collateral
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        require!(amount > 0, ErrorCode::ZeroAmount);
        
        // Capture values BEFORE mutable borrow
        let position_info = ctx.accounts.position.to_account_info();
        let vault_info = ctx.accounts.vault_token_account.to_account_info();
        let provider_token_info = ctx.accounts.provider_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        
        let position = &mut ctx.accounts.position;
        
        let free = position.total.saturating_sub(position.reserved);
        require!(amount <= free, ErrorCode::InsufficientFreeCollateral);
        
        // Build signer seeds
        let mode_id_bytes = position.mode_id.to_le_bytes();
        let provider_key = position.provider;
        let bump = position.bump;
        
        // Update state
        position.total = position.total.checked_sub(amount).ok_or(ErrorCode::Underflow)?;
        let new_total = position.total;
        let mode_id = position.mode_id;
        
        // Drop mutable borrow
        let _ = position;
        
        // Transfer
        let seeds: &[&[u8]] = &[
            b"pos",
            provider_key.as_ref(),
            &mode_id_bytes,
            &[bump],
        ];
        let signer_seeds = &[seeds];
        
        let cpi_accounts = Transfer {
            from: vault_info,
            to: provider_token_info,
            authority: position_info,
        };
        let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, amount)?;
        
        emit!(CollateralWithdrawn {
            provider: provider_key,
            mode_id,
            amount,
            new_total,
        });
        
        Ok(())
    }

    /// Reserve collateral for a session (CPI from session_escrow)
    pub fn reserve(ctx: Context<Reserve>, session: Pubkey, amount_r: u64) -> Result<()> {
        let position = &mut ctx.accounts.position;
        
        let free = position.total.saturating_sub(position.reserved);
        require!(amount_r <= free, ErrorCode::InsufficientFreeCollateral);
        
        position.reserved = position.reserved.checked_add(amount_r).ok_or(ErrorCode::Overflow)?;
        require!(position.reserved <= position.total, ErrorCode::ReservedExceedsTotal);
        
        let provider = position.provider;
        let new_reserved = position.reserved;
        
        emit!(CollateralReserved {
            provider,
            session,
            amount: amount_r,
            new_reserved,
        });
        
        Ok(())
    }

    /// Release reserved collateral (session completed successfully)
    pub fn release(ctx: Context<Release>, session: Pubkey, amount_r: u64) -> Result<()> {
        let position = &mut ctx.accounts.position;
        
        require!(amount_r <= position.reserved, ErrorCode::ReleaseExceedsReserved);
        
        position.reserved = position.reserved.checked_sub(amount_r).ok_or(ErrorCode::Underflow)?;
        
        let provider = position.provider;
        let new_reserved = position.reserved;
        
        emit!(CollateralReleased {
            provider,
            session,
            amount: amount_r,
            new_reserved,
        });
        
        Ok(())
    }

    /// Slash collateral and pay to user (claim payout)
    pub fn slash_and_pay(
        ctx: Context<SlashAndPay>,
        session: Pubkey,
        payout_amount: u64,
    ) -> Result<()> {
        // Capture values BEFORE mutable borrow
        let position_info = ctx.accounts.position.to_account_info();
        let vault_info = ctx.accounts.vault_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let user_owner = ctx.accounts.user_token_account.owner;
        
        let position = &mut ctx.accounts.position;
        
        require!(payout_amount <= position.reserved, ErrorCode::PayoutExceedsReserved);
        
        // Capture for signer seeds
        let mode_id_bytes = position.mode_id.to_le_bytes();
        let provider_key = position.provider;
        let bump = position.bump;
        
        // Update state
        position.reserved = position.reserved.checked_sub(payout_amount).ok_or(ErrorCode::Underflow)?;
        position.total = position.total.checked_sub(payout_amount).ok_or(ErrorCode::Underflow)?;
        let new_total = position.total;
        let new_reserved = position.reserved;
        
        // Drop mutable borrow
        let _ = position;
        
        // Transfer
        let seeds: &[&[u8]] = &[
            b"pos",
            provider_key.as_ref(),
            &mode_id_bytes,
            &[bump],
        ];
        let signer_seeds = &[seeds];
        
        let cpi_accounts = Transfer {
            from: vault_info,
            to: user_token_info,
            authority: position_info,
        };
        let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, payout_amount)?;
        
        emit!(CollateralSlashed {
            provider: provider_key,
            session,
            payout_amount,
            user: user_owner,
            new_total,
            new_reserved,
        });
        
        Ok(())
    }
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
#[instruction(mode_id: u32)]
pub struct Deposit<'info> {
    #[account(
        init_if_needed,
        payer = provider,
        space = 8 + ProviderPosition::INIT_SPACE,
        seeds = [b"pos", provider.key().as_ref(), &mode_id.to_le_bytes()],
        bump
    )]
    pub position: Account<'info, ProviderPosition>,
    
    #[account(
        init_if_needed,
        payer = provider,
        associated_token::mint = collateral_mint,
        associated_token::authority = position
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub provider_token_account: Account<'info, TokenAccount>,
    
    pub collateral_mint: Account<'info, Mint>,
    
    /// Position NFT mint (created externally, authority = position PDA)
    #[account(mut)]
    pub position_nft_mint: Account<'info, Mint>,
    
    /// Provider's NFT token account
    #[account(
        init_if_needed,
        payer = provider,
        associated_token::mint = position_nft_mint,
        associated_token::authority = provider
    )]
    pub provider_nft_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub provider: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [b"pos", provider.key().as_ref(), &position.mode_id.to_le_bytes()],
        bump = position.bump,
        has_one = provider @ ErrorCode::WrongProvider
    )]
    pub position: Account<'info, ProviderPosition>,
    
    #[account(
        mut,
        associated_token::mint = position.mint,
        associated_token::authority = position
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub provider_token_account: Account<'info, TokenAccount>,
    
    pub provider: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Reserve<'info> {
    #[account(
        mut,
        seeds = [b"pos", position.provider.as_ref(), &position.mode_id.to_le_bytes()],
        bump = position.bump
    )]
    pub position: Account<'info, ProviderPosition>,
    
    /// Provider must sign to authorize reservation
    pub provider: Signer<'info>,
}

#[derive(Accounts)]
pub struct Release<'info> {
    #[account(
        mut,
        seeds = [b"pos", position.provider.as_ref(), &position.mode_id.to_le_bytes()],
        bump = position.bump
    )]
    pub position: Account<'info, ProviderPosition>,
    
    /// Session escrow authority (CPI signer)
    pub session_authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct SlashAndPay<'info> {
    #[account(
        mut,
        seeds = [b"pos", position.provider.as_ref(), &position.mode_id.to_le_bytes()],
        bump = position.bump
    )]
    pub position: Account<'info, ProviderPosition>,
    
    #[account(
        mut,
        associated_token::mint = position.mint,
        associated_token::authority = position
    )]
    pub vault_token_account: Account<'info, TokenAccount>,
    
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    
    /// Session escrow authority (CPI signer)
    pub session_authority: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct ProviderPosition {
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
pub struct CollateralDeposited {
    pub provider: Pubkey,
    pub mode_id: u32,
    pub amount: u64,
    pub new_total: u64,
}

#[event]
pub struct CollateralWithdrawn {
    pub provider: Pubkey,
    pub mode_id: u32,
    pub amount: u64,
    pub new_total: u64,
}

#[event]
pub struct CollateralReserved {
    pub provider: Pubkey,
    pub session: Pubkey,
    pub amount: u64,
    pub new_reserved: u64,
}

#[event]
pub struct CollateralReleased {
    pub provider: Pubkey,
    pub session: Pubkey,
    pub amount: u64,
    pub new_reserved: u64,
}

#[event]
pub struct CollateralSlashed {
    pub provider: Pubkey,
    pub session: Pubkey,
    pub payout_amount: u64,
    pub user: Pubkey,
    pub new_total: u64,
    pub new_reserved: u64,
}

// ============================================================================
// Errors
// ============================================================================

#[error_code]
pub enum ErrorCode {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Arithmetic underflow")]
    Underflow,
    #[msg("Insufficient free collateral")]
    InsufficientFreeCollateral,
    #[msg("Reserved exceeds total")]
    ReservedExceedsTotal,
    #[msg("Release amount exceeds reserved")]
    ReleaseExceedsReserved,
    #[msg("Payout exceeds reserved collateral")]
    PayoutExceedsReserved,
    #[msg("Wrong provider")]
    WrongProvider,
}
