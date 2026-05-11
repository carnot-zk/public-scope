//! Shared types for circuits and on-chain code. All money uses integer micro-USD (6 dp);
//! no floating-point in the guest.

/// Pyth oracle checkpoints per settlement batch (guest, host `hash_pyth_checkpoints`, on-chain).
/// **Must match** `carnot_engine::constants::N_PYTH_CHECKPOINTS` and `@carnot/sdk` `N_PYTH_CHECKPOINTS`.
pub const N_PYTH_CHECKPOINTS: usize = 3;

/// Max bps deviation between normalized Pyth price and nearest OHLC close (settlement guest B6).
/// TypeScript: `MAX_PYTH_VS_OHLC_DEVIATION_BPS` in `@carnot/sdk`.
pub const MAX_DEVIATION_BPS: u64 = 200;

/// Basis-point divisor for fees and multipliers (matches on-chain bps math).
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Minimum trade multiplier (1.0×). Settlement guest constraint A4.
pub const MIN_TRADE_MULTIPLIER_BPS: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    Up,
    Down,
}

/// A single record — private input to settlement circuit.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeRecord {
    pub trade_id: [u8; 32],
    pub trader_pubkey: [u8; 32],
    pub direction: Direction,
    /// Entry price in micro-USD
    pub entry_price: u64,
    /// Exit price in micro-USD
    pub exit_price: u64,
    /// Stake in micro-USDT
    pub stake_usdt: u64,
    /// Quoted multiplier in bps (e.g. 19500 = 1.95x)
    pub multiplier_bps: u32,
    pub window_start: i64,
    pub window_end: i64,
    /// Band lower bound in micro-USD
    pub band_lower: u64,
    /// Band upper bound in micro-USD
    pub band_upper: u64,
    /// Maximum gross payout the protocol can owe for this trade (stake * multiplier / 10_000)
    pub max_payout_usdt: u64,
}

/// One second of OHLC price data.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OhlcTick {
    pub ts: i64,
    pub open: u64,
    pub high: u64,
    pub low: u64,
    pub close: u64,
}

/// Raw Pyth snapshot; micro-USD is `price * 10^(exponent + 6)` (see `normalize_pyth_price`).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PythCheckpoint {
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
}

/// Inputs passed from host to guest circuit.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CircuitInputs {
    /// Trades pre-sorted by trader_pubkey (ascending).
    pub trades: Vec<TradeRecord>,
    pub trade_commitments: Vec<[u8; 32]>,
    /// Trade IDs pre-sorted ascending.
    pub sorted_trade_ids: Vec<[u8; 32]>,
    pub ohlc: Vec<OhlcTick>,
    pub batch_id: [u8; 32],
    pub window_start: i64,
    pub window_end: i64,
    pub pool_balance_before: u64,
    pub current_liability_before: u64,
    pub keeper_fee_bps: u64,
    pub protocol_fee_bps: u64,
    pub market_max_multiplier: u32,
    pub market_regime_id: u64,
    pub pyth_feed_id: [u8; 32],
    /// Oracle price checkpoints at window_start, midpoint, and window_end.
    pub pyth_checkpoints: Vec<PythCheckpoint>,
}

/// Inputs for proving one chunk of a larger settlement batch.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CircuitChunkInputs {
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub inner: CircuitInputs,
}

/// Public summary emitted for each proven chunk.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChunkOutputs {
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub batch_id: [u8; 32],
    pub window_start: i64,
    pub window_end: i64,
    pub num_trades: u32,
    pub net_payout: i64,
    pub pool_balance_before: u64,
    pub pool_balance_after: u64,
    pub current_liability_before: u64,
    pub keeper_fee: u64,
    pub protocol_fee: u64,
    pub num_winners: u32,
    pub num_losers: u32,
    pub total_winners_payout: u64,
    pub total_losers_stake: u64,
    pub market_regime_id: u64,
    pub payouts_commitment: [u8; 32],
    pub trades_commitment: [u8; 32],
    pub nullifier_hash: [u8; 32],
    pub public_outputs_hash: [u8; 32],
}

