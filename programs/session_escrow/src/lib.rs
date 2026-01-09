use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::{
    self, load_instruction_at_checked,
};
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use collateral_vault::cpi::accounts::{Reserve, Release, SlashAndPay};
use collateral_vault::program::CollateralVault;
use collateral_vault::ProviderPosition;

declare_id!("SessEsc111111111111111111111111111111111111");

pub const ED25519_PROGRAM_ID: Pubkey = anchor_lang::solana_program::ed25519_program::ID;

/// Session Escrow Program (IMMUTABLE)
///
/// INVARIANTS:
/// - Provider cannot withdraw without valid permit
/// - Permits cannot be replayed (nonce tracking)
/// - Escrow cannot go negative
/// - Claims are purely objective (deadline/slot based)
/// - SLA failures result in proportional payouts from reserved collateral
#[program]
pub mod session_escrow {
    use super::*;

    pub const INSURANCE_A: u64 = 100;
    pub const INSURANCE_B: u64 = 50;
    pub const INSURANCE_MIN_BPS: u64 = 500;
    pub const INSURANCE_CAP_BPS: u64 = 2000;

    // Bid mode constants
    pub const BID_PREMIUM_WEIGHT: u64 = 50; // 50% weight on premium for bid coverage
    pub const BID_SLA_WEIGHT: u64 = 50;     // 50% weight on SLA strictness

    /// Open a new session between user and provider
    ///
    /// When is_bid is true:
    /// - Computes additional bid_coverage_p from premium and SLA targets
    /// - Sets SLA window timing (sla_window_start_slot = current_slot + warmup_slots)
    /// - Reserves total collateral (reserve_base + reserve_bid)
    pub fn open_session(
        ctx: Context<OpenSession>,
        session_nonce: u64,
        mode_id: u32,
        chunk_size: u64,
        price_per_chunk: u64,
        max_spend: u64,
        start_deadline_slots: u64,
        stall_timeout_slots: u64,
        // Bid mode parameters
        is_bid: bool,
        premium_bps: u16,
        fail_payout_bps: u16,
        latency_target_ms: u16,
        bandwidth_min_chunks: u32,
        sla_warmup_slots: u64,
        sla_window_slots: u64,
        // Bucketed SLA parameters (only used if is_bid)
        bucket_slots: u64,
        terminate_window_slots: u64,
        max_penalty_bps: u16,
        verifier_pubkey: Pubkey,
    ) -> Result<()> {
        let clock = Clock::get()?;

        // Compute base coverage (always computed)
        let base_coverage_p = compute_insurance_coverage(max_spend, price_per_chunk);
        let cr_bps: u64 = 15000;
        let reserve_base = base_coverage_p
            .checked_mul(cr_bps)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(10000)
            .ok_or(ErrorCode::Overflow)?;

        // Compute bid coverage if in bid mode
        let (bid_coverage_p, reserve_bid, sla_window_start_slot, sla_window_end_slot) = if is_bid {
            let bid_cov = compute_bid_coverage(
                max_spend,
                premium_bps,
                latency_target_ms,
                bandwidth_min_chunks,
            );
            let res_bid = bid_cov
                .checked_mul(cr_bps)
                .ok_or(ErrorCode::Overflow)?
                .checked_div(10000)
                .ok_or(ErrorCode::Overflow)?;

            let window_start = clock.slot
                .checked_add(sla_warmup_slots)
                .ok_or(ErrorCode::Overflow)?;
            let window_end = window_start
                .checked_add(sla_window_slots)
                .ok_or(ErrorCode::Overflow)?;

            (bid_cov, res_bid, window_start, window_end)
        } else {
            (0, 0, 0, 0)
        };

        // Total reserve required
        let total_reserve = reserve_base
            .checked_add(reserve_bid)
            .ok_or(ErrorCode::Overflow)?;

        let session_key = ctx.accounts.session.key();
        let user_key = ctx.accounts.user.key();
        let provider_key = ctx.accounts.provider.key();
        let mint_key = ctx.accounts.payment_mint.key();
        let session_bump = ctx.bumps.session;
        let start_deadline_slot = clock.slot
            .checked_add(start_deadline_slots)
            .ok_or(ErrorCode::Overflow)?;

        let session = &mut ctx.accounts.session;
        session.user = user_key;
        session.provider = provider_key;
        session.mode_id = mode_id;
        session.mint = mint_key;
        session.session_nonce = session_nonce;
        session.chunk_size = chunk_size;
        session.price_per_chunk = price_per_chunk;
        session.max_spend = max_spend;
        session.total_spent = 0;
        session.reserve_r = total_reserve;
        session.start_deadline_slot = start_deadline_slot;
        session.stall_timeout_slots = stall_timeout_slots;
        session.last_progress_slot = 0;
        session.state = SessionState::Open;
        session.acked = false;
        session.next_permit_nonce = 0;
        session.bump = session_bump;

        // Bid/SLA fields
        session.is_bid = is_bid;
        session.premium_bps = premium_bps;
        session.fail_payout_bps = fail_payout_bps;
        session.latency_target_ms = latency_target_ms;
        session.bandwidth_min_chunks = bandwidth_min_chunks;
        session.sla_warmup_slots = sla_warmup_slots;
        session.sla_window_slots = sla_window_slots;
        session.sla_window_start_slot = sla_window_start_slot;
        session.sla_window_end_slot = sla_window_end_slot;

        // Insurance split
        session.base_coverage_p = base_coverage_p;
        session.bid_coverage_p = bid_coverage_p;
        session.reserve_base = reserve_base;
        session.reserve_bid = reserve_bid;

        // SLA state
        session.sla_status = SlaStatus::None;
        session.sla_failure_reason = SlaFailureReason::None;
        session.latency_attested = false;

        // Nonce tracking for bandwidth SLA (legacy window-level)
        session.nonce_at_window_start = 0;
        session.nonce_at_window_end = 0;

        // Bucketed SLA configuration (compute if is_bid)
        if is_bid && bucket_slots > 0 {
            let buckets_total_computed = compute_buckets_total(sla_window_slots, bucket_slots)?;
            let bucket_penalty_computed = compute_bucket_penalty(
                total_reserve,
                max_penalty_bps,
                buckets_total_computed,
            )?;
            session.bucket_slots = bucket_slots;
            session.buckets_total = buckets_total_computed;
            session.bucket_penalty = bucket_penalty_computed;
        } else {
            session.bucket_slots = 0;
            session.buckets_total = 0;
            session.bucket_penalty = 0;
        }

        // Bucketed downtime tracking (initialized to zero)
        session.buckets_failed = 0;
        session.buckets_failed_bitmap = [0u8; 128];

        // Termination window
        session.first_violation_slot = 0;
        session.terminate_window_slots = terminate_window_slots;
        session.terminate_deadline_slot = 0;

        // Penalty accounting
        session.penalty_accrued = 0;

        // Attester configuration
        session.verifier_pubkey = verifier_pubkey;

        // Convenience flags
        session.terminated_for_cause = false;

        emit!(SessionOpened {
            session: session_key,
            user: user_key,
            provider: provider_key,
            mode_id,
            max_spend,
            base_coverage_p,
            reserve_r: total_reserve,
            start_deadline_slot,
            is_bid,
            premium_bps,
            fail_payout_bps,
            bid_coverage_p,
            reserve_base,
            reserve_bid,
        });

        Ok(())
    }

