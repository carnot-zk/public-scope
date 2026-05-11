use anchor_lang::prelude::*;

#[event]
pub struct SettlementBatchCommitted {
    pub batch_id: [u8; 32],
    pub net_payout_usdt: i64,
    pub keeper_pubkey: Pubkey,
}

#[event]
pub struct TraderDeposited {
    pub trader: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TraderWithdrawn {
    pub trader: Pubkey,
    pub amount: u64,
}

#[event]
pub struct WinningsDistributed {
    pub batch_id: [u8; 32],
    pub trader: Pubkey,
    pub trade_id: [u8; 32],
    pub winnings_usdt: u64,
    pub stake_usdt: u64,
}

#[event]
pub struct LpDeposited {
    pub lp: Pubkey,
    pub amount: u64,
    pub shares: u64,
}

#[event]
pub struct LpWithdrawn {
    pub lp: Pubkey,
    pub amount: u64,
    pub shares: u64,
}

#[event]
pub struct MarketUpdated {
    pub regime_id: u64,
    pub fortress_spread_bps: u64,
    pub max_multiplier: u64,
}

#[event]
pub struct SolvencyChecked {
    pub total_deposited: u64,
    pub current_liability: u64,
    pub solvency_ratio_bps: u32,
}

#[event]
pub struct KeeperRotated {
    pub old_keeper: Pubkey,
    pub new_keeper: Pubkey,
}

#[event]
pub struct MarginLocked {
    pub trader: Pubkey,
    pub amount: u64,
    pub total_locked: u64,
}

#[event]
pub struct MarginUnlocked {
    pub trader: Pubkey,
    pub amount: u64,
    pub total_locked: u64,
}

#[event]
pub struct LpYieldAccrued {
    pub amount: u64,
    pub total_accrued_protocol_fees: u64,
}