/// Wire type for chunk proofs into the aggregator guest (host writes, guest reads).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChunkProofInput {
    pub vk_digest: [u32; 8],
    pub pv_digest: [u8; 32],
    pub outputs: ChunkOutputs,
}

/// Aggregator circuit input: identifies the batch and carries one [`ChunkProofInput`] per chunk.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AggregatorInputs {
    pub batch_id: [u8; 32],
    pub total_chunks: u32,
    pub chunks: Vec<ChunkProofInput>,
}

/// Hash one payout leaf for a batch claim tree.
///
/// Leaf preimage:
///   batch_id || trade_id || trader_pubkey || payout_le || index_le
pub fn hash_payout_leaf(
    batch_id: &[u8; 32],
    trade_id: &[u8; 32],
    trader_pubkey: &[u8; 32],
    payout: u64,
    index: u32,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut h = Sha256::new();
    h.update(batch_id);
    h.update(trade_id);
    h.update(trader_pubkey);
    h.update(&payout.to_le_bytes());
    h.update(&index.to_le_bytes());
    h.finalize().into()
}

/// Hash two Merkle siblings in left-right order.
pub fn hash_merkle_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut h = Sha256::new();
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// Compute a deterministic Merkle root from payout leaves.
///
/// If the level has an odd number of nodes, the final node is duplicated.
/// Empty input returns the zero hash.
pub fn compute_payouts_merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }

    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() {
                level[i + 1]
            } else {
                level[i]
            };
            next.push(hash_merkle_pair(&left, &right));
            i += 2;
        }
        level = next;
    }

    level[0]
}

/// Keccak256 trade commitment; LE multi-byte fields; must match on-chain `place_trade`.
pub fn compute_trade_commitment(t: &TradeRecord) -> [u8; 32] {
    use sha3::{Digest, Keccak256};

    let direction_bit = match t.direction {
        Direction::Up => 0u64,
        Direction::Down => 1u64,
    };
    let packed_direction_multiplier = direction_bit | ((t.multiplier_bps as u64) << 1);

    let mut hasher = Keccak256::new();
    hasher.update(&t.trader_pubkey);
    // Commitment hashes `trade_id` as opaque 32 bytes (may differ from on-chain u64 seed encoding).
    hasher.update(&t.trade_id);
    hasher.update(&t.band_lower.to_le_bytes());
    hasher.update(&t.band_upper.to_le_bytes());
    hasher.update(&t.stake_usdt.to_le_bytes());
    hasher.update(&packed_direction_multiplier.to_le_bytes());
    hasher.update(&t.window_start.to_le_bytes());
    hasher.update(&t.window_end.to_le_bytes());
    hasher.finalize().into()
}

/// Direction-aware trade outcome determination.
///
/// - Direction::Up:   wins if any tick's high reached or exceeded `band_upper`.
/// - Direction::Down: wins if any tick's low reached or fell below `band_lower`.
pub fn verify_trade_outcome_directional(trade: &TradeRecord, ohlc: &[OhlcTick]) -> bool {
    match trade.direction {
        Direction::Up => {
            for tick in ohlc {
                if tick.high >= trade.band_upper {
                    return true;
                }
            }
            false
        }
        Direction::Down => {
            for tick in ohlc {
                if tick.low <= trade.band_lower {
                    return true;
                }
            }
            false
        }
    }
}

/// Wins iff settlement close is in `[band_lower, band_upper)` (half-open).
/// Close = last `OhlcTick.close` with `ts <= trade.window_end`.
pub fn verify_trade_outcome_close_in_band(
    trade: &TradeRecord,
    settlement_close: u64,
) -> bool {
    settlement_close >= trade.band_lower && settlement_close < trade.band_upper
}