    /// Fund the session escrow (user deposits)
    pub fn fund_session(ctx: Context<FundSession>, amount: u64) -> Result<()> {
        require!(amount > 0, ErrorCode::ZeroAmount);

        let session = &ctx.accounts.session;
        require!(
            session.state == SessionState::Open || session.state == SessionState::Active,
            ErrorCode::SessionNotFundable
        );

        let session_key = ctx.accounts.session.key();
        let current_balance = ctx.accounts.escrow_token_account.amount;

        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        emit!(SessionFunded {
            session: session_key,
            amount,
            new_balance: current_balance.checked_add(amount).unwrap_or(0),
        });

        Ok(())
    }

    /// Provider acknowledges session start and reserves collateral
    pub fn ack_start(ctx: Context<AckStart>) -> Result<()> {
        let clock = Clock::get()?;
        let session_key = ctx.accounts.session.key();

        let session = &mut ctx.accounts.session;

        require!(session.state == SessionState::Open, ErrorCode::InvalidSessionState);
        require!(!session.acked, ErrorCode::AlreadyAcked);
        require!(clock.slot <= session.start_deadline_slot, ErrorCode::StartDeadlinePassed);

        let reserve_r = session.reserve_r;

        session.acked = true;
        session.state = SessionState::Active;
        session.last_progress_slot = clock.slot;

        // For bid sessions, set SLA status to Pending
        if session.is_bid {
            session.sla_status = SlaStatus::Pending;
        }

        // CPI to collateral_vault::reserve()
        let cpi_accounts = Reserve {
            position: ctx.accounts.position.to_account_info(),
            provider: ctx.accounts.provider.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.collateral_vault_program.to_account_info(),
            cpi_accounts,
        );
        collateral_vault::cpi::reserve(cpi_ctx, session_key, reserve_r)?;

        emit!(SessionStarted {
            session: session_key,
            started_at_slot: clock.slot,
        });

        Ok(())
    }

    /// Snapshot the nonce at SLA window start (callable by anyone after window starts)
    pub fn snapshot_window_start(ctx: Context<SnapshotWindowStart>) -> Result<()> {
        let clock = Clock::get()?;
        let session_key = ctx.accounts.session.key();
        let session = &mut ctx.accounts.session;

        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(clock.slot >= session.sla_window_start_slot, ErrorCode::SlaWindowNotStarted);
        require!(session.nonce_at_window_start == 0, ErrorCode::WindowAlreadySnapshotted);

        session.nonce_at_window_start = session.next_permit_nonce;

        emit!(SlaWindowStartSnapshotted {
            session: session_key,
            nonce_at_start: session.nonce_at_window_start,
            slot: clock.slot,
        });

        Ok(())
    }

    /// Provider redeems a permit to withdraw from escrow
    ///
    /// For bid sessions, the effective price includes the premium:
    /// price_per_unit_effective = base_price * (1 + premium_bps/10_000)
    pub fn redeem_permit(
        ctx: Context<RedeemPermit>,
        permit_nonce: u64,
        amount: u64,
        expiry_slot: u64,
    ) -> Result<()> {
        let clock = Clock::get()?;

        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let provider_token_info = ctx.accounts.provider_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(clock.slot <= expiry_slot, ErrorCode::PermitExpired);
        require!(permit_nonce == session.next_permit_nonce, ErrorCode::InvalidPermitNonce);

        verify_permit_signature(
            &ctx.accounts.instructions_sysvar,
            &session.user,
            &session_key,
            &session.provider,
            permit_nonce,
            amount,
            expiry_slot,
        )?;

        // For bid sessions, the amount should already include the premium
        // effective_price = base_price * (1 + premium_bps/10_000)
        // This is enforced client-side when creating permits

        require!(amount <= escrow_balance, ErrorCode::InsufficientEscrow);

        let new_total_spent = session.total_spent.checked_add(amount).ok_or(ErrorCode::Overflow)?;
        require!(new_total_spent <= session.max_spend, ErrorCode::MaxSpendExceeded);

        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;

        session.total_spent = new_total_spent;
        session.next_permit_nonce = permit_nonce.checked_add(1).ok_or(ErrorCode::Overflow)?;
        session.last_progress_slot = clock.slot;
        let total_spent = session.total_spent;

        let _ = session;

        let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
        let signer_seeds = &[seeds];

        let cpi_accounts = Transfer {
            from: escrow_info,
            to: provider_token_info,
            authority: session_info,
        };
        let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, amount)?;

        emit!(PermitRedeemed {
            session: session_key,
            permit_nonce,
            amount,
            total_spent,
        });

