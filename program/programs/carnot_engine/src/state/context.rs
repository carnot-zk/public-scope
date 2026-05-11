use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::*;
use crate::state::*;

#[derive(Accounts)]
pub struct AdminInit<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + GlobalState::INIT_SPACE,
        seeds = [GLOBAL_SEED],
        bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        init,
        payer = admin,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [VAULT_SEED],
        bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        init,
        payer = admin,
        associated_token::mint = usdt_mint,
        associated_token::authority = vault_state
    )]
    pub lp_vault_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = admin,
        token::mint = usdt_mint,
        token::authority = vault_state,
        seeds = [TRADER_VAULT_TOKEN_SEED],
        bump
    )]
    pub trader_vault_token_account: Account<'info, TokenAccount>,
    pub usdt_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminControl<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = admin
    )]
    pub global_state: Account<'info, GlobalState>,
}

#[derive(Accounts)]
pub struct WithdrawProtocolFees<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = admin
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(mut)]
    pub lp_vault_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub admin_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(market_id: [u8; 32], batch_id: [u8; 32], _proof_a: [u8; 64], _proof_b: [u8; 128], _proof_c: [u8; 64], _proof_nonce: [u8; 32], _public_outputs_hash: [u8; 32], _window_start: i64, _window_end: i64, _pyth_checkpoints_hash: [u8; 32], _net_payout_usdt: i64, _pool_balance_before: u64, _pool_balance_after: u64, _num_trades: u32, nullifier_hash: [u8; 32])]
pub struct VerifyAndSettle<'info> {
    #[account(mut)]
    pub keeper: Signer<'info>,
    #[account(
        mut,
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = keeper
    )]
    pub global_state: Box<Account<'info, GlobalState>>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Box<Account<'info, VaultState>>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market_id.as_ref()],
        bump = market_state.bump
    )]
    pub market_state: Box<Account<'info, MarketState>>,
    #[account(
        init,
        payer = keeper,
        space = 8 + NullifierAccount::INIT_SPACE,
        seeds = [NULLIFIER_SEED, nullifier_hash.as_ref()],
        bump
    )]
    pub nullifier_account: Box<Account<'info, NullifierAccount>>,
    #[account(
        init,
        payer = keeper,
        space = 8 + BatchReceiptAccount::INIT_SPACE,
        seeds = [BATCH_SEED, batch_id.as_ref()],
        bump
    )]
    pub batch_receipt_account: Box<Account<'info, BatchReceiptAccount>>,
    #[account(mut)]
    pub lp_vault_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [TRADER_VAULT_TOKEN_SEED],
        bump,
        token::authority = vault_state
    )]
    pub trader_vault_token_account: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub keeper_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(market_id: [u8; 32])]
pub struct InitMarket<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = admin
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        init,
        payer = admin,
        space = 8 + MarketState::INIT_SPACE,
        seeds = [MARKET_SEED, market_id.as_ref()],
        bump
    )]
    pub market_state: Account<'info, MarketState>,
    pub system_program: Program<'info, System>,
}

/// `trade_id` is the 32-byte SHA-256 of the off-chain order ID.
/// `claim_receipt` is created here (init) — its existence is the double-claim guard.
#[derive(Accounts)]
#[instruction(batch_id: [u8; 32], trade_id: [u8; 32])]
pub struct DistributeWinnings<'info> {
    /// Pays rent for claim_receipt. Anyone can call on behalf of a trader.
    #[account(mut)]
    pub claimer: Signer<'info>,
    /// CHECK: validated implicitly via PDA seeds of claim_receipt and trader_account.
    pub trader: UncheckedAccount<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        seeds = [BATCH_SEED, batch_id.as_ref()],
        bump = batch_receipt_account.bump
    )]
    pub batch_receipt_account: Account<'info, BatchReceiptAccount>,
    #[account(
        init,
        payer = claimer,
        space = 8 + ClaimReceipt::INIT_SPACE,
        seeds = [CLAIM_RECEIPT_SEED, batch_id.as_ref(), trade_id.as_ref()],
        bump
    )]
    pub claim_receipt: Account<'info, ClaimReceipt>,
    #[account(
        mut,
        seeds = [TRADER_ACCOUNT_SEED, trader.key().as_ref()],
        bump = trader_account.bump
    )]
    pub trader_account: Account<'info, TraderAccount>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(market_id: [u8; 32])]
pub struct UpdateMarket<'info> {
    pub keeper: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = keeper
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market_id.as_ref()],
        bump = market_state.bump
    )]
    pub market_state: Account<'info, MarketState>,
}

#[derive(Accounts)]
pub struct LpDeposit<'info> {
    #[account(mut)]
    pub lp: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        init_if_needed,
        payer = lp,
        space = 8 + LpPosition::INIT_SPACE,
        seeds = [LP_POSITION_SEED, lp.key().as_ref()],
        bump
    )]
    pub lp_position: Account<'info, LpPosition>,
    #[account(mut)]
    pub lp_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LpWithdraw<'info> {
    #[account(mut)]
    pub lp: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        mut,
        seeds = [LP_POSITION_SEED, lp.key().as_ref()],
        bump = lp_position.bump
    )]
    pub lp_position: Account<'info, LpPosition>,
    #[account(mut)]
    pub lp_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub lp_vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct TraderDeposit<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        init_if_needed,
        payer = trader,
        space = 8 + TraderAccount::INIT_SPACE,
        seeds = [TRADER_ACCOUNT_SEED, trader.key().as_ref()],
        bump
    )]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [TRADER_VAULT_TOKEN_SEED],
        bump,
        token::authority = vault_state
    )]
    pub trader_vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LockMargin<'info> {
    pub keeper: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = keeper
    )]
    pub global_state: Account<'info, GlobalState>,
    /// CHECK: validated via PDA seeds derived from this key
    pub trader: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [TRADER_ACCOUNT_SEED, trader.key().as_ref()],
        bump = trader_account.bump
    )]
    pub trader_account: Account<'info, TraderAccount>,
}

#[derive(Accounts)]
pub struct UnlockMargin<'info> {
    pub keeper: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump,
        has_one = keeper
    )]
    pub global_state: Account<'info, GlobalState>,
    /// CHECK: validated via PDA seeds derived from this key
    pub trader: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [TRADER_ACCOUNT_SEED, trader.key().as_ref()],
        bump = trader_account.bump
    )]
    pub trader_account: Account<'info, TraderAccount>,
}

#[derive(Accounts)]
pub struct TraderWithdraw<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(
        seeds = [GLOBAL_SEED],
        bump = global_state.bump
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        mut,
        seeds = [VAULT_SEED],
        bump = vault_state.bump
    )]
    pub vault_state: Account<'info, VaultState>,
    #[account(
        mut,
        seeds = [TRADER_ACCOUNT_SEED, trader.key().as_ref()],
        bump = trader_account.bump
    )]
    pub trader_account: Account<'info, TraderAccount>,
    #[account(mut)]
    pub trader_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [TRADER_VAULT_TOKEN_SEED],
        bump,
        token::authority = vault_state
    )]
    pub trader_vault_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

