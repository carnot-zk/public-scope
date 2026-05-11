use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum VolRegime {
    Low,
    Medium,
    High,
    Extreme,
}

#[account]
#[derive(InitSpace)]
pub struct MarketState {
    pub market_id: [u8; 32],
    pub pyth_feed_id: [u8; 32],
    pub regime_id: u64,
    pub fortress_spread_bps: u64,
    pub max_multiplier: u64,
    pub vol_regime: VolRegime,
    pub last_updated: i64,
    pub bump: u8,
}

impl MarketState {
    /// Write the mutable subset of market parameters shared by both `init` and `update`.
    pub fn apply_params(
        &mut self,
        market_id: [u8; 32],
        regime_id: u64,
        fortress_spread_bps: u64,
        max_multiplier: u64,
        vol_regime: VolRegime,
        now: i64,
    ) {
        self.market_id = market_id;
        self.regime_id = regime_id;
        self.fortress_spread_bps = fortress_spread_bps;
        self.max_multiplier = max_multiplier;
        self.vol_regime = vol_regime;
        self.last_updated = now;
    }
}
