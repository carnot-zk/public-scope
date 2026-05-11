use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;
use solana_bn254::prelude::{
    alt_bn128_g1_addition_be, alt_bn128_g1_multiplication_be, alt_bn128_pairing_be,
};
use solana_sha256_hasher::hashv as sha256_hashv;

use crate::constants::N_PYTH_CHECKPOINTS;
use crate::errors::CarnotError;

pub mod helper;
pub use helper::*;

pub fn compute_merkle_root_from_proof(
    mut leaf: [u8; 32],
    mut index: u32,
    proof_nodes: &[[u8; 32]],
) -> [u8; 32] {
    for sibling in proof_nodes {
        let (left, right) = if index & 1 == 0 {
            (leaf, *sibling)
        } else {
            (*sibling, leaf)
        };
        leaf = sha256_hashv(&[&left, &right]).to_bytes();
        index >>= 1;
    }
    leaf
}

/// SHA256 of all settlement fields; order must match the guest preimage in `program/src/main.rs`.
#[allow(clippy::too_many_arguments)]
pub fn compute_public_outputs_hash(
    batch_id: &[u8; 32],
    window_start: i64,
    window_end: i64,
    num_trades: u32,
    market_regime_id: u64,
    market_max_multiplier: u32,
    pyth_checkpoints_hash: &[u8; 32],
    pool_balance_before: u64,
    pool_balance_after: u64,
    current_liability_before: u64,
    net_payout: i64,
    keeper_fee: u64,
    protocol_fee: u64,
    num_winners: u32,
    num_losers: u32,
    total_winners_payout: u64,
    total_losers_stake: u64,
    payouts_commitment: &[u8; 32],
    trades_commitment: &[u8; 32],
    nullifier_hash: &[u8; 32],
) -> [u8; 32] {
    sha256_hashv(&[
        batch_id.as_ref(),
        &window_start.to_le_bytes(),
        &window_end.to_le_bytes(),
        &num_trades.to_le_bytes(),
        &market_regime_id.to_le_bytes(),
        &market_max_multiplier.to_le_bytes(),
        pyth_checkpoints_hash.as_ref(),
        &pool_balance_before.to_le_bytes(),
        &pool_balance_after.to_le_bytes(),
        &current_liability_before.to_le_bytes(),
        &net_payout.to_le_bytes(),
        &keeper_fee.to_le_bytes(),
        &protocol_fee.to_le_bytes(),
        &num_winners.to_le_bytes(),
        &num_losers.to_le_bytes(),
        &total_winners_payout.to_le_bytes(),
        &total_losers_stake.to_le_bytes(),
        payouts_commitment.as_ref(),
        trades_commitment.as_ref(),
        nullifier_hash.as_ref(),
    ])
    .to_bytes()
}

/// SP1 `public_values_hash`: SHA256 of the 32-byte public values buffer, top 3 bits cleared.
/// Mirrors `sp1-verifier::hash_public_inputs`.
pub fn compute_public_values_hash(public_outputs_hash: &[u8; 32]) -> [u8; 32] {
    let mut result = sha256_hashv(&[public_outputs_hash.as_ref()]).to_bytes();
    result[0] &= 0x1f;
    result
}