/// Compute P&L for a single trade in micro-USDT (signed: positive = winner payout).
/// Returns `None` if stake × multiplier does not fit in the payout arithmetic.
pub fn compute_pnl(trade: &TradeRecord, won: bool) -> Option<i64> {
    if won {
        let gross = (trade.stake_usdt as u128)
            .checked_mul(trade.multiplier_bps as u128)?
            .checked_div(BPS_DENOMINATOR as u128)? as i64;
        Some(gross - trade.stake_usdt as i64)
    } else {
        Some(-(trade.stake_usdt as i64))
    }
}

/// SHA-256(batch_id || sorted trade_ids). Sorts a copy; the guest hashes the given order.
pub fn compute_nullifier(batch_id: &[u8; 32], trades: &[TradeRecord]) -> [u8; 32] {
    let mut trade_ids: Vec<[u8; 32]> = trades.iter().map(|t| t.trade_id).collect();
    trade_ids.sort();

    hash_nullifier_from_sorted_trade_ids(batch_id, &trade_ids)
}

/// SHA-256(batch_id || id_0 || … || id_{n-1}) for trade IDs already sorted ascending (circuit + host).
pub fn hash_nullifier_from_sorted_trade_ids(
    batch_id: &[u8; 32],
    sorted_trade_ids: &[[u8; 32]],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(batch_id);
    for id in sorted_trade_ids {
        hasher.update(id);
    }
    hasher.finalize().into()
}

/// SHA-256 of checkpoints (each field LE); must match guest and host `compute_public_outputs`.
pub fn hash_pyth_checkpoints(checkpoints: &[PythCheckpoint]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut h = Sha256::new();
    for cp in checkpoints {
        h.update(&cp.price.to_le_bytes());
        h.update(&cp.conf.to_le_bytes());
        h.update(&cp.exponent.to_le_bytes());
        h.update(&cp.publish_time.to_le_bytes());
    }
    h.finalize().into()
}

