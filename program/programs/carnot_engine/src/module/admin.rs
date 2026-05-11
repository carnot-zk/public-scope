use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;

use crate::constants::*;
use crate::errors::CarnotError;
use crate::events::{
    LpYieldAccrued, SettlementBatchCommitted, SolvencyChecked, WinningsDistributed,
};
use crate::invoke::vault_token_transfer;
use crate::state::*;
use crate::utils::{
    compute_merkle_root_from_proof, compute_public_outputs_hash, compute_public_values_hash,
    hash_pyth_checkpoints, require_nonzero, safe_abs_i64_to_u64, safe_elapsed_secs, verify_groth16,
    SafeMath,
};

pub fn admin_init(
    ctx: Context<AdminInit>,
    keeper: Pubkey,
    protocol_fee_bps: u16,
    min_batch_interval_secs: i64,
    min_regime_update_interval_secs: i64,
) -> Result<()> {
    ctx.accounts.global_state.init(
        ctx.accounts.admin.key(),
        keeper,
        protocol_fee_bps,
        min_batch_interval_secs,
        min_regime_update_interval_secs,
        ctx.bumps.global_state,
    );
    ctx.accounts.vault_state.init(ctx.bumps.vault_state);
    Ok(())
}

pub fn withdraw_protocol_fees(ctx: Context<WithdrawProtocolFees>, amount: u64) -> Result<()> {
    require_nonzero(amount)?;
    require!(
        amount <= ctx.accounts.vault_state.accrued_protocol_fees,
        CarnotError::InvalidAmount
    );

    {
        let vault = &mut ctx.accounts.vault_state;
        vault.accrued_protocol_fees = vault.accrued_protocol_fees.safe_sub(amount)?;
        vault.lp_total_deposited = vault.lp_total_deposited.safe_sub(amount)?;
    }

    vault_token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.vault_state,
        &ctx.accounts.lp_vault_token_account.to_account_info(),
        &ctx.accounts.admin_token_account.to_account_info(),
        amount,
    )?;

    Ok(())
}

