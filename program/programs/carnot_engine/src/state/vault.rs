use crate::constants::MIN_SOLVENCY_RATIO_BPS;
use crate::errors::CarnotError;
use crate::utils::{safe_ratio_bps_u128, safe_to_u32};
use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]

pub struct VaultState {
    pub total_lp_shares: u64,
    pub lp_total_deposited: u64,
    pub trader_total_margin: u64,
    pub current_liability: u64,
    pub solvency_ratio_bps: u32,
    pub accrued_protocol_fees: u64,
    pub bump: u8,
}

impl VaultState {
    pub fn init(&mut self, bump: u8) {
        self.total_lp_shares = 0;
        self.lp_total_deposited = 0;
        self.trader_total_margin = 0;
        self.current_liability = 0;
        self.solvency_ratio_bps = u32::MAX;
        self.accrued_protocol_fees = 0;
        self.bump = bump;
    }

    pub fn check_solvency(&mut self) -> Result<()> {
        if self.current_liability == 0 {
            self.solvency_ratio_bps = u32::MAX;
            return Ok(());
        }
        let ratio = safe_ratio_bps_u128(self.lp_total_deposited, self.current_liability)?;
        require!(
            ratio >= MIN_SOLVENCY_RATIO_BPS,
            CarnotError::InsufficientSolvency
        );
        self.solvency_ratio_bps = safe_to_u32(ratio)?;
        Ok(())
    }
}
