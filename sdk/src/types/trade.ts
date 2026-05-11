export type Direction = "Up" | "Down";

/** A single tap trade record — the private input to the settlement ZK circuit. */
export interface TradeRecord {
  tradeId: string;
  traderPubkey: string;
  direction: Direction;
  /** Entry price in micro-USD (6 decimal places, e.g. 97_020_000_000 = $97,020.00) */
  entryPrice: bigint;
  /** Exit price in micro-USD */
  exitPrice: bigint;
  /** Stake in micro-USDT (6 decimals) */
  stakeUsdt: bigint;
  /** Quoted multiplier in bps (e.g. 19500 = 1.95x) */
  multiplierBps: number;
  /** Unix timestamp seconds */
  windowStart: number;
  /** Unix timestamp seconds */
  windowEnd: number;
  /** $20 band lower bound in micro-USD */
  bandLower: bigint;
  /** $20 band upper bound in micro-USD */
  bandUpper: bigint;
  /** Maximum payout in micro-USDT */
  maxPayoutUsdt: bigint;
}

/** One second of OHLC price data from the platform's internal feed. */
export interface OhlcTick {
  /** Unix timestamp seconds (start of this 1-second candle) */
  ts: number;
  /** Open in micro-USD */
  open: bigint;
  /** High in micro-USD */
  high: bigint;
  /** Low in micro-USD */
  low: bigint;
  /** Close in micro-USD */
  close: bigint;
}

/** Trade record as returned by backend keeper APIs (JSON-safe strings). */
export interface KeeperTradeRecordWire {
  tradeId: string;
  traderPubkey: string;
  direction: Direction;
  entryPrice: string;
  exitPrice: string;
  stakeUsdt: string;
  multiplierBps: number;
  windowStart: number;
  windowEnd: number;
  bandLower: string;
  bandUpper: string;
  maxPayoutUsdt: string;
}

/** OHLC tick as returned by backend keeper APIs (JSON-safe strings). */
export interface KeeperOhlcTickWire {
  ts: number;
  open: string;
  high: string;
  low: string;
  close: string;
}