/// Trades are placed off-chain and committed to the ZK circuit; only the
/// settlement proof and per-trade payout distribution need to touch the chain.
///
/// `trade_id`    — 32-byte SHA-256 of the off-chain order ID (matches the merkle leaf).
/// `stake_usdt`  — original stake, trusted from the keeper (aggregate verified by ZK proof).
/// `winnings_usdt` — gross payout: 0 for a loss, stake * multiplier / 10_000 for a win.
///                   Verified against the batch `payouts_commitment` merkle root.
/// Double-claim is prevented by the `claim_receipt` PDA init in the account context.
pub fn distribute_winnings(
    ctx: Context<DistributeWinnings>,
    batch_id: [u8; 32],
    trade_id: [u8; 32],
    stake_usdt: u64,
    payout_index: u32,
    winnings_usdt: u64,
    proof_nodes: Vec<[u8; 32]>,
) -> Result<()> {
    ctx.accounts.global_state.require_not_paused()?;
    require_nonzero(stake_usdt)?;

    let receipt = &ctx.accounts.batch_receipt_account;
    require!(receipt.batch_id == batch_id, CarnotError::InvalidBatch);
    require!(receipt.settled_at > 0, CarnotError::BatchNotSettled);

    // Verify merkle proof: leaf = sha256(batch_id || trade_id || trader || winnings_le || index_le)
    // This must exactly match hashPayoutLeaf in settlement-calculator.service.ts.
    let payout_leaf = hashv(&[
        &batch_id,
        &trade_id,
        ctx.accounts.trader.key().as_ref(),
        &winnings_usdt.to_le_bytes(),
        &payout_index.to_le_bytes(),
    ])
    .to_bytes();
    let computed_root = compute_merkle_root_from_proof(payout_leaf, payout_index, &proof_nodes);
    require!(
        computed_root == receipt.payouts_commitment,
        CarnotError::InvalidMerkleProof
    );

    ctx.accounts.claim_receipt.bump = ctx.bumps.claim_receipt;

    let trader_account = &mut ctx.accounts.trader_account;
    require!(
        trader_account.balance_usdt >= stake_usdt,
        CarnotError::InsufficientAvailableMargin
    );
    trader_account.balance_usdt = trader_account
        .balance_usdt
        .safe_sub(stake_usdt)?
        .safe_add(winnings_usdt)?;
    // Saturating: settlement may arrive after an explicit unlock_margin call.
    trader_account.locked_usdt = trader_account.locked_usdt.saturating_sub(stake_usdt);

    emit!(WinningsDistributed {
        batch_id,
        trader: ctx.accounts.trader.key(),
        trade_id,
        winnings_usdt,
        stake_usdt,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn verify_and_settle(
    ctx: Context<VerifyAndSettle>,
    market_id: [u8; 32],
    batch_id: [u8; 32],
    proof_a: [u8; 64],
    proof_b: [u8; 128],
    proof_c: [u8; 64],
    proof_nonce: [u8; 32],
    public_outputs_hash: [u8; 32],
    window_start: i64,
    window_end: i64,
    pyth_checkpoints_hash: [u8; 32],
    net_payout_usdt: i64,
    pool_balance_before: u64,
    pool_balance_after: u64,
    num_trades: u32,
    nullifier_hash: [u8; 32],
    keeper_fee: u64,
    current_liability_before: u64,
    protocol_fee: u64,
    num_winners: u32,
    num_losers: u32,
    total_winners_payout: u64,
    total_losers_stake: u64,
    market_regime_id: u64,
    payouts_commitment: [u8; 32],
    trades_commitment: [u8; 32],
) -> Result<()> {
    let global = &mut ctx.accounts.global_state;
    global.require_not_paused()?;

    let now = Clock::get()?.unix_timestamp;
    require!(
        safe_elapsed_secs(now, global.last_batch_at)? >= global.min_batch_interval_secs,
        CarnotError::BatchTooSoon
    );

    require!(
        current_liability_before == ctx.accounts.vault_state.current_liability,
        CarnotError::InvalidPoolBalance
    );
    require!(
        market_regime_id == ctx.accounts.market_state.regime_id,
        CarnotError::InvalidMarketRegime
    );
    require!(
        ctx.accounts.market_state.market_id == market_id,
        CarnotError::InvalidMarketRegime
    );
    let market_max_multiplier: u32 = ctx
        .accounts
        .market_state
        .max_multiplier
        .try_into()
        .map_err(|_| error!(CarnotError::InvalidAmount))?;

    let sp1_vkey_hash: [u8; 32] = DEFAULT_VERIFYING_KEY_BYTES[0..32]
        .try_into()
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))?;
    let vk_root: [u8; 32] = DEFAULT_VERIFYING_KEY_BYTES[32..64]
        .try_into()
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))?;

    let expected_public_outputs_hash = compute_public_outputs_hash(
        &batch_id,
        window_start,
        window_end,
        num_trades,
        market_regime_id,
        market_max_multiplier,
        &pyth_checkpoints_hash,
        pool_balance_before,
        pool_balance_after,
        current_liability_before,
        net_payout_usdt,
        keeper_fee,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        &payouts_commitment,
        &trades_commitment,
        &nullifier_hash,
    );
    require!(
        expected_public_outputs_hash == public_outputs_hash,
        CarnotError::InvalidGroth16Input
    );

    let public_values_hash = compute_public_values_hash(&public_outputs_hash);

    require!(
        pool_balance_before == ctx.accounts.vault_state.lp_total_deposited,
        CarnotError::InvalidPoolBalance
    );

    let expected_feed_id = ctx.accounts.market_state.pyth_feed_id;
    let observed_checkpoints_hash =
        hash_pyth_checkpoints(ctx.remaining_accounts, &expected_feed_id)?;
    require!(
        observed_checkpoints_hash == pyth_checkpoints_hash,
        CarnotError::InvalidPriceHash
    );

    verify_groth16(
        &DEFAULT_VERIFYING_KEY_BYTES,
        &proof_a,
        &proof_b,
        &proof_c,
        &sp1_vkey_hash,
        &public_values_hash,
        &vk_root,
        &proof_nonce,
    )?;

    let mut lp_to_trader_transfer = 0_u64;
    let mut trader_to_lp_transfer = 0_u64;
    let mut protocol_fee_accrued = 0_u64;
    if net_payout_usdt >= 0 {
        let payout = safe_abs_i64_to_u64(net_payout_usdt)?;
        {
            let vault = &mut ctx.accounts.vault_state;
            vault.lp_total_deposited = vault.lp_total_deposited.safe_sub(payout)?;
            vault.trader_total_margin = vault.trader_total_margin.safe_add(payout)?;
        }
        lp_to_trader_transfer = payout;
    } else {
        let gain = safe_abs_i64_to_u64(net_payout_usdt)?;
        {
            let vault = &mut ctx.accounts.vault_state;
            vault.trader_total_margin = vault.trader_total_margin.safe_sub(gain)?;
            vault.lp_total_deposited = vault.lp_total_deposited.safe_add(gain)?;
            vault.accrued_protocol_fees = vault.accrued_protocol_fees.safe_add(protocol_fee)?;
            protocol_fee_accrued = protocol_fee;
        }
        trader_to_lp_transfer = gain;
    }

    {
        let vault = &mut ctx.accounts.vault_state;
        vault.current_liability = vault.current_liability.safe_sub(current_liability_before)?;
    }

    if lp_to_trader_transfer > 0 {
        vault_token_transfer(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_state,
            &ctx.accounts.lp_vault_token_account.to_account_info(),
            &ctx.accounts.trader_vault_token_account.to_account_info(),
            lp_to_trader_transfer,
        )?;
    }
    if trader_to_lp_transfer > 0 {
        vault_token_transfer(
            &ctx.accounts.token_program,
            &ctx.accounts.vault_state,
            &ctx.accounts.trader_vault_token_account.to_account_info(),
            &ctx.accounts.lp_vault_token_account.to_account_info(),
            trader_to_lp_transfer,
        )?;
    }
    if protocol_fee_accrued > 0 {
        emit!(LpYieldAccrued {
            amount: protocol_fee_accrued,
            total_accrued_protocol_fees: ctx.accounts.vault_state.accrued_protocol_fees,
        });
    }

    {
        let vault = &mut ctx.accounts.vault_state;
        require!(
            vault.lp_total_deposited >= keeper_fee,
            CarnotError::InvalidAmount
        );
        vault.lp_total_deposited = vault.lp_total_deposited.safe_sub(keeper_fee)?;
    }
    vault_token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.vault_state,
        &ctx.accounts.lp_vault_token_account.to_account_info(),
        &ctx.accounts.keeper_token_account.to_account_info(),
        keeper_fee,
    )?;

    require!(
        pool_balance_after == ctx.accounts.vault_state.lp_total_deposited,
        CarnotError::InvalidPoolBalance
    );

    ctx.accounts.vault_state.check_solvency()?;

    let nullifier = &mut ctx.accounts.nullifier_account;
    nullifier.nullifier_hash = nullifier_hash;
    nullifier.bump = ctx.bumps.nullifier_account;

    let receipt = &mut ctx.accounts.batch_receipt_account;
    receipt.batch_id = batch_id;
    receipt.settled_at = now;
    receipt.window_start = window_start;
    receipt.window_end = window_end;
    receipt.num_trades = num_trades;
    receipt.num_winners = num_winners;
    receipt.num_losers = num_losers;
    receipt.net_payout_usdt = net_payout_usdt;
    receipt.pool_balance_before = pool_balance_before;
    receipt.pool_balance_after = pool_balance_after;
    receipt.current_liability_before = current_liability_before;
    receipt.total_winners_payout = total_winners_payout;
    receipt.total_losers_stake = total_losers_stake;
    receipt.keeper_fee = keeper_fee;
    receipt.protocol_fee = protocol_fee;
    receipt.pyth_checkpoints_hash = pyth_checkpoints_hash;
    receipt.payouts_commitment = payouts_commitment;
    receipt.trades_commitment = trades_commitment;
    receipt.nullifier_hash = nullifier_hash;
    receipt.market_regime_id = market_regime_id;
    receipt.keeper_pubkey = ctx.accounts.keeper.key();
    receipt.bump = ctx.bumps.batch_receipt_account;

    global.last_batch_at = now;

    emit!(SettlementBatchCommitted {
        batch_id,
        net_payout_usdt,
        keeper_pubkey: ctx.accounts.keeper.key(),
    });
    emit!(SolvencyChecked {
        total_deposited: ctx.accounts.vault_state.lp_total_deposited,
        current_liability: ctx.accounts.vault_state.current_liability,
        solvency_ratio_bps: ctx.accounts.vault_state.solvency_ratio_bps,
    });
    Ok(())
}
