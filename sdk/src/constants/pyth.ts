/**
 * Pyth Core price feeds used by Carnot (Hermes Crypto vs USD pairs, 32-byte hex, no 0x prefix).
 * Feed ids are global in Pyth Core; the same value applies on devnet and mainnet Solana.
 */

/** Default Carnot perp markets and extra USD pairs the stack may reference. */
export const CARNOT_PYTH_USD_FEED_IDS = {
  BTC_USD:
    'e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43',
  SOL_USD:
    'ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d',
  JUP_USD:
    '0a0408d619e9380abad35060f9192039ed5042fa6f82301d0e48bb52be830996',
} as const;

export type CarnotPythUsdFeedKey = keyof typeof CARNOT_PYTH_USD_FEED_IDS;

/** Typed accessor for any Carnot Pyth USD feed id. */
export function getCarnotPythUsdFeedId(key: CarnotPythUsdFeedKey): string {
  return CARNOT_PYTH_USD_FEED_IDS[key];
}

/**
 * Maps default `marketId` (e.g. from `CARNOT_MARKETS_JSON`) to a Pyth USD feed key.
 * Markets not listed here still supply `pythFeedId` explicitly in config.
 */
export const CARNOT_DEFAULT_MARKET_PYTH_USD_KEY = {
  btcusdt: 'BTC_USD',
  solusdt: 'SOL_USD',
} as const satisfies Record<string, CarnotPythUsdFeedKey>;

export type CarnotDefaultMarketId = keyof typeof CARNOT_DEFAULT_MARKET_PYTH_USD_KEY;

/** Resolve the default Pyth feed id for a built-in Carnot market id, if known. */
export function getCarnotPythUsdFeedIdForDefaultMarket(
  marketId: string,
): string | undefined {
  if (!(marketId in CARNOT_DEFAULT_MARKET_PYTH_USD_KEY)) return undefined;
  const key =
    CARNOT_DEFAULT_MARKET_PYTH_USD_KEY[marketId as CarnotDefaultMarketId];
  return CARNOT_PYTH_USD_FEED_IDS[key];
}

/** Flat names aligned with backend env vars (`PYTH_*_USD_FEED_ID`). */
export const PYTH_BTC_USD_FEED_ID = CARNOT_PYTH_USD_FEED_IDS.BTC_USD;
export const PYTH_SOL_USD_FEED_ID = CARNOT_PYTH_USD_FEED_IDS.SOL_USD;
export const PYTH_JUP_USD_FEED_ID = CARNOT_PYTH_USD_FEED_IDS.JUP_USD;

/** Maximum age (seconds) of a Pyth price attestation before it is rejected. */
export const PYTH_MAX_PRICE_AGE_SECS = 60;
