use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]

pub struct NullifierAccount {
    pub nullifier_hash: [u8; 32],
    pub bump: u8,
}

/// All amounts are in micro-USDT (6 decimals).
/// `balance_usdt` = deposited - settled_losses + settled_winnings - withdrawn.
/// `locked_usdt`  = sum of open trade stakes; keeper increments on trade acceptance,
///                  distribute_winnings decrements on settlement.
/// `_reserved2` pads to 65 bytes to match accounts created by the prior layout.
#[account]
#[derive(InitSpace)]
pub struct TraderAccount {
    pub owner: Pubkey,
    pub balance_usdt: u64,
    pub locked_usdt: u64,
    pub _reserved2: u64,
    pub bump: u8,
}

/// Nullifier PDA created once per (batch_id, trade_id) to prevent double-claiming.
/// Seed: [CLAIM_RECEIPT_SEED, batch_id, trade_id].
#[account]
#[derive(InitSpace)]
pub struct ClaimReceipt {
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]

pub struct LpPosition {
    pub owner: Pubkey,
    pub lp_shares: u64,
    pub last_deposit_at: i64,
    pub bump: u8,
}

/// On-chain audit record for each settled batch. Seeded by [BATCH_SEED, batch_id].
#[account]
#[derive(InitSpace)]

pub struct BatchReceiptAccount {
    pub batch_id: [u8; 32],
    pub settled_at: i64,
    pub window_start: i64,
    pub window_end: i64,
    pub num_trades: u32,
    pub num_winners: u32,
    pub num_losers: u32,
    pub net_payout_usdt: i64,
    pub pool_balance_before: u64,
    pub pool_balance_after: u64,
    pub current_liability_before: u64,
    pub total_winners_payout: u64,
    pub total_losers_stake: u64,
    pub keeper_fee: u64,
    pub protocol_fee: u64,
    pub pyth_checkpoints_hash: [u8; 32],
    pub payouts_commitment: [u8; 32],
    pub trades_commitment: [u8; 32],
    pub nullifier_hash: [u8; 32],
    pub market_regime_id: u64,
    pub keeper_pubkey: Pubkey,
    pub bump: u8,
}
