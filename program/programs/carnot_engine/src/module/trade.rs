use anchor_lang::prelude::*;

use crate::errors::CarnotError;
use crate::events::{MarginLocked, MarginUnlocked, TraderDeposited, TraderWithdrawn};
use crate::invoke::{token_transfer, vault_token_transfer};
use crate::state::*;
use crate::utils::{require_nonzero, SafeMath};

pub fn trader_deposit(ctx: Context<TraderDeposit>, amount: u64) -> Result<()> {
    require_nonzero(amount)?;
    ctx.accounts.global_state.require_not_paused()?;

    token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.trader_token_account.to_account_info(),
        &ctx.accounts.trader_vault_token_account.to_account_info(),
        &ctx.accounts.trader.to_account_info(),
        amount,
    )?;

    let trader_account = &mut ctx.accounts.trader_account;
    trader_account.owner = ctx.accounts.trader.key();
    trader_account.bump = ctx.bumps.trader_account;
    trader_account.balance_usdt = trader_account.balance_usdt.safe_add(amount)?;
    ctx.accounts.vault_state.trader_total_margin =
        ctx.accounts.vault_state.trader_total_margin.safe_add(amount)?;

    emit!(TraderDeposited {
        trader: ctx.accounts.trader.key(),
        amount,
    });
    Ok(())
}

pub fn trader_withdraw(ctx: Context<TraderWithdraw>, amount: u64) -> Result<()> {
    ctx.accounts.global_state.require_not_paused()?;
    let trader_account = &mut ctx.accounts.trader_account;
    require!(
        trader_account.owner == ctx.accounts.trader.key(),
        CarnotError::Unauthorized
    );
    require_nonzero(amount)?;
    let withdrawable = trader_account
        .balance_usdt
        .saturating_sub(trader_account.locked_usdt);
    require!(
        amount <= withdrawable,
        CarnotError::ExceedsWithdrawableBalance
    );

    trader_account.balance_usdt = trader_account.balance_usdt.safe_sub(amount)?;
    ctx.accounts.vault_state.trader_total_margin =
        ctx.accounts.vault_state.trader_total_margin.safe_sub(amount)?;

    vault_token_transfer(
        &ctx.accounts.token_program,
        &ctx.accounts.vault_state,
        &ctx.accounts.trader_vault_token_account.to_account_info(),
        &ctx.accounts.trader_token_account.to_account_info(),
        amount,
    )?;

    emit!(TraderWithdrawn {
        trader: ctx.accounts.trader.key(),
        amount,
    });
    Ok(())
}

pub fn lock_margin(ctx: Context<LockMargin>, amount: u64) -> Result<()> {
    require_nonzero(amount)?;
    ctx.accounts.global_state.require_not_paused()?;
    let trader_account = &mut ctx.accounts.trader_account;
    let new_locked = trader_account.locked_usdt.safe_add(amount)?;
    require!(
        new_locked <= trader_account.balance_usdt,
        CarnotError::InsufficientAvailableMargin
    );
    trader_account.locked_usdt = new_locked;
    emit!(MarginLocked {
        trader: ctx.accounts.trader.key(),
        amount,
        total_locked: trader_account.locked_usdt,
    });
    Ok(())
}

pub fn unlock_margin(ctx: Context<UnlockMargin>, amount: u64) -> Result<()> {
    require_nonzero(amount)?;
    ctx.accounts.global_state.require_not_paused()?;
    let trader_account = &mut ctx.accounts.trader_account;
    require!(
        amount <= trader_account.locked_usdt,
        CarnotError::InvalidAmount
    );
    trader_account.locked_usdt = trader_account.locked_usdt.safe_sub(amount)?;
    emit!(MarginUnlocked {
        trader: ctx.accounts.trader.key(),
        amount,
        total_locked: trader_account.locked_usdt,
    });
    Ok(())
}