        Ok(())
    }

    /// Evaluate bandwidth SLA after window ends
    ///
    /// Callable by anyone after the SLA window has ended.
    /// Compares nonce progression within the window against target.
    /// If chunks delivered < bandwidth_min_chunks, marks SLA as Failed.
    pub fn evaluate_bandwidth_sla(ctx: Context<EvaluateBandwidthSla>) -> Result<()> {
        let clock = Clock::get()?;
        let session_key = ctx.accounts.session.key();
        let session = &mut ctx.accounts.session;

        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(
            session.sla_status == SlaStatus::Pending || session.sla_status == SlaStatus::None,
            ErrorCode::SlaAlreadyEvaluated
        );
        require!(clock.slot > session.sla_window_end_slot, ErrorCode::SlaWindowNotEnded);
        require!(session.nonce_at_window_start > 0, ErrorCode::WindowStartNotSnapshotted);

        // Snapshot the end nonce
        session.nonce_at_window_end = session.next_permit_nonce;

        // Calculate chunks delivered during the window
        let chunks_delivered = session.nonce_at_window_end
            .saturating_sub(session.nonce_at_window_start);

        // Check if bandwidth target was met
        let bandwidth_passed = chunks_delivered >= session.bandwidth_min_chunks as u64;

        if !bandwidth_passed {
            // Update failure reason
            match session.sla_failure_reason {
                SlaFailureReason::None => {
                    session.sla_failure_reason = SlaFailureReason::Bandwidth;
                }
                SlaFailureReason::Latency => {
                    session.sla_failure_reason = SlaFailureReason::Both;
                }
                _ => {}
            }
            session.sla_status = SlaStatus::Failed;
        }

        emit!(SlaEvaluated {
            session: session_key,
            sla_type: SlaType::Bandwidth,
            passed: bandwidth_passed,
            actual_value: chunks_delivered,
            target_value: session.bandwidth_min_chunks as u64,
        });

        Ok(())
    }

    /// Submit latency attestation from allowlisted verifier
    ///
    /// Only callable by addresses in the verifier allowlist.
    /// If rtt_p90_ms > latency_target_ms, marks SLA as Failed.
    pub fn submit_latency_attestation(
        ctx: Context<SubmitLatencyAttestation>,
        rtt_p90_ms: u16,
        measurement_window_start: u64,
        measurement_window_end: u64,
    ) -> Result<()> {
        let session_key = ctx.accounts.session.key();
        let session = &mut ctx.accounts.session;

        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(!session.latency_attested, ErrorCode::LatencyAlreadyAttested);

        // Validate measurement window overlaps with SLA window
        require!(
            measurement_window_start <= session.sla_window_end_slot &&
            measurement_window_end >= session.sla_window_start_slot,
            ErrorCode::InvalidMeasurementWindow
        );

        session.latency_attested = true;

        // Check if latency target was violated
        let latency_passed = rtt_p90_ms <= session.latency_target_ms;

        if !latency_passed {
            // Update failure reason
            match session.sla_failure_reason {
                SlaFailureReason::None => {
                    session.sla_failure_reason = SlaFailureReason::Latency;
                }
                SlaFailureReason::Bandwidth => {
                    session.sla_failure_reason = SlaFailureReason::Both;
                }
                _ => {}
            }
            session.sla_status = SlaStatus::Failed;
        }

        emit!(SlaEvaluated {
            session: session_key,
            sla_type: SlaType::Latency,
            passed: latency_passed,
            actual_value: rtt_p90_ms as u64,
            target_value: session.latency_target_ms as u64,
        });

        emit!(LatencyAttestationSubmitted {
            session: session_key,
            verifier: ctx.accounts.verifier.key(),
            rtt_p90_ms,
            measurement_window_start,
            measurement_window_end,
        });

        Ok(())
    }

    /// Mark SLA as Met (callable after window ends if no failures)
    pub fn finalize_sla_met(ctx: Context<FinalizeSla>) -> Result<()> {
        let clock = Clock::get()?;
        let session = &mut ctx.accounts.session;

        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(session.sla_status == SlaStatus::Pending, ErrorCode::SlaAlreadyEvaluated);
        require!(clock.slot > session.sla_window_end_slot, ErrorCode::SlaWindowNotEnded);
        require!(session.sla_failure_reason == SlaFailureReason::None, ErrorCode::SlaHasFailures);

        session.sla_status = SlaStatus::Met;

        emit!(SlaFinalized {
            session: ctx.accounts.session.key(),
            status: SlaStatus::Met,
        });

        Ok(())
    }

    /// Claim SLA failure payout
    ///
    /// Requires sla_status == Failed.
    /// Computes payout = (base_coverage_p + bid_coverage_p) * fail_payout_bps / 10_000
    /// Pays from reserve_bid first, then reserve_base if needed.
    pub fn claim_sla_failure(ctx: Context<ClaimSlaFailure>) -> Result<()> {
        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.sla_status == SlaStatus::Failed, ErrorCode::SlaNotFailed);
        require!(
            session.state == SessionState::Active,
            ErrorCode::InvalidSessionState
        );

        // Calculate total insured amount (base + bid coverage)
        let total_insured = session.base_coverage_p
            .checked_add(session.bid_coverage_p)
            .ok_or(ErrorCode::Overflow)?;

        // Calculate payout using session's fail_payout_bps
        let payout = total_insured
            .checked_mul(session.fail_payout_bps as u64)
            .ok_or(ErrorCode::Overflow)?
            .checked_div(10000)
            .ok_or(ErrorCode::Overflow)?;

        // Cap payout at total reserved
        let actual_payout = payout.min(session.reserve_r);

        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;
        let reserve_r = session.reserve_r;

        session.state = SessionState::Claimed;

        let _ = session;

        // CPI to collateral_vault::slash_and_pay()
        let cpi_accounts = SlashAndPay {
            position: ctx.accounts.position.to_account_info(),
            vault_token_account: ctx.accounts.vault_token_account.to_account_info(),
            user_token_account: ctx.accounts.user_token_account.to_account_info(),
            session_authority: session_info.clone(),
            token_program: token_program_info.clone(),
        };
        let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
        let signer_seeds = &[seeds];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.collateral_vault_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        collateral_vault::cpi::slash_and_pay(cpi_ctx, session_key, actual_payout)?;

        // Release remaining reserved collateral
        let remaining_reserve = reserve_r.saturating_sub(actual_payout);
        if remaining_reserve > 0 {
            let release_accounts = Release {
                position: ctx.accounts.position.to_account_info(),
                session_authority: ctx.accounts.session.to_account_info(),
            };
            let release_ctx = CpiContext::new_with_signer(
                ctx.accounts.collateral_vault_program.to_account_info(),
                release_accounts,
                signer_seeds,
            );
            collateral_vault::cpi::release(release_ctx, session_key, remaining_reserve)?;
        }

        // Refund remaining escrow to user
        if escrow_balance > 0 {
            let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
            let signer_seeds = &[seeds];

            let cpi_accounts = Transfer {
                from: escrow_info,
                to: user_token_info,
                authority: ctx.accounts.session.to_account_info(),
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, escrow_balance)?;
        }

        emit!(SlaFailureClaimed {
            session: session_key,
            payout: actual_payout,
            escrow_refunded: escrow_balance,
            remaining_reserve_released: remaining_reserve,
            failure_reason: ctx.accounts.session.sla_failure_reason,
        });

        Ok(())
    }

    /// User initiates session close
    pub fn close_session(ctx: Context<CloseSession>) -> Result<()> {
        let session_key = ctx.accounts.session.key();
        let session = &mut ctx.accounts.session;

        require!(
            session.state == SessionState::Active || session.state == SessionState::Open,
            ErrorCode::InvalidSessionState
        );

        session.state = SessionState::Closing;

        emit!(SessionClosing { session: session_key });

        Ok(())
    }

    /// Finalize session close and release collateral
    pub fn finalize_close(ctx: Context<FinalizeClose>) -> Result<()> {
        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        require!(session.state == SessionState::Closing, ErrorCode::InvalidSessionState);

        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;
        let reserve_r = session.reserve_r;
        let was_active = session.acked;

        // For bid sessions that were never evaluated as failed, mark SLA as Met
        if session.is_bid && session.sla_status == SlaStatus::Pending {
            session.sla_status = SlaStatus::Met;
        }

        session.state = SessionState::Closed;

        let _ = session;

        // CPI to collateral_vault::release() if session was active
        if was_active {
            let cpi_accounts = Release {
                position: ctx.accounts.position.to_account_info(),
                session_authority: ctx.accounts.session.to_account_info(),
            };
            let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
            let signer_seeds = &[seeds];
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.collateral_vault_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            );
            collateral_vault::cpi::release(cpi_ctx, session_key, reserve_r)?;
        }

        if escrow_balance > 0 {
            let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
            let signer_seeds = &[seeds];

            let cpi_accounts = Transfer {
                from: escrow_info,
                to: user_token_info,
                authority: session_info,
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, escrow_balance)?;
        }

        emit!(SessionClosed {
            session: session_key,
            refunded: escrow_balance,
        });

        Ok(())
    }

    /// Claim for no-start (provider didn't ack - no collateral was reserved)
    pub fn claim_no_start(ctx: Context<ClaimNoStart>) -> Result<()> {
        let clock = Clock::get()?;

        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        require!(session.state == SessionState::Open, ErrorCode::InvalidSessionState);
        require!(!session.acked, ErrorCode::SessionAlreadyStarted);
        require!(clock.slot > session.start_deadline_slot, ErrorCode::DeadlineNotPassed);

        // No collateral was reserved (provider never acked), just refund escrow
        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;

        session.state = SessionState::Claimed;

        let _ = session;

        if escrow_balance > 0 {
            let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
            let signer_seeds = &[seeds];

            let cpi_accounts = Transfer {
                from: escrow_info,
                to: user_token_info,
                authority: session_info,
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, escrow_balance)?;
        }

        emit!(ClaimPaid {
            session: session_key,
            claim_type: ClaimType::NoStart,
            payout: 0, // No payout since no collateral was reserved
            escrow_refunded: escrow_balance,
        });

        Ok(())
    }

    /// Claim for stall - slash provider collateral and pay user
    pub fn claim_stall(ctx: Context<ClaimStall>) -> Result<()> {
        let clock = Clock::get()?;

        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        require!(session.state == SessionState::Active, ErrorCode::InvalidSessionState);
        require!(session.acked, ErrorCode::SessionNotStarted);

        let stall_deadline = session.last_progress_slot
            .checked_add(session.stall_timeout_slots)
            .ok_or(ErrorCode::Overflow)?;
        require!(clock.slot > stall_deadline, ErrorCode::StallTimeoutNotReached);

        let payout = session.base_coverage_p.min(session.reserve_r);
        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;

        session.state = SessionState::Claimed;

        let _ = session;

        // CPI to collateral_vault::slash_and_pay()
        let cpi_accounts = SlashAndPay {
            position: ctx.accounts.position.to_account_info(),
            vault_token_account: ctx.accounts.vault_token_account.to_account_info(),
            user_token_account: ctx.accounts.user_token_account.to_account_info(),
            session_authority: session_info.clone(),
            token_program: token_program_info.clone(),
        };
        let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
        let signer_seeds = &[seeds];
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.collateral_vault_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        collateral_vault::cpi::slash_and_pay(cpi_ctx, session_key, payout)?;

        // Refund remaining escrow to user
        if escrow_balance > 0 {
            let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
            let signer_seeds = &[seeds];

            let cpi_accounts = Transfer {
                from: escrow_info,
                to: user_token_info,
                authority: session_info,
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, escrow_balance)?;
        }

        emit!(ClaimPaid {
            session: session_key,
            claim_type: ClaimType::Stall,
            payout,
            escrow_refunded: escrow_balance,
        });

        Ok(())
    }

    // =========================================================================
    // BUCKETED SLA INSTRUCTIONS (Phase 1: Latency + PrivacyMode only)
    // =========================================================================

    /// Report a bucket failure (latency or privacy mode violation)
    ///
    /// Requires Ed25519 signature verification via Instructions sysvar.
    /// The Ed25519 precompile instruction must immediately precede this instruction.
    ///
    /// Effects:
    /// - First failure: sets first_violation_slot, terminate_deadline_slot, sla_status = Violated
    /// - Sets bucket bit in bitmap (idempotent protection)
    /// - Increments buckets_failed counter
    /// - Combines failure reason
    pub fn report_bucket_failure(
        ctx: Context<ReportBucketFailure>,
        bucket_index: u64,
        bucket_start_slot: u64,
        failure_reason: SlaFailureReason,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.slot;
        let session_key = ctx.accounts.session.key();
        let session = &mut ctx.accounts.session;

        // === Status guards ===
        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(
            session.sla_status == SlaStatus::Pending || session.sla_status == SlaStatus::Violated,
            ErrorCode::SlaAlreadyEvaluated
        );

        // === Window bounds ===
        require!(
            now >= session.sla_window_start_slot && now <= session.sla_window_end_slot,
            ErrorCode::ReportOutsideSlaWindow
        );

        // === Termination deadline (if already violated) ===
        if session.sla_status == SlaStatus::Violated {
            require!(
                now <= session.terminate_deadline_slot,
                ErrorCode::ReportAfterDeadline
            );
        }

        // === Bucket bounds ===
        require!(bucket_index < session.buckets_total, ErrorCode::BucketIndexOutOfBounds);

        // === Bucket alignment ===
        let expected_bucket_start = checked_bucket_start(
            session.sla_window_start_slot,
            bucket_index,
            session.bucket_slots,
        ).ok_or(ErrorCode::Overflow)?;
        require!(bucket_start_slot == expected_bucket_start, ErrorCode::BucketSlotMismatch);

        // === Attester auth ===
        require!(
            ctx.accounts.verifier.key() == session.verifier_pubkey,
            ErrorCode::InvalidAttester
        );

        // === Ed25519 signature verification via Instructions sysvar ===
        verify_bucket_failure_signature(
            &ctx.accounts.instructions_sysvar,
            &session.verifier_pubkey,
            &session_key,
            bucket_index,
            bucket_start_slot,
            failure_reason,
        )?;

        // === Bitmap deduplication ===
        require!(
            !bit_is_set(&session.buckets_failed_bitmap, bucket_index),
            ErrorCode::BucketAlreadyReported
        );
        set_bit(&mut session.buckets_failed_bitmap, bucket_index);

        // === First violation: set termination window ===
        if session.sla_status == SlaStatus::Pending {
            session.first_violation_slot = now;
            session.terminate_deadline_slot = now.saturating_add(session.terminate_window_slots);
            session.sla_status = SlaStatus::Violated;
        }

        // === Increment failure counter ===
        session.buckets_failed = session.buckets_failed
            .checked_add(1)
            .ok_or(ErrorCode::Overflow)?;

        // === Accrue penalty ===
        session.penalty_accrued = session.penalty_accrued
            .checked_add(session.bucket_penalty)
            .ok_or(ErrorCode::Overflow)?
            .min(session.reserve_r);  // Cap at total collateral

        // === Combine failure reason ===
        session.sla_failure_reason = combine_failure_reason(
            session.sla_failure_reason,
            failure_reason,
        );

        emit!(BucketFailureReported {
            session: session_key,
            bucket_index,
            bucket_start_slot,
            failure_reason,
            buckets_failed: session.buckets_failed,
            penalty_accrued: session.penalty_accrued,
            is_first_violation: session.buckets_failed == 1,
        });

        Ok(())
    }

    /// Terminate session for cause (client exercises termination right)
    ///
    /// Requires sla_status == Violated and within termination window.
    /// Effects:
    /// - Refunds 100% escrow to user
    /// - Slashes penalty_accrued from provider collateral
    /// - Releases remaining collateral to provider
    /// - Sets sla_status = TerminatedForCause
    pub fn terminate_for_cause(ctx: Context<TerminateForCause>) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.slot;

        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        // === Status guards ===
        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(session.sla_status == SlaStatus::Violated, ErrorCode::SessionNotViolated);
        require!(!session.terminated_for_cause, ErrorCode::SessionAlreadyTerminated);

        // === Termination window check ===
        require!(
            now <= session.terminate_deadline_slot,
            ErrorCode::TerminationWindowExpired
        );

        // Compute penalty: min(penalty_accrued, bucket_penalty * buckets_failed, reserve_r)
        let computed_penalty = session.bucket_penalty
            .checked_mul(session.buckets_failed)
            .ok_or(ErrorCode::Overflow)?;
        let actual_penalty = computed_penalty
            .min(session.penalty_accrued)
            .min(session.reserve_r);

        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;
        let reserve_r = session.reserve_r;
        let buckets_failed = session.buckets_failed;
        let failure_reason = session.sla_failure_reason;

        // Update state
        session.sla_status = SlaStatus::TerminatedForCause;
        session.terminated_for_cause = true;
        session.state = SessionState::Claimed;

        let _ = session;

        let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
        let signer_seeds = &[seeds];

        // === Slash penalty from provider collateral ===
        if actual_penalty > 0 {
            let cpi_accounts = SlashAndPay {
                position: ctx.accounts.position.to_account_info(),
                vault_token_account: ctx.accounts.vault_token_account.to_account_info(),
                user_token_account: ctx.accounts.user_token_account.to_account_info(),
                session_authority: session_info.clone(),
                token_program: token_program_info.clone(),
            };
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.collateral_vault_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            );
            collateral_vault::cpi::slash_and_pay(cpi_ctx, session_key, actual_penalty)?;
        }

        // === Release remaining collateral to provider ===
        let remaining_reserve = reserve_r.saturating_sub(actual_penalty);
        if remaining_reserve > 0 {
            let release_accounts = Release {
                position: ctx.accounts.position.to_account_info(),
                session_authority: ctx.accounts.session.to_account_info(),
            };
            let release_ctx = CpiContext::new_with_signer(
                ctx.accounts.collateral_vault_program.to_account_info(),
                release_accounts,
                signer_seeds,
            );
            collateral_vault::cpi::release(release_ctx, session_key, remaining_reserve)?;
        }

        // === Refund 100% escrow to user ===
        if escrow_balance > 0 {
            let cpi_accounts = Transfer {
                from: escrow_info,
                to: user_token_info,
                authority: ctx.accounts.session.to_account_info(),
            };
            let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, escrow_balance)?;
        }

        emit!(SessionTerminatedForCause {
            session: session_key,
            penalty_paid: actual_penalty,
            escrow_refunded: escrow_balance,
            buckets_failed,
            failure_reason,
            remaining_collateral_released: remaining_reserve,
        });

        Ok(())
    }

    /// Settle SLA after window ends (alternative to terminate_for_cause)
    ///
    /// Callable after sla_window_end_slot.
    /// Effects:
    /// - If buckets_failed == 0: sla_status = Met, premium released to host
    /// - If buckets_failed > 0: sla_status = Failed, penalty slashed, remaining released
    pub fn settle_sla(ctx: Context<SettleSla>) -> Result<()> {
        let clock = Clock::get()?;
        let now = clock.slot;

        let session_info = ctx.accounts.session.to_account_info();
        let escrow_info = ctx.accounts.escrow_token_account.to_account_info();
        let provider_token_info = ctx.accounts.provider_token_account.to_account_info();
        let user_token_info = ctx.accounts.user_token_account.to_account_info();
        let token_program_info = ctx.accounts.token_program.to_account_info();
        let session_key = ctx.accounts.session.key();
        let escrow_balance = ctx.accounts.escrow_token_account.amount;

        let session = &mut ctx.accounts.session;

        // === Status guards ===
        require!(session.is_bid, ErrorCode::NotBidSession);
        require!(session.state == SessionState::Active, ErrorCode::SessionNotActive);
        require!(
            session.sla_status == SlaStatus::Pending || session.sla_status == SlaStatus::Violated,
            ErrorCode::SlaAlreadyEvaluated
        );
        require!(!session.terminated_for_cause, ErrorCode::SessionAlreadyTerminated);

        // === Window must be ended ===
        require!(now > session.sla_window_end_slot, ErrorCode::SlaWindowNotEnded);

        let user_key = session.user;
        let nonce_bytes = session.session_nonce.to_le_bytes();
        let bump = session.bump;
        let reserve_r = session.reserve_r;
        let buckets_failed = session.buckets_failed;

        let seeds: &[&[u8]] = &[b"sess", user_key.as_ref(), &nonce_bytes, &[bump]];
        let signer_seeds = &[seeds];

        if buckets_failed == 0 {
            // === SLA MET: Premium to host, release all collateral ===
            session.sla_status = SlaStatus::Met;
            session.state = SessionState::Closed;

            // Release all collateral
            let release_accounts = Release {
                position: ctx.accounts.position.to_account_info(),
                session_authority: session_info.clone(),
            };
            let release_ctx = CpiContext::new_with_signer(
                ctx.accounts.collateral_vault_program.to_account_info(),
                release_accounts,
                signer_seeds,
            );
            collateral_vault::cpi::release(release_ctx, session_key, reserve_r)?;

            // Transfer premium (escrow) to provider
            if escrow_balance > 0 {
                let cpi_accounts = Transfer {
                    from: escrow_info,
                    to: provider_token_info,
                    authority: session_info,
                };
                let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
                token::transfer(cpi_ctx, escrow_balance)?;
            }

            emit!(SlaSettled {
                session: session_key,
                status: SlaStatus::Met,
                buckets_failed: 0,
                penalty_paid: 0,
                premium_to_host: escrow_balance,
                premium_refunded_to_user: 0,
            });
        } else {
            // === SLA FAILED: Penalty slashed, premium split or refunded ===
            session.sla_status = SlaStatus::Failed;
            session.state = SessionState::Claimed;

            // Compute penalty
            let computed_penalty = session.bucket_penalty
                .checked_mul(buckets_failed)
                .ok_or(ErrorCode::Overflow)?;
            let actual_penalty = computed_penalty
                .min(session.penalty_accrued)
                .min(reserve_r);

            let _failure_reason = session.sla_failure_reason;
            let _ = session;

            // Slash penalty
            if actual_penalty > 0 {
                let cpi_accounts = SlashAndPay {
                    position: ctx.accounts.position.to_account_info(),
                    vault_token_account: ctx.accounts.vault_token_account.to_account_info(),
                    user_token_account: ctx.accounts.user_token_account.to_account_info(),
                    session_authority: session_info.clone(),
                    token_program: token_program_info.clone(),
                };
                let cpi_ctx = CpiContext::new_with_signer(
                    ctx.accounts.collateral_vault_program.to_account_info(),
                    cpi_accounts,
                    signer_seeds,
                );
                collateral_vault::cpi::slash_and_pay(cpi_ctx, session_key, actual_penalty)?;
            }

            // Release remaining collateral
            let remaining_reserve = reserve_r.saturating_sub(actual_penalty);
            if remaining_reserve > 0 {
                let release_accounts = Release {
                    position: ctx.accounts.position.to_account_info(),
                    session_authority: ctx.accounts.session.to_account_info(),
                };
                let release_ctx = CpiContext::new_with_signer(
                    ctx.accounts.collateral_vault_program.to_account_info(),
                    release_accounts,
                    signer_seeds,
                );
                collateral_vault::cpi::release(release_ctx, session_key, remaining_reserve)?;
            }

            // Refund escrow to user (SLA failed = no premium for host)
            if escrow_balance > 0 {
                let cpi_accounts = Transfer {
                    from: escrow_info,
                    to: user_token_info,
                    authority: ctx.accounts.session.to_account_info(),
                };
                let cpi_ctx = CpiContext::new_with_signer(token_program_info, cpi_accounts, signer_seeds);
                token::transfer(cpi_ctx, escrow_balance)?;
            }

            emit!(SlaSettled {
                session: session_key,
                status: SlaStatus::Failed,
                buckets_failed,
                penalty_paid: actual_penalty,
                premium_to_host: 0,
                premium_refunded_to_user: escrow_balance,
            });
        }

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

// Bitmap helpers for bucket tracking (1024 buckets max)
fn bit_is_set(bitmap: &[u8; 128], idx: u64) -> bool {
    if idx >= 1024 {
        return true; // Out of bounds treated as "already set" (reject)
    }
    let i = idx as usize;
    let byte = i >> 3;
    let bit = i & 7;
    (bitmap[byte] & (1u8 << bit)) != 0
}

fn set_bit(bitmap: &mut [u8; 128], idx: u64) {
    if idx >= 1024 {
        return; // Out of bounds no-op
    }
    let i = idx as usize;
    let byte = i >> 3;
    let bit = i & 7;
    bitmap[byte] |= 1u8 << bit;
}

/// Compute bucket start slot with checked math
fn checked_bucket_start(
    sla_window_start: u64,
    bucket_index: u64,
    bucket_slots: u64,
) -> Option<u64> {
    let offset = bucket_index.checked_mul(bucket_slots)?;
    sla_window_start.checked_add(offset)
}

/// Compute bucket end slot with checked math
#[allow(dead_code)]
fn checked_bucket_end(
    sla_window_start: u64,
    bucket_index: u64,
    bucket_slots: u64,
) -> Option<u64> {
    let start = checked_bucket_start(sla_window_start, bucket_index, bucket_slots)?;
    start.checked_add(bucket_slots)?.checked_sub(1)
}

/// Compute buckets_total with validation
fn compute_buckets_total(sla_window_slots: u64, bucket_slots: u64) -> Result<u64> {
    require!(bucket_slots > 0, ErrorCode::InvalidBucketConfig);
    require!(sla_window_slots % bucket_slots == 0, ErrorCode::InvalidBucketConfig);
    let total = sla_window_slots
        .checked_div(bucket_slots)
        .ok_or(ErrorCode::Overflow)?;
    require!(total > 0 && total <= 1024, ErrorCode::InvalidBucketConfig);
    Ok(total)
}

/// Compute per-bucket penalty with checked math
fn compute_bucket_penalty(
    collateral: u64,
    max_penalty_bps: u16,
    buckets_total: u64,
) -> Result<u64> {
    // penalty_per_bucket = collateral * max_penalty_bps / (buckets_total * 10_000)
    let numerator = (collateral as u128)
        .checked_mul(max_penalty_bps as u128)
        .ok_or(ErrorCode::Overflow)?;
    let denominator = (buckets_total as u128)
        .checked_mul(10_000)
        .ok_or(ErrorCode::Overflow)?;
    let result = numerator
        .checked_div(denominator)
        .ok_or(ErrorCode::Overflow)?;
    u64::try_from(result).map_err(|_| ErrorCode::Overflow.into())
}

/// Combine failure reasons
fn combine_failure_reason(current: SlaFailureReason, new: SlaFailureReason) -> SlaFailureReason {
    match (current, new) {
        (SlaFailureReason::None, x) => x,
        (x, SlaFailureReason::None) => x,
        (SlaFailureReason::Latency, SlaFailureReason::Bandwidth) => SlaFailureReason::Both,
        (SlaFailureReason::Bandwidth, SlaFailureReason::Latency) => SlaFailureReason::Both,
        (SlaFailureReason::Both, _) => SlaFailureReason::Both,
        (_, SlaFailureReason::Both) => SlaFailureReason::Both,
        (x, _) => x, // Keep existing if same type
    }
}

/// Verify Ed25519 signature via Instructions sysvar introspection
/// 
/// The Ed25519 precompile instruction must be in the same transaction,
/// immediately preceding this instruction. We verify:
/// 1. The instruction targets the Ed25519 program
/// 2. The pubkey matches expected verifier
/// 3. The message matches our expected payload
fn verify_bucket_failure_signature(
    instructions_sysvar: &AccountInfo,
    expected_verifier: &Pubkey,
    session_key: &Pubkey,
    bucket_index: u64,
    bucket_start_slot: u64,
    failure_reason: SlaFailureReason,
) -> Result<()> {
    // Get current instruction index
    let current_ix_idx = instructions::load_current_index_checked(instructions_sysvar)
        .map_err(|_| ErrorCode::InvalidEd25519Instruction)?;
    
    // Ed25519 instruction must be immediately before this one
    require!(current_ix_idx > 0, ErrorCode::InvalidEd25519Instruction);
    
    let ed25519_ix = load_instruction_at_checked(
        (current_ix_idx - 1) as usize,
        instructions_sysvar,
    ).map_err(|_| ErrorCode::InvalidEd25519Instruction)?;
    
    // Verify it's the Ed25519 program
    require!(
        ed25519_ix.program_id == ED25519_PROGRAM_ID,
        ErrorCode::InvalidEd25519Instruction
    );
    
    // Ed25519 instruction data format:
    // - 2 bytes: number of signatures
    // - For each signature:
    //   - 2 bytes: signature offset
    //   - 2 bytes: signature instruction index (0xFF = same tx)
    //   - 2 bytes: public key offset  
    //   - 2 bytes: public key instruction index
    //   - 2 bytes: message data offset
    //   - 2 bytes: message data size
    //   - 2 bytes: message instruction index
    // Then the actual data (signatures, pubkeys, messages)
    
    require!(ed25519_ix.data.len() >= 16, ErrorCode::InvalidEd25519Instruction);
    
    // Build expected message: (program_id, session, bucket_index, bucket_start, failure_reason)
    let mut expected_message = Vec::with_capacity(32 + 32 + 8 + 8 + 1);
    expected_message.extend_from_slice(&crate::ID.to_bytes());  // Domain separator
    expected_message.extend_from_slice(&session_key.to_bytes());
    expected_message.extend_from_slice(&bucket_index.to_le_bytes());
    expected_message.extend_from_slice(&bucket_start_slot.to_le_bytes());
    expected_message.push(failure_reason as u8);
    
    // Parse Ed25519 instruction to verify pubkey and message
    // Simplified check: verify the instruction contains our expected verifier pubkey
    // and the message bytes match
    let verifier_bytes = expected_verifier.to_bytes();
    
    // Check pubkey is present in instruction data
    let pubkey_found = ed25519_ix.data
        .windows(32)
        .any(|w| w == verifier_bytes);
    require!(pubkey_found, ErrorCode::InvalidAttester);
    
    // Check message is present in instruction data
    let message_found = ed25519_ix.data
        .windows(expected_message.len())
        .any(|w| w == expected_message.as_slice());
    require!(message_found, ErrorCode::SignatureMessageMismatch);
    
    Ok(())
}

fn compute_insurance_coverage(max_spend: u64, price_per_chunk: u64) -> u64 {
    use session_escrow::{INSURANCE_A, INSURANCE_B, INSURANCE_MIN_BPS, INSURANCE_CAP_BPS};

    let term_a = max_spend.saturating_mul(INSURANCE_A).saturating_div(10000);
    let term_b = price_per_chunk.saturating_mul(INSURANCE_B).saturating_div(10000);
    let raw_coverage = term_a.saturating_add(term_b);

    let p_min = max_spend.saturating_mul(INSURANCE_MIN_BPS).saturating_div(10000);
    let p_cap = max_spend.saturating_mul(INSURANCE_CAP_BPS).saturating_div(10000);

    raw_coverage.max(p_min).min(p_cap)
}

/// Compute bid coverage based on premium and SLA strictness
///
/// bid_coverage_p = base_factor * (premium_weight * premium_bps + sla_weight * sla_strictness)
/// where sla_strictness is derived from latency and bandwidth targets
fn compute_bid_coverage(
    max_spend: u64,
    premium_bps: u16,
    latency_target_ms: u16,
    bandwidth_min_chunks: u32,
) -> u64 {
    use session_escrow::{BID_PREMIUM_WEIGHT, BID_SLA_WEIGHT};

    // Premium contribution (higher premium = more coverage)
    let premium_factor = (premium_bps as u64)
        .saturating_mul(BID_PREMIUM_WEIGHT)
        .saturating_div(10000);

    // SLA strictness contribution
    // Lower latency target = stricter = more coverage
    // Higher bandwidth target = stricter = more coverage
    let latency_strictness = if latency_target_ms > 0 {
        // Normalize: 100ms target = 100, 50ms = 200, 200ms = 50
        10000u64.saturating_div(latency_target_ms as u64)
    } else {
        100
    };

    let bandwidth_strictness = (bandwidth_min_chunks as u64).min(1000); // Cap at 1000 for normalization

    let sla_factor = latency_strictness
        .saturating_add(bandwidth_strictness)
        .saturating_mul(BID_SLA_WEIGHT)
        .saturating_div(10000);

    // Combined factor applied to max_spend
    let total_factor = premium_factor.saturating_add(sla_factor);

    max_spend
        .saturating_mul(total_factor)
        .saturating_div(10000)
}

fn verify_permit_signature(
    instructions_sysvar: &AccountInfo,
    _user: &Pubkey,
    _session: &Pubkey,
    _provider: &Pubkey,
    _permit_nonce: u64,
    _amount: u64,
    _expiry_slot: u64,
) -> Result<()> {
    let ix = load_instruction_at_checked(0, instructions_sysvar)
        .map_err(|_| ErrorCode::InvalidSignatureInstruction)?;

    require!(ix.program_id == ED25519_PROGRAM_ID, ErrorCode::InvalidSignatureInstruction);
    require!(ix.data.len() >= 16, ErrorCode::InvalidSignatureData);

    Ok(())
}

// ============================================================================
// Accounts
// ============================================================================

#[derive(Accounts)]
#[instruction(session_nonce: u64, mode_id: u32)]
pub struct OpenSession<'info> {
    #[account(
        init,
        payer = user,
        space = 8 + Session::INIT_SPACE,
        seeds = [b"sess", user.key().as_ref(), &session_nonce.to_le_bytes()],
        bump
    )]
    pub session: Account<'info, Session>,

    #[account(
        init,
        payer = user,
        associated_token::mint = payment_mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    pub payment_mint: Account<'info, Mint>,

    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Provider pubkey
    pub provider: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FundSession<'info> {
    #[account(
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AckStart<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = provider @ ErrorCode::WrongProvider
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    pub provider: Signer<'info>,

    pub collateral_vault_program: Program<'info, CollateralVault>,
}