/// Pyth `price` to micro-USD: `price * 10^(exponent + 6)`.
///
/// Returns `None` for negative prices or when a positive shift would overflow `u64`.
pub fn normalize_pyth_price(price: i64, exponent: i32) -> Option<u64> {
    if price < 0 {
        return None;
    }
    let p = price as u64;
    let shift = exponent + 6i32;
    if shift >= 0 {
        p.checked_mul(10u64.pow(shift as u32))
    } else {
        Some(p / 10u64.pow((-shift) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tick_full(ts: i64, open: u64, high: u64, low: u64, close: u64) -> OhlcTick {
        OhlcTick { ts, open, high, low, close }
    }

    fn make_tick(ts: i64, price: u64) -> OhlcTick {
        OhlcTick { ts, open: price, high: price, low: price, close: price }
    }

    fn make_trade(band_lower: u64, band_upper: u64, stake: u64, multiplier_bps: u32) -> TradeRecord {
        let entry = band_lower.saturating_sub(1000);
        TradeRecord {
            trade_id: [0u8; 32],
            trader_pubkey: [0u8; 32],
            direction: Direction::Up,
            entry_price: entry,
            exit_price: entry,
            stake_usdt: stake,
            multiplier_bps,
            window_start: 0,
            window_end: 5,
            band_lower,
            band_upper,
            max_payout_usdt: (stake as u128 * multiplier_bps as u128 / BPS_DENOMINATOR as u128) as u64,
        }
    }

    #[test]
    fn test_direct_band_touch_up() {
        let trade = make_trade(97_020_000_000, 97_040_000_000, 1_000_000, 19_500);
        let ohlc = vec![
            make_tick_full(0, 97_010_000_000, 97_045_000_000, 97_010_000_000, 97_015_000_000),
        ];
        assert!(verify_trade_outcome_directional(&trade, &ohlc));
    }

    #[test]
    fn test_direct_band_touch_down() {
        let mut trade = make_trade(97_020_000_000, 97_040_000_000, 1_000_000, 19_500);
        trade.direction = Direction::Down;
        let ohlc = vec![
            make_tick_full(0, 97_030_000_000, 97_035_000_000, 97_018_000_000, 97_028_000_000),
        ];
        assert!(verify_trade_outcome_directional(&trade, &ohlc));
    }

    #[test]
    fn test_no_touch_up() {
        let trade = TradeRecord {
            trade_id: [0u8; 32],
            trader_pubkey: [0u8; 32],
            direction: Direction::Up,
            entry_price: 97_000_000_000,
            exit_price: 97_000_000_000,
            stake_usdt: 1_000_000,
            multiplier_bps: 50_000,
            window_start: 0,
            window_end: 5,
            band_lower: 200_000_000_000,
            band_upper: 210_000_000_000,
            max_payout_usdt: 5_000_000,
        };
        let ohlc = vec![
            make_tick(0, 97_000_000_000),
            make_tick(1, 97_005_000_000),
            make_tick(2, 96_990_000_000),
        ];
        assert!(!verify_trade_outcome_directional(&trade, &ohlc));
    }

    #[test]
    fn test_directional_down_no_win_when_only_upper_touched() {
        let trade = TradeRecord {
            trade_id: [0u8; 32],
            trader_pubkey: [0u8; 32],
            direction: Direction::Down,
            entry_price: 97_030_000_000,
            exit_price: 97_030_000_000,
            stake_usdt: 1_000_000,
            multiplier_bps: 19_500,
            window_start: 0,
            window_end: 5,
            band_lower: 97_020_000_000,
            band_upper: 97_040_000_000,
            max_payout_usdt: 1_950_000,
        };
        let ohlc = vec![
            make_tick_full(0, 97_030_000_000, 97_045_000_000, 97_025_000_000, 97_030_000_000),
        ];
        assert!(!verify_trade_outcome_directional(&trade, &ohlc));
    }

    #[test]
    fn test_pnl_win() {
        let trade = make_trade(5_000, 10_000, 1_000_000, 20_000);
        let pnl = compute_pnl(&trade, true).expect("fixture trade must yield valid PnL");
        assert_eq!(pnl, 1_000_000);
    }

    #[test]
    fn test_pnl_loss() {
        let trade = make_trade(5_000, 10_000, 1_000_000, 20_000);
        let pnl = compute_pnl(&trade, false).expect("fixture trade must yield valid PnL");
        assert_eq!(pnl, -1_000_000);
    }

    #[test]
    fn test_nullifier_is_deterministic() {
        let batch_id = [1u8; 32];
        let mut trades = vec![
            make_trade(5_000, 10_000, 1_000_000, 20_000),
            make_trade(5_000, 10_000, 2_000_000, 19_000),
        ];
        trades[0].trade_id = [1u8; 32];
        trades[1].trade_id = [2u8; 32];
        trades[0].max_payout_usdt = 2_000_000;
        trades[1].max_payout_usdt = 3_800_000;

        let n1 = compute_nullifier(&batch_id, &trades);
        trades.reverse();
        let n2 = compute_nullifier(&batch_id, &trades);
        assert_eq!(n1, n2, "nullifier must be order-independent");
    }

    #[test]
    fn test_trade_commitment_deterministic() {
        let trade = make_trade(1_000_000, 2_000_000, 500_000, 15_000);
        let c1 = compute_trade_commitment(&trade);
        let c2 = compute_trade_commitment(&trade);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_max_payout_in_trade_record() {
        let trade = make_trade(97_020_000_000, 97_040_000_000, 1_000_000, 19_500);
        assert_eq!(trade.max_payout_usdt, 1_950_000);
    }

    #[test]
    fn test_payout_merkle_root_deterministic() {
        let batch_id = [0xabu8; 32];
        let trade_a = [0x11u8; 32];
        let trade_b = [0x22u8; 32];
        let pk_a = [0xaau8; 32];
        let pk_b = [0xbbu8; 32];

        let leaves = vec![
            hash_payout_leaf(&batch_id, &trade_a, &pk_a, 1_000_000, 0),
            hash_payout_leaf(&batch_id, &trade_b, &pk_b, 0, 1),
        ];
        let root_1 = compute_payouts_merkle_root(&leaves);
        let root_2 = compute_payouts_merkle_root(&leaves);
        assert_eq!(root_1, root_2);
    }
}