/// Standard Groth16 over BN254 using Solana's alt_bn128 precompiles.
///
/// SP1 v6 Groth16 uses 5 public inputs:
///   s1 = sp1_vkey_hash, s2 = public_values_hash, s3 = exit_code = 0 (skipped),
///   s4 = vk_root, s5 = proof_nonce
///
/// vk_x = IC[0] + s1·IC[1] + s2·IC[2] + s4·IC[4] + s5·IC[5]
pub fn verify_groth16(
    vk_bytes: &[u8],
    proof_a: &[u8; 64],
    proof_b: &[u8; 128],
    proof_c: &[u8; 64],
    sp1_vkey_hash: &[u8; 32],
    public_values_hash: &[u8; 32],
    vk_root: &[u8; 32],
    proof_nonce: &[u8; 32],
) -> Result<()> {
    require!(vk_bytes.len() >= 896, CarnotError::InvalidGroth16Input);

    // VK blob layout (big-endian):
    //   [  0.. 32) sp1_vkey_hash   [ 32.. 64) vk_root
    //   [ 64..128) alpha_g1        [128..256) beta_g2
    //   [256..384) gamma_g2        [384..512) delta_g2
    //   [512..576) ic0_g1          [576..640) ic1_g1
    //   [640..704) ic2_g1          [704..768) ic3_g1 (exit_code=0, skipped)
    //   [768..832) ic4_g1          [832..896) ic5_g1
    let alpha_g1 = &vk_bytes[64..128];
    let beta_g2 = &vk_bytes[128..256];
    let gamma_g2 = &vk_bytes[256..384];
    let delta_g2 = &vk_bytes[384..512];
    let ic0_g1 = &vk_bytes[512..576];
    let ic1_g1 = &vk_bytes[576..640];
    let ic2_g1 = &vk_bytes[640..704];
    let ic4_g1 = &vk_bytes[768..832];
    let ic5_g1 = &vk_bytes[832..896];

    let t1 = g1_scalar_mul(ic1_g1, sp1_vkey_hash)?;
    let t2 = g1_scalar_mul(ic2_g1, public_values_hash)?;
    let t4 = g1_scalar_mul(ic4_g1, vk_root)?;
    let t5 = g1_scalar_mul(ic5_g1, proof_nonce)?;

    let vk_x = g1_add(ic0_g1, &t1)?;
    let vk_x = g1_add(&vk_x, &t2)?;
    let vk_x = g1_add(&vk_x, &t4)?;
    let vk_x = g1_add(&vk_x, &t5)?;

    // Negate A for the pairing equation: e(-A,B)·e(alpha,beta)·e(vk_x,gamma)·e(C,delta) == 1
    let neg_a = negate_g1_be(proof_a)?;

    let mut pairing_input = Vec::with_capacity(768);
    pairing_input.extend_from_slice(&neg_a);
    pairing_input.extend_from_slice(proof_b);
    pairing_input.extend_from_slice(alpha_g1);
    pairing_input.extend_from_slice(beta_g2);
    pairing_input.extend_from_slice(&vk_x);
    pairing_input.extend_from_slice(gamma_g2);
    pairing_input.extend_from_slice(proof_c);
    pairing_input.extend_from_slice(delta_g2);

    let output = alt_bn128_pairing_be(&pairing_input).map_err(|_| error!(CarnotError::InvalidProof))?;
    require!(output.len() == 32, CarnotError::InvalidProof);
    require!(
        output[31] == 1 && output[..31].iter().all(|b| *b == 0),
        CarnotError::InvalidProof
    );
    Ok(())
}

/// Hash `N_PYTH_CHECKPOINTS` PriceUpdateV2 accounts from `remaining_accounts`.
///
/// Layout per checkpoint (LE): price(i64) || conf(u64) || exponent(i32) || publish_time(i64)
/// Each account is verified to be owned by the Pyth Receiver program.
pub fn hash_pyth_checkpoints(
    remaining_accounts: &[AccountInfo],
    expected_feed_id: &[u8; 32],
) -> Result<[u8; 32]> {
    require!(
        remaining_accounts.len() == N_PYTH_CHECKPOINTS,
        CarnotError::InvalidGroth16Input
    );

    let mut all_bytes: Vec<&[u8]> = Vec::with_capacity(remaining_accounts.len() * 4);
    let mut price_bytes_arr: Vec<[u8; 8]> = Vec::with_capacity(remaining_accounts.len());
    let mut conf_bytes_arr: Vec<[u8; 8]> = Vec::with_capacity(remaining_accounts.len());
    let mut exp_bytes_arr: Vec<[u8; 4]> = Vec::with_capacity(remaining_accounts.len());
    let mut ts_bytes_arr: Vec<[u8; 8]> = Vec::with_capacity(remaining_accounts.len());

    for account_info in remaining_accounts.iter() {
        require!(
            account_info.owner == &pyth_solana_receiver_sdk::ID,
            CarnotError::InvalidOracleAccount
        );
        let data = account_info
            .try_borrow_data()
            .map_err(|_| error!(CarnotError::InvalidOracleAccount))?;
        let mut data_slice: &[u8] = &data;
        let price_update = PriceUpdateV2::try_deserialize(&mut data_slice)
            .map_err(|_| error!(CarnotError::InvalidOracleAccount))?;
        let msg = &price_update.price_message;
        require!(
            &msg.feed_id == expected_feed_id,
            CarnotError::InvalidOracleAccount
        );
        price_bytes_arr.push(msg.price.to_le_bytes());
        conf_bytes_arr.push(msg.conf.to_le_bytes());
        exp_bytes_arr.push(msg.exponent.to_le_bytes());
        ts_bytes_arr.push(msg.publish_time.to_le_bytes());
    }

    for i in 0..remaining_accounts.len() {
        all_bytes.push(&price_bytes_arr[i]);
        all_bytes.push(&conf_bytes_arr[i]);
        all_bytes.push(&exp_bytes_arr[i]);
        all_bytes.push(&ts_bytes_arr[i]);
    }

    Ok(sha256_hashv(&all_bytes).to_bytes())
}

