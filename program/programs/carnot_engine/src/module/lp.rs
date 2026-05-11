use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::CarnotError;
use crate::events::{LpDeposited, LpWithdrawn, SolvencyChecked};
use crate::invoke::{token_transfer, vault_token_transfer};
use crate::state::*;
use crate::utils::{require_nonzero, safe_elapsed_secs, safe_mul_div_u64, SafeMath};

pub fn lp_deposit(ctx: Context<LpDeposit>, amount: u64) -> Result<()> {
    require_nonzero(amount)?;
    ctx.accounts.global_state.require_not_paused()?;

    token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.lp_token_account.to_account_info(),
        &ctx.accounts.lp_vault_token_account.to_account_info(),
        &ctx.accounts.lp.to_account_info(),
        amount,
    )?;

    let vault = &mut ctx.accounts.vault_state;
    let shares = if vault.total_lp_shares == 0 || vault.lp_total_deposited == 0 {
        amount
    } else {
        safe_mul_div_u64(amount, vault.total_lp_shares, vault.lp_total_deposited)?
    };

    vault.lp_total_deposited = vault.lp_total_deposited.safe_add(amount)?;
    vault.total_lp_shares = vault.total_lp_shares.safe_add(shares)?;

    let lp_position = &mut ctx.accounts.lp_position;
    lp_position.owner = ctx.accounts.lp.key();
    lp_position.lp_shares = lp_position.lp_shares.safe_add(shares)?;
    lp_position.last_deposit_at = Clock::get()?.unix_timestamp;
    lp_position.bump = ctx.bumps.lp_position;

    vault.check_solvency()?;
    emit!(LpDeposited {
        lp: ctx.accounts.lp.key(),
        amount,
        shares,
    });
    emit!(SolvencyChecked {
        total_deposited: vault.lp_total_deposited,
        current_liability: vault.current_liability,
        solvency_ratio_bps: vault.solvency_ratio_bps,
    });
    Ok(())
}

pub fn lp_withdraw(ctx: Context<LpWithdraw>, shares: u64) -> Result<()> {
    require_nonzero(shares)?;
    ctx.accounts.global_state.require_not_paused()?;

    let (amount, total_deposited, current_liability, solvency_ratio_bps) = {
        let lp_position = &mut ctx.accounts.lp_position;
        require!(
            lp_position.owner == ctx.accounts.lp.key(),
            CarnotError::Unauthorized
        );
        require!(
            lp_position.lp_shares >= shares,
            CarnotError::InsufficientShares
        );
        let now = Clock::get()?.unix_timestamp;
        require!(
            safe_elapsed_secs(now, lp_position.last_deposit_at)? >= WITHDRAW_TIMELOCK_SECS,
            CarnotError::TimelockNotExpired
        );

        let vault = &mut ctx.accounts.vault_state;
        let amount = safe_mul_div_u64(shares, vault.lp_total_deposited, vault.total_lp_shares)?;

        vault.lp_total_deposited = vault.lp_total_deposited.safe_sub(amount)?;
        vault.total_lp_shares = vault.total_lp_shares.safe_sub(shares)?;
        lp_position.lp_shares = lp_position.lp_shares.safe_sub(shares)?;

        vault.check_solvency()?;
        (
            amount,
            vault.lp_total_deposited,
            vault.current_liability,
            vault.solvency_ratio_bps,
        )
    };

    vault_token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.vault_state,
        &ctx.accounts.lp_vault_token_account.to_account_info(),
        &ctx.accounts.lp_token_account.to_account_info(),
        amount,
    )?;

    emit!(LpWithdrawn {
        lp: ctx.accounts.lp.key(),
        amount,
        shares,
    });
    emit!(SolvencyChecked {
        total_deposited,
        current_liability,
        solvency_ratio_bps,
    });
    Ok(())
}
