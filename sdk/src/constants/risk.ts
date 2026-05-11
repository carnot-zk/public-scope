/** Minimum pool solvency ratio in bps. Hard on-chain invariant in PoolReserve. */
export const MIN_SOLVENCY_RATIO_BPS = 15_000; // 1.5x

/** Maximum pool utilization in bps before far-from-spot bands are disabled. */
export const MAX_UTILIZATION_BPS = 7_000; // 70%

/** Maximum single-bet exposure as bps of pool balance. */
export const MAX_SINGLE_BET_EXPOSURE_BPS = 200; // 2%

/** Platform edge in bps (7%) */
export const PLATFORM_EDGE_BPS = 700;

/** Keeper fee bps currently used by backend settlement computation */
export const KEEPER_FEE_BPS = 25; // 0.25%

/** Protocol fee bps currently used by backend settlement computation */
export const PROTOCOL_FEE_BPS = 100; // 1%

/** Max multiplier bps accepted by settlement computation */
export const MARKET_MAX_MULTIPLIER_BPS = 100_000;