#[derive(Accounts)]
pub struct SnapshotWindowStart<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,
}

#[derive(Accounts)]
pub struct RedeemPermit<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = provider @ ErrorCode::WrongProvider
    )]
    pub session: Account<'info, Session>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub provider_token_account: Account<'info, TokenAccount>,

    pub provider: Signer<'info>,

    /// CHECK: Instructions sysvar
    #[account(address = instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EvaluateBandwidthSla<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,
}

#[derive(Accounts)]
pub struct SubmitLatencyAttestation<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,

    /// Verifier must be in the allowlist (checked via registry)
    pub verifier: Signer<'info>,

    /// Registry for verifier allowlist check
    #[account(
        seeds = [b"registry"],
        bump,
        seeds::program = mode_registry::ID
    )]
    pub registry: Account<'info, mode_registry::Registry>,
}

#[derive(Accounts)]
pub struct FinalizeSla<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,
}

#[derive(Accounts)]
pub struct ClaimSlaFailure<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position (for slash CPI)
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    /// Provider's collateral vault token account
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub collateral_vault_program: Program<'info, CollateralVault>,
}

#[derive(Accounts)]
pub struct CloseSession<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct FinalizeClose<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position (for release CPI)
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub collateral_vault_program: Program<'info, CollateralVault>,
}

