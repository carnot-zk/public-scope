use anchor_lang::prelude::*;

use crate::errors::CarnotError;
use crate::events::MarketUpdated;
use crate::state::*;
use crate::utils::safe_elapsed_secs;

#[allow(clippy::too_many_arguments)]
pub fn init_market(
    ctx: Context<InitMarket>,
    market_id: [u8; 32],
    pyth_feed_id: [u8; 32],
    regime_id: u64,
    fortress_spread_bps: u64,
    max_multiplier: u64,
    vol_regime: VolRegime,
) -> Result<()> {
    require!(!ctx.accounts.global_state.paused, CarnotError::Paused);
    require!(fortress_spread_bps > 0, CarnotError::InvalidAmount);
    require!(max_multiplier > 0, CarnotError::InvalidAmount);

    let now = Clock::get()?.unix_timestamp;
    let market = &mut ctx.accounts.market_state;
    market.apply_params(market_id, regime_id, fortress_spread_bps, max_multiplier, vol_regime, now);
    market.pyth_feed_id = pyth_feed_id;
    market.bump = ctx.bumps.market_state;

    Ok(())
}

pub fn update_market(
    ctx: Context<UpdateMarket>,
    market_id: [u8; 32],
    regime_id: u64,
    fortress_spread_bps: u64,
    max_multiplier: u64,
    vol_regime: VolRegime,
) -> Result<()> {
    require!(!ctx.accounts.global_state.paused, CarnotError::Paused);
    let market = &mut ctx.accounts.market_state;
    require!(
        regime_id > market.regime_id,
        CarnotError::InvalidMarketRegime
    );
    require!(fortress_spread_bps > 0, CarnotError::InvalidAmount);
    require!(max_multiplier > 0, CarnotError::InvalidAmount);
    let now = Clock::get()?.unix_timestamp;
    require!(
        safe_elapsed_secs(now, market.last_updated)?
            >= ctx.accounts.global_state.min_regime_update_interval_secs,
        CarnotError::BatchTooSoon
    );

    market.apply_params(market_id, regime_id, fortress_spread_bps, max_multiplier, vol_regime, now);

    emit!(MarketUpdated {
        regime_id,
        fortress_spread_bps,
        max_multiplier,
    });
    Ok(())
}
