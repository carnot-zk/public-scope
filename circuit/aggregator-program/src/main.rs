#![no_main]
sp1_zkvm::entrypoint!(main);

use carnot_lib::AggregatorInputs;
use sha2::{Digest, Sha256};
use sp1_zkvm::lib::verify::verify_sp1_proof;

fn main() {
    let inputs: AggregatorInputs = sp1_zkvm::io::read();
    let total_chunks = usize::try_from(inputs.total_chunks).expect(
        "aggregator: `total_chunks` exceeds usize on this platform (malformed host input)",
    );
    assert_eq!(total_chunks, inputs.chunks.len());
    assert!(!inputs.chunks.is_empty());

    let mut total_trades: u64 = 0;
    let mut total_winners: u64 = 0;
    let mut total_losers: u64 = 0;
    let mut total_winners_payout: u128 = 0;
    let mut total_losers_stake: u128 = 0;
    let mut aggregate_net_payout: i128 = 0;
    let mut aggregate_keeper_fee: u128 = 0;
    let mut aggregate_protocol_fee: u128 = 0;

    let mut hasher = Sha256::new();
    hasher.update(&inputs.batch_id);
    hasher.update(&inputs.total_chunks.to_le_bytes());

    for (expected_idx, chunk) in inputs.chunks.iter().enumerate() {
        verify_sp1_proof(&chunk.vk_digest, &chunk.pv_digest);

        assert_eq!(chunk.outputs.chunk_index as usize, expected_idx);
        assert_eq!(chunk.outputs.total_chunks, inputs.total_chunks);
        assert_eq!(chunk.outputs.batch_id, inputs.batch_id);

        hasher.update(&chunk.outputs.chunk_index.to_le_bytes());
        hasher.update(&chunk.outputs.public_outputs_hash);
        hasher.update(&chunk.outputs.payouts_commitment);
        hasher.update(&chunk.outputs.trades_commitment);
        hasher.update(&chunk.outputs.nullifier_hash);
        hasher.update(&chunk.outputs.num_trades.to_le_bytes());
        hasher.update(&chunk.outputs.num_winners.to_le_bytes());
        hasher.update(&chunk.outputs.num_losers.to_le_bytes());
        hasher.update(&chunk.outputs.net_payout.to_le_bytes());
        hasher.update(&chunk.outputs.keeper_fee.to_le_bytes());
        hasher.update(&chunk.outputs.protocol_fee.to_le_bytes());

        total_trades += chunk.outputs.num_trades as u64;
        total_winners += chunk.outputs.num_winners as u64;
        total_losers += chunk.outputs.num_losers as u64;
        total_winners_payout += chunk.outputs.total_winners_payout as u128;
        total_losers_stake += chunk.outputs.total_losers_stake as u128;
        aggregate_net_payout += chunk.outputs.net_payout as i128;
        aggregate_keeper_fee += chunk.outputs.keeper_fee as u128;
        aggregate_protocol_fee += chunk.outputs.protocol_fee as u128;
    }

    hasher.update(&total_trades.to_le_bytes());
    hasher.update(&total_winners.to_le_bytes());
    hasher.update(&total_losers.to_le_bytes());
    hasher.update(&total_winners_payout.to_le_bytes());
    hasher.update(&total_losers_stake.to_le_bytes());
    hasher.update(&aggregate_net_payout.to_le_bytes());
    hasher.update(&aggregate_keeper_fee.to_le_bytes());
    hasher.update(&aggregate_protocol_fee.to_le_bytes());

    let final_hash: [u8; 32] = hasher.finalize().into();
    sp1_zkvm::io::commit(&final_hash);
}