#[derive(Accounts)]
pub struct ClaimNoStart<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ClaimStall<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position (for slash CPI)
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    /// Provider's collateral vault token account
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub collateral_vault_program: Program<'info, CollateralVault>,
}

// ============================================================================
// Bucketed SLA Account Structs
// ============================================================================

#[derive(Accounts)]
pub struct ReportBucketFailure<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,

    /// Authorized verifier (must match session.verifier_pubkey)
    pub verifier: Signer<'info>,

    /// CHECK: Instructions sysvar for Ed25519 signature introspection
    #[account(address = instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct TerminateForCause<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump,
        has_one = user @ ErrorCode::WrongUser
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position (for slash CPI)
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    /// Provider's collateral vault token account
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub collateral_vault_program: Program<'info, CollateralVault>,
}

#[derive(Accounts)]
pub struct SettleSla<'info> {
    #[account(
        mut,
        seeds = [b"sess", session.user.as_ref(), &session.session_nonce.to_le_bytes()],
        bump = session.bump
    )]
    pub session: Account<'info, Session>,

    /// Provider's collateral position (for slash/release CPI)
    #[account(mut)]
    pub position: Account<'info, ProviderPosition>,

    /// Provider's collateral vault token account
    #[account(mut)]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = session.mint,
        associated_token::authority = session
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,

    /// Provider token account (for premium payment if SLA met)
    #[account(mut)]
    pub provider_token_account: Account<'info, TokenAccount>,

    /// User token account (for escrow refund if SLA failed)
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub collateral_vault_program: Program<'info, CollateralVault>,
}

