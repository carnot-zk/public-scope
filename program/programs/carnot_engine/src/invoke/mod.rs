use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, Transfer};

use crate::constants::VAULT_SEED;
use crate::state::VaultState;

pub fn token_transfer<'info>(
    token_program: &Program<'info, Token>,
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    token::transfer(
        CpiContext::new(
            token_program.to_account_info(),
            Transfer {
                from: from.clone(),
                to: to.clone(),
                authority: authority.clone(),
            },
        ),
        amount,
    )?;
    Ok(())
}

pub fn vault_token_transfer<'info>(
    token_program: &Program<'info, Token>,
    vault_state: &Account<'info, VaultState>,
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let bump = [vault_state.bump];
    let signer_seeds: &[&[u8]] = &[VAULT_SEED, &bump];
    token::transfer(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            Transfer {
                from: from.clone(),
                to: to.clone(),
                authority: vault_state.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;
    Ok(())
}
