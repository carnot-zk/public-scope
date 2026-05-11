use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod invoke;
pub mod module;
pub mod state;
pub mod utils;

pub use constants::*;
pub use errors::*;
pub use events::*;
pub use module::*;
#[allow(ambiguous_glob_reexports)]
pub use state::*;

#[cfg(feature = "staging")]
declare_id!("Aw5cs27PQXAeLqyZpTeHdmPXWyzvEWfWc7LkPBznSHCL");

#[cfg(not(feature = "staging"))]
declare_id!("7gEEhnpAqFNKKqYWejqbxfVy58RNwtTXED2c7yEsgjJH");

#[program]
pub mod carnot_engine {
    use super::*;

    pub fn admin_init(
        ctx: Context<AdminInit>,
        keeper: Pubkey,
        protocol_fee_bps: u16,
        min_batch_interval_secs: i64,
        min_regime_update_interval_secs: i64,
    ) -> Result<()> {
        module::admin::admin_init(
            ctx,
            keeper,
            protocol_fee_bps,
            min_batch_interval_secs,
            min_regime_update_interval_secs,
        )
    }

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
        module::market::init_market(
            ctx,
            market_id,
            pyth_feed_id,
            regime_id,
            fortress_spread_bps,
            max_multiplier,
            vol_regime,
        )
    }

    pub fn update_market(
        ctx: Context<UpdateMarket>,
        market_id: [u8; 32],
        regime_id: u64,
        fortress_spread_bps: u64,
        max_multiplier: u64,
        vol_regime: VolRegime,
    ) -> Result<()> {
        module::market::update_market(
            ctx,
            market_id,
            regime_id,
            fortress_spread_bps,
            max_multiplier,
            vol_regime,
        )
    }

    pub fn lp_deposit(ctx: Context<LpDeposit>, amount: u64) -> Result<()> {
        module::lp::lp_deposit(ctx, amount)
    }

    pub fn lp_withdraw(ctx: Context<LpWithdraw>, shares: u64) -> Result<()> {
        module::lp::lp_withdraw(ctx, shares)
    }

    pub fn trader_deposit(ctx: Context<TraderDeposit>, amount: u64) -> Result<()> {
        module::trade::trader_deposit(ctx, amount)
    }

    pub fn trader_withdraw(ctx: Context<TraderWithdraw>, amount: u64) -> Result<()> {
        module::trade::trader_withdraw(ctx, amount)
    }

    pub fn lock_margin(ctx: Context<LockMargin>, amount: u64) -> Result<()> {
        module::trade::lock_margin(ctx, amount)
    }

    pub fn unlock_margin(ctx: Context<UnlockMargin>, amount: u64) -> Result<()> {
        module::trade::unlock_margin(ctx, amount)
    }

    pub fn distribute_winnings(
        ctx: Context<DistributeWinnings>,
        batch_id: [u8; 32],
        trade_id: [u8; 32],
        stake_usdt: u64,
        payout_index: u32,
        winnings_usdt: u64,
        proof_nodes: Vec<[u8; 32]>,
    ) -> Result<()> {
        module::admin::distribute_winnings(
            ctx,
            batch_id,
            trade_id,
            stake_usdt,
            payout_index,
            winnings_usdt,
            proof_nodes,
        )
    }

    pub fn rotate_keeper(ctx: Context<AdminControl>, new_keeper: Pubkey) -> Result<()> {
        let old_keeper = ctx.accounts.global_state.keeper;
        ctx.accounts.global_state.rotate_keeper(new_keeper);
        emit!(KeeperRotated { old_keeper, new_keeper });
        Ok(())
    }

    pub fn pause(ctx: Context<AdminControl>) -> Result<()> {
        ctx.accounts.global_state.pause();
        Ok(())
    }

    pub fn unpause(ctx: Context<AdminControl>) -> Result<()> {
        ctx.accounts.global_state.unpause();
        Ok(())
    }

    pub fn withdraw_protocol_fees(ctx: Context<WithdrawProtocolFees>, amount: u64) -> Result<()> {
        module::admin::withdraw_protocol_fees(ctx, amount)
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
        module::admin::verify_and_settle(
            ctx,
            market_id,
            batch_id,
            proof_a,
            proof_b,
            proof_c,
            proof_nonce,
            public_outputs_hash,
            window_start,
            window_end,
            pyth_checkpoints_hash,
            net_payout_usdt,
            pool_balance_before,
            pool_balance_after,
            num_trades,
            nullifier_hash,
            keeper_fee,
            current_liability_before,
            protocol_fee,
            num_winners,
            num_losers,
            total_winners_payout,
            total_losers_stake,
            market_regime_id,
            payouts_commitment,
            trades_commitment,
        )
    }
}