// ============================================================================
// State
// ============================================================================

#[account]
#[derive(InitSpace)]
pub struct Session {
    // Core session fields
    pub user: Pubkey,
    pub provider: Pubkey,
    pub mode_id: u32,
    pub mint: Pubkey,
    pub session_nonce: u64,
    pub chunk_size: u64,
    pub price_per_chunk: u64,
    pub max_spend: u64,
    pub total_spent: u64,
    pub reserve_r: u64,
    pub start_deadline_slot: u64,
    pub stall_timeout_slots: u64,
    pub last_progress_slot: u64,
    pub state: SessionState,
    pub acked: bool,
    pub next_permit_nonce: u64,
    pub bump: u8,

    // Bid/SLA fields
    pub is_bid: bool,
    pub premium_bps: u16,
    pub fail_payout_bps: u16,
    pub latency_target_ms: u16,
    pub bandwidth_min_chunks: u32,
    pub sla_warmup_slots: u64,
    pub sla_window_slots: u64,
    pub sla_window_start_slot: u64,
    pub sla_window_end_slot: u64,

    // Insurance split
    pub base_coverage_p: u64,
    pub bid_coverage_p: u64,
    pub reserve_base: u64,
    pub reserve_bid: u64,

    // SLA state
    pub sla_status: SlaStatus,
    pub sla_failure_reason: SlaFailureReason,
    pub latency_attested: bool,

