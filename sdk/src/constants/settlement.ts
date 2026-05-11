/**
 * Settlement / ZK circuit invariants shared with `carnot-circuit` (`carnot-lib`) and
 * `carnot-programs` (`carnot_engine::constants`). Keep these numerically identical.
 */

/** Pyth checkpoints per batch (window interior layout is host-defined; count is fixed). */
export const N_PYTH_CHECKPOINTS = 3;

/** Guest B6: max bps deviation between normalized Pyth price and nearest OHLC close. */
export const MAX_PYTH_VS_OHLC_DEVIATION_BPS = 200;

/** Divisor for all `*_bps` fields (fees, multipliers). */
export const BPS_DENOMINATOR = 10_000;

/** Minimum trade multiplier (1.0x); matches settlement guest constraint A4. */
export const MIN_TRADE_MULTIPLIER_BPS = 10_000;