fn g1_scalar_mul(point: &[u8], scalar: &[u8; 32]) -> Result<[u8; 64]> {
    let mut input = [0u8; 96];
    input[..64].copy_from_slice(point);
    input[64..].copy_from_slice(scalar);
    alt_bn128_g1_multiplication_be(&input)
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))?
        .try_into()
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))
}

fn g1_add(a: &[u8], b: &[u8; 64]) -> Result<[u8; 64]> {
    let mut input = [0u8; 128];
    input[..64].copy_from_slice(a);
    input[64..].copy_from_slice(b);
    alt_bn128_g1_addition_be(&input)
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))?
        .try_into()
        .map_err(|_| error!(CarnotError::InvalidGroth16Input))
}

/// Negate a BN254 G1 point in big-endian: (x, p − y).
/// BN254 Fq prime: 0x30644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd47
fn negate_g1_be(point: &[u8; 64]) -> Result<[u8; 64]> {
    const P: [u8; 32] = [
        0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58,
        0x5d, 0x97, 0x81, 0x6a, 0x91, 0x68, 0x71, 0xca, 0x8d, 0x3c, 0x20, 0x8c, 0x16, 0xd8, 0x7c,
        0xfd, 0x47,
    ];
    let mut neg = [0u8; 64];
    neg[..32].copy_from_slice(&point[..32]);
    let y = &point[32..64];
    // borrow is always 0 or 1; u8 is sufficient and enables From<u8> for i16.
    let mut borrow: u8 = 0;
    for i in (0..32).rev() {
        let diff = i16::from(P[i]) - i16::from(y[i]) - i16::from(borrow);
        if diff < 0 {
            // SAFETY: diff ∈ [−256, −1] here (P[i] ∈ [0,255], y[i] ∈ [0,255], borrow ∈ {0,1});
            // diff + 256 ∈ [0, 255], which fits in u8.
            neg[32 + i] = (diff + 256) as u8;
            borrow = 1;
        } else {
            // SAFETY: diff ∈ [0, 255] here; fits in u8.
            neg[32 + i] = diff as u8;
            borrow = 0;
        }
    }
    Ok(neg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sha256_hasher::hashv;

    fn hash_leaf(
        batch_id: [u8; 32],
        trade_id: u64,
        trader: [u8; 32],
        payout: u64,
        idx: u32,
    ) -> [u8; 32] {
        let mut trade_id_bytes = [0u8; 32];
        trade_id_bytes[..8].copy_from_slice(&trade_id.to_le_bytes());
        hashv(&[
            &batch_id,
            &trade_id_bytes,
            &trader,
            &payout.to_le_bytes(),
            &idx.to_le_bytes(),
        ])
        .to_bytes()
    }

    #[test]
    fn merkle_proof_validates_expected_leaf() {
        let batch_id = [7u8; 32];
        let trader_a = [1u8; 32];
        let trader_b = [2u8; 32];
        let leaf_a = hash_leaf(batch_id, 1, trader_a, 1_950_000, 0);
        let leaf_b = hash_leaf(batch_id, 2, trader_b, 0, 1);
        let root = hashv(&[&leaf_a, &leaf_b]).to_bytes();
        assert_eq!(compute_merkle_root_from_proof(leaf_a, 0, &[leaf_b]), root);
    }

    #[test]
    fn merkle_proof_rejects_wrong_sibling() {
        let batch_id = [9u8; 32];
        let trader_a = [3u8; 32];
        let leaf = hash_leaf(batch_id, 42, trader_a, 123, 0);
        let root = hashv(&[&leaf, &[5u8; 32]]).to_bytes();
        let computed = compute_merkle_root_from_proof(leaf, 0, &[[0u8; 32]]);
        assert_ne!(computed, root);
    }
}