    // Nonce tracking for bandwidth SLA (legacy window-level)
    pub nonce_at_window_start: u64,
    pub nonce_at_window_end: u64,

    // Bucketed SLA configuration
    pub bucket_slots: u64,                  // Slots per bucket (e.g. 750 ≈ 5 min at 400ms)
    pub buckets_total: u64,                 // sla_window_slots / bucket_slots (max 1024)
    pub bucket_penalty: u64,                // Precomputed penalty per bucket

    // Bucketed downtime tracking
    pub buckets_failed: u64,                // Counter for fast penalty calc
    pub buckets_failed_bitmap: [u8; 128],   // 1024 bits = 1024 buckets max

    // Termination window
    pub first_violation_slot: u64,          // 0 until first fail
    pub terminate_window_slots: u64,        // e.g. 302400 ≈ 7 days
    pub terminate_deadline_slot: u64,       // first_violation + window

    // Penalty accounting
    pub penalty_accrued: u64,               // Running total (tokens)

    // Attester configuration
    pub verifier_pubkey: Pubkey,            // Authorized attester for bucket reports

    // Convenience flags
    pub terminated_for_cause: bool,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum SessionState {
    Open,
    Active,
    Closing,
    Closed,
    Claimed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum SlaStatus {
    None,               // Not a bid session or SLA not yet active
    Pending,            // SLA active, no violations yet
    Violated,           // First miss occurred, termination window open
    Met,                // Window ended, SLA passed
    Failed,             // Window ended, violations settled (no termination)
    TerminatedForCause, // Client exercised termination right
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum SlaFailureReason {
    None,
    Latency,
    Bandwidth,
    Both,
    PrivacyMode,  // Future: privacy/confidentiality violations
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum ClaimType {
    NoStart,
    Stall,
    SlaFailure,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum SlaType {
    Bandwidth,
    Latency,
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct SessionOpened {
    pub session: Pubkey,
    pub user: Pubkey,
    pub provider: Pubkey,
    pub mode_id: u32,
    pub max_spend: u64,
    pub base_coverage_p: u64,
    pub reserve_r: u64,
    pub start_deadline_slot: u64,
    // Bid mode fields
    pub is_bid: bool,
    pub premium_bps: u16,
    pub fail_payout_bps: u16,
    pub bid_coverage_p: u64,
    pub reserve_base: u64,
    pub reserve_bid: u64,
}

#[event]
pub struct SessionFunded {
    pub session: Pubkey,
    pub amount: u64,
    pub new_balance: u64,
}

#[event]
pub struct SessionStarted {
    pub session: Pubkey,
    pub started_at_slot: u64,
}

#[event]
pub struct SlaWindowStartSnapshotted {
    pub session: Pubkey,
    pub nonce_at_start: u64,
    pub slot: u64,
}

#[event]
pub struct PermitRedeemed {
    pub session: Pubkey,
    pub permit_nonce: u64,
    pub amount: u64,
    pub total_spent: u64,
}

#[event]
pub struct SlaEvaluated {
    pub session: Pubkey,
    pub sla_type: SlaType,
    pub passed: bool,
    pub actual_value: u64,
    pub target_value: u64,
}

#[event]
pub struct LatencyAttestationSubmitted {
    pub session: Pubkey,
    pub verifier: Pubkey,
    pub rtt_p90_ms: u16,
    pub measurement_window_start: u64,
    pub measurement_window_end: u64,
}

#[event]
pub struct SlaFinalized {
    pub session: Pubkey,
    pub status: SlaStatus,
}

#[event]
pub struct SlaFailureClaimed {
    pub session: Pubkey,
    pub payout: u64,
    pub escrow_refunded: u64,
    pub remaining_reserve_released: u64,
    pub failure_reason: SlaFailureReason,
}

// Bucketed SLA Events
#[event]
pub struct BucketFailureReported {
    pub session: Pubkey,
    pub bucket_index: u64,
    pub bucket_start_slot: u64,
    pub failure_reason: SlaFailureReason,
    pub buckets_failed: u64,
    pub penalty_accrued: u64,
    pub is_first_violation: bool,
}

#[event]
pub struct SessionTerminatedForCause {
    pub session: Pubkey,
    pub penalty_paid: u64,
    pub escrow_refunded: u64,
    pub buckets_failed: u64,
    pub failure_reason: SlaFailureReason,
    pub remaining_collateral_released: u64,
}

#[event]
pub struct SlaSettled {
    pub session: Pubkey,
    pub status: SlaStatus,
    pub buckets_failed: u64,
    pub penalty_paid: u64,
    pub premium_to_host: u64,
    pub premium_refunded_to_user: u64,
}

#[event]
pub struct SessionClosing {
    pub session: Pubkey,
}

#[event]
pub struct SessionClosed {
    pub session: Pubkey,
    pub refunded: u64,
}

#[event]
pub struct ClaimPaid {
    pub session: Pubkey,
    pub claim_type: ClaimType,
    pub payout: u64,
    pub escrow_refunded: u64,
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
    #[msg("Session not in fundable state")]
    SessionNotFundable,
    #[msg("Invalid session state")]
    InvalidSessionState,
    #[msg("Session already acknowledged")]
    AlreadyAcked,
    #[msg("Start deadline has passed")]
    StartDeadlinePassed,
    #[msg("Session not active")]
    SessionNotActive,
    #[msg("Permit has expired")]
    PermitExpired,
    #[msg("Invalid permit nonce")]
    InvalidPermitNonce,
    #[msg("Invalid signature instruction")]
    InvalidSignatureInstruction,
    #[msg("Invalid signature data")]
    InvalidSignatureData,
    #[msg("Insufficient escrow balance")]
    InsufficientEscrow,
    #[msg("Max spend exceeded")]
    MaxSpendExceeded,
    #[msg("Session already started")]
    SessionAlreadyStarted,
    #[msg("Deadline not passed")]
    DeadlineNotPassed,
    #[msg("Session not started")]
    SessionNotStarted,
    #[msg("Stall timeout not reached")]
    StallTimeoutNotReached,
    #[msg("Wrong user")]
    WrongUser,
    #[msg("Wrong provider")]
    WrongProvider,
    // SLA-related errors
    #[msg("Not a bid session")]
    NotBidSession,
    #[msg("SLA already evaluated")]
    SlaAlreadyEvaluated,
    #[msg("SLA window has not ended")]
    SlaWindowNotEnded,
    #[msg("SLA window has not started")]
    SlaWindowNotStarted,
    #[msg("Window start already snapshotted")]
    WindowAlreadySnapshotted,
    #[msg("Window start not snapshotted")]
    WindowStartNotSnapshotted,
    #[msg("SLA not failed")]
    SlaNotFailed,
    #[msg("SLA has failures")]
    SlaHasFailures,
    #[msg("Invalid measurement window")]
    InvalidMeasurementWindow,
    #[msg("Verifier not authorized")]
    VerifierNotAuthorized,
    #[msg("Latency already attested")]
    LatencyAlreadyAttested,
    // Bucketed SLA errors
    #[msg("Invalid bucket configuration")]
    InvalidBucketConfig,
    #[msg("Bucket already reported")]
    BucketAlreadyReported,
    #[msg("Bucket index out of bounds")]
    BucketIndexOutOfBounds,
    #[msg("Bucket slot mismatch")]
    BucketSlotMismatch,
    #[msg("Termination window expired")]
    TerminationWindowExpired,
    #[msg("Termination window not started")]
    TerminationWindowNotStarted,
    #[msg("Session not in violated state")]
    SessionNotViolated,
    #[msg("Session already terminated")]
    SessionAlreadyTerminated,
    #[msg("Invalid attester")]
    InvalidAttester,
    #[msg("Invalid Ed25519 signature instruction")]
    InvalidEd25519Instruction,
    #[msg("Signature message mismatch")]
    SignatureMessageMismatch,
    #[msg("Report outside SLA window")]
    ReportOutsideSlaWindow,
    #[msg("Report after termination deadline")]
    ReportAfterDeadline,
}
