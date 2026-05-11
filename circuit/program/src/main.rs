//! Carnot settlement guest: constraints on private inputs; commits `public_outputs_hash` as the SP1 public value.
//!
//! Public preimage field order (must match on-chain):
//! 1.  batch_id                [u8; 32]
//! 2.  window_start            i64
//! 3.  window_end              i64
//! 4.  num_trades              u32
//! 5.  market_regime_id        u64
//! 6.  market_max_multiplier   u32
//! 7.  pyth_checkpoints_hash   [u8; 32]  (SHA256 of N_PYTH_CHECKPOINTS Pyth oracle rows)
//! 8.  pool_balance_before     u64
//! 9.  pool_balance_after      u64
//! 10. current_liability_before u64
//! 11. net_payout              i64
//! 12. keeper_fee              u64
//! 13. protocol_fee            u64
//! 14. num_winners             u32
//! 15. num_losers              u32
//! 16. total_winners_payout    u64
//! 17. total_losers_stake      u64
//! 18. payouts_commitment      [u8; 32]
//! 19. trades_commitment       [u8; 32]
//! 20. nullifier_hash          [u8; 32]

#![no_main]
sp1_zkvm::entrypoint!(main);

use carnot_lib::{
    compute_payouts_merkle_root, compute_trade_commitment, hash_nullifier_from_sorted_trade_ids,
    hash_payout_leaf, hash_pyth_checkpoints, normalize_pyth_price, CircuitInputs, MAX_DEVIATION_BPS,
    MIN_TRADE_MULTIPLIER_BPS, N_PYTH_CHECKPOINTS,
};
use sha2::{Digest, Sha256};

