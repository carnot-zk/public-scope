use crate::errors::CarnotError;
use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]

pub struct GlobalState {
    pub admin: Pubkey,
    pub keeper: Pubkey,
    pub protocol_fee_bps: u16,
    pub paused: bool,
    pub last_batch_at: i64,
    pub min_batch_interval_secs: i64,
    pub min_regime_update_interval_secs: i64,
    pub bump: u8,
}

impl GlobalState {
    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn unpause(&mut self) {
        self.paused = false;
    }

    pub fn rotate_keeper(&mut self, new_keeper: Pubkey) {
        self.keeper = new_keeper;
    }

    /// Shared guard used by every instruction that must be blocked while paused.
    pub fn require_not_paused(&self) -> Result<()> {
        require!(!self.paused, CarnotError::Paused);
        Ok(())
    }

    pub fn init(
        &mut self,
        admin: Pubkey,
        keeper: Pubkey,
        protocol_fee_bps: u16,
        min_batch_interval_secs: i64,
        min_regime_update_interval_secs: i64,
        bump: u8,
    ) {
        self.admin = admin;
        self.keeper = keeper;
        self.protocol_fee_bps = protocol_fee_bps;
        self.paused = false;
        self.last_batch_at = 0;
        self.min_batch_interval_secs = min_batch_interval_secs;
        self.min_regime_update_interval_secs = min_regime_update_interval_secs;
        self.bump = bump;
    }
}