pub fn main() {
    println!("cycle-tracker-start: total");
    println!("cycle-tracker-start: io-read");
    let inputs: CircuitInputs = sp1_zkvm::io::read::<CircuitInputs>();
    println!("cycle-tracker-end: io-read");

    let CircuitInputs {
        trades,
        trade_commitments,
        sorted_trade_ids,
        ohlc,
        batch_id,
        window_start,
        window_end,
        pool_balance_before,
        current_liability_before,
        keeper_fee_bps,
        protocol_fee_bps,
        market_max_multiplier,
        market_regime_id,
        pyth_feed_id: _pyth_feed_id,
        pyth_checkpoints,
    } = inputs;

    let num_trades = trades.len();
    assert!(!trades.is_empty(), "batch cannot be empty");
    assert_eq!(
        trade_commitments.len(),
        num_trades,
        "trade_commitments length must equal trades length"
    );
    assert_eq!(
        sorted_trade_ids.len(),
        num_trades,
        "sorted_trade_ids length must equal trades length"
    );

    if !ohlc.is_empty() {
        assert!(
            ohlc[0].ts <= window_start,
            "B3: OHLC does not cover window_start"
        );
        assert!(
            ohlc[ohlc.len() - 1].ts >= window_end,
            "B3: OHLC does not cover window_end"
        );
    }
    assert!(!ohlc.is_empty(), "B3: OHLC data is required for settlement — empty OHLC would mark all trades as losses");

    println!("cycle-tracker-start: ohlc-validation");
    for i in 0..ohlc.len() {
        let tick = &ohlc[i];
        if i > 0 {
            assert!(ohlc[i - 1].ts < tick.ts, "B1: OHLC ticks not in chronological order");
        }
        assert!(tick.low <= tick.open);
        assert!(tick.low <= tick.close);
        assert!(tick.high >= tick.open);
        assert!(tick.high >= tick.close);
        assert!(tick.low <= tick.high);
    }

    assert_eq!(
        pyth_checkpoints.len(),
        N_PYTH_CHECKPOINTS,
        "B4: expected {} Pyth checkpoints, got {}",
        N_PYTH_CHECKPOINTS,
        pyth_checkpoints.len()
    );
    for i in 0..pyth_checkpoints.len() {
        let cp = &pyth_checkpoints[i];
        assert!(cp.publish_time >= window_start, "B4: checkpoint[{}] before window_start", i);
        assert!(cp.publish_time <= window_end, "B4: checkpoint[{}] after window_end", i);
        if i > 0 {
            assert!(
                pyth_checkpoints[i - 1].publish_time < cp.publish_time,
                "B4: checkpoints not strictly sorted by publish_time"
            );
        }
    }

    let pyth_checkpoints_hash = hash_pyth_checkpoints(&pyth_checkpoints);
    println!("cycle-tracker-end: ohlc-validation");

    // B6: normalized Pyth vs nearest OHLC close within MAX_DEVIATION_BPS (integer bps inequality below).
    println!("cycle-tracker-start: pyth-deviation-check");
    for (i, cp) in pyth_checkpoints.iter().enumerate() {
        let normalized = normalize_pyth_price(cp.price, cp.exponent).expect(
            "B6: negative Pyth price or overflow when normalizing to micro-USD",
        );

        let idx = ohlc.partition_point(|t| t.ts <= cp.publish_time);
        let nearest = if ohlc.is_empty() {
            panic!("B6: no OHLC data to compare against checkpoint[{}]", i);
        } else if idx == 0 {
            &ohlc[0]
        } else if idx >= ohlc.len() {
            &ohlc[ohlc.len() - 1]
        } else {
            let before = &ohlc[idx - 1];
            let after = &ohlc[idx];
            let before_dist = u64::try_from(cp.publish_time.saturating_sub(before.ts))
                .expect("B6: checkpoint publish_time before OHLC tick ts");
            let after_dist = u64::try_from(after.ts.saturating_sub(cp.publish_time))
                .expect("B6: OHLC tick ts before checkpoint publish_time");
            if before_dist <= after_dist { before } else { after }
        };

        let ohlc_close = nearest.close;
        let diff = if normalized > ohlc_close {
            normalized - ohlc_close
        } else {
            ohlc_close - normalized
        };
        // |pyth−ohlc|/pyth ≤ MAX_DEVIATION_BPS/10_000  ⇔  diff*10_000 ≤ normalized*MAX_DEVIATION_BPS
        assert!(
            diff.saturating_mul(10_000) <= normalized.saturating_mul(MAX_DEVIATION_BPS),
            "B6: checkpoint[{}] Pyth price deviates > {}bps from internal OHLC close",
            i,
            MAX_DEVIATION_BPS
        );
    }
    println!("cycle-tracker-end: pyth-deviation-check");

    for pair in sorted_trade_ids.windows(2) {
        assert!(
            pair[0] < pair[1],
            "A2: sorted_trade_ids not strictly sorted or contains duplicate trade_id"
        );
    }

    for pair in trades.windows(2) {
        assert!(
            pair[0].trader_pubkey <= pair[1].trader_pubkey,
            "A3: trades not sorted by trader_pubkey"
        );
    }

    let mut trades_commitment_hasher = Sha256::new();
    let mut payout_leaves = Vec::with_capacity(num_trades);

    let mut num_winners: u32 = 0;
    let mut num_losers: u32 = 0;
    let mut total_winners_payout: u64 = 0;
    let mut total_losers_stake: u64 = 0;
    let mut net_payout: i64 = 0;
    let mut settled_liability: u64 = 0;
    println!("cycle-tracker-start: trade-loop");

    for (trade, &stored_commitment) in trades.iter().zip(&trade_commitments) {
        let computed = compute_trade_commitment(trade);
        assert_eq!(computed, stored_commitment);

        assert!(trade.multiplier_bps >= MIN_TRADE_MULTIPLIER_BPS);
        assert!(trade.multiplier_bps <= market_max_multiplier);

        assert!(trade.window_start < trade.window_end);
        assert!(trade.window_start >= window_start);
        assert!(trade.window_end <= window_end);

        assert!(trade.stake_usdt > 0);

        trades_commitment_hasher.update(&stored_commitment);

        let gross_payout: u64 = (trade.stake_usdt as u128)
            .checked_mul(trade.multiplier_bps as u128)
            .expect("C4: stake * multiplier overflow")
            .checked_div(10_000)
            .expect("C4: gross payout divide by BPS")
            .try_into()
            .expect("C4: gross payout exceeds u64");
        assert_eq!(gross_payout, trade.max_payout_usdt);
        settled_liability = settled_liability
            .checked_add(trade.max_payout_usdt)
            .expect("C4: settled_liability accumulator overflow");

        // Settlement close: last OHLC close with ts ≤ trade.window_end; win iff [band_lower, band_upper).
        let settle_idx = ohlc.partition_point(|t| t.ts <= trade.window_end);
        let settlement_close = if settle_idx == 0 {
            ohlc[0].close
        } else {
            ohlc[settle_idx - 1].close
        };
        let won = carnot_lib::verify_trade_outcome_close_in_band(trade, settlement_close);

        let payout: u64;
        if won {
            let gain = gross_payout
                .checked_sub(trade.stake_usdt)
                .expect("C: winner gross below stake");
            net_payout = net_payout
                .checked_add(gain as i64)
                .expect("C: net_payout accumulator overflow");
            total_winners_payout = total_winners_payout
                .checked_add(gross_payout)
                .expect("C: total_winners_payout overflow");
            num_winners += 1;
            payout = gross_payout;
        } else {
            net_payout = net_payout
                .checked_sub(trade.stake_usdt as i64)
                .expect("C: net_payout accumulator underflow");
            total_losers_stake = total_losers_stake
                .checked_add(trade.stake_usdt)
                .expect("C: total_losers_stake overflow");
            num_losers += 1;
            payout = 0;
        }

        let payout_index = payout_leaves.len() as u32;
        payout_leaves.push(hash_payout_leaf(
            &batch_id,
            &trade.trade_id,
            &trade.trader_pubkey,
            payout,
            payout_index,
        ));
    }
    println!("cycle-tracker-end: trade-loop");

    let _ = settled_liability;

    let trades_commitment: [u8; 32] = trades_commitment_hasher.finalize().into();
    let payouts_commitment = compute_payouts_merkle_root(&payout_leaves);

    let net_abs = net_payout.unsigned_abs();
    let keeper_fee: u64 = (net_abs as u128)
        .checked_mul(keeper_fee_bps as u128)
        .expect("D3: |net_payout| * keeper_fee_bps overflow")
        .checked_div(10_000)
        .expect("D3: keeper fee divide by BPS")
        .try_into()
        .expect("D3: keeper fee exceeds u64");

    let protocol_fee: u64 = if net_payout < 0 {
        (net_abs as u128)
            .checked_mul(protocol_fee_bps as u128)
            .expect("D4: |net_payout| * protocol_fee_bps overflow")
            .checked_div(10_000)
            .expect("D4: protocol fee divide by BPS")
            .try_into()
            .expect("D4: protocol fee exceeds u64")
    } else {
        0
    };

    let pool_balance_after: u64 = if net_payout >= 0 {
        pool_balance_before
            .checked_sub(net_payout as u64)
            .expect("D1: pool_balance_after underflow (net payout)")
            .checked_sub(keeper_fee)
            .expect("D1: pool_balance_after underflow (keeper fee)")
    } else {
        pool_balance_before
            .checked_add(net_abs)
            .expect("D1: pool_balance_after overflow (LP pays winners)")
            .checked_sub(keeper_fee)
            .expect("D1: pool_balance_after underflow (keeper fee on LP loss)")
    };

    assert_eq!(
        (num_winners + num_losers) as usize,
        num_trades,
        "D6: num_winners + num_losers != num_trades"
    );

    let nullifier_hash = hash_nullifier_from_sorted_trade_ids(&batch_id, &sorted_trade_ids);
    println!("cycle-tracker-end: total");

    let public_outputs_hash: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(&batch_id);
        h.update(&window_start.to_le_bytes());
        h.update(&window_end.to_le_bytes());
        h.update(&(num_trades as u32).to_le_bytes());
        h.update(&market_regime_id.to_le_bytes());
        h.update(&market_max_multiplier.to_le_bytes());
        h.update(&pyth_checkpoints_hash);
        h.update(&pool_balance_before.to_le_bytes());
        h.update(&pool_balance_after.to_le_bytes());
        h.update(&current_liability_before.to_le_bytes());
        h.update(&net_payout.to_le_bytes());
        h.update(&keeper_fee.to_le_bytes());
        h.update(&protocol_fee.to_le_bytes());
        h.update(&num_winners.to_le_bytes());
        h.update(&num_losers.to_le_bytes());
        h.update(&total_winners_payout.to_le_bytes());
        h.update(&total_losers_stake.to_le_bytes());
        h.update(&payouts_commitment);
        h.update(&trades_commitment);
        h.update(&nullifier_hash);
        h.finalize().into()
    };
    sp1_zkvm::io::commit(&public_outputs_hash);
}
