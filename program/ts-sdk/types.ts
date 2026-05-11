import { PublicKey } from "@solana/web3.js";

export enum VolRegime {
  Low = "low",
  Medium = "medium",
  High = "high",
  Extreme = "extreme",
}

export interface GlobalState {
  admin: PublicKey;
  keeper: PublicKey;
  protocolFeeBps: number;
  paused: boolean;
  lastBatchAt: bigint;
  minBatchIntervalSecs: bigint;
  minRegimeUpdateIntervalSecs: bigint;
  bump: number;
}

export interface VaultState {
  totalLpShares: bigint;
  lpTotalDeposited: bigint;
  traderTotalMargin: bigint;
  currentLiability: bigint;
  solvencyRatioBps: number;
  accruedProtocolFees: bigint;
  bump: number;
}

export interface MarketState {
  marketId: number[];
  pythFeedId: number[];
  regimeId: bigint;
  fortressSpreadBps: bigint;
  maxMultiplier: bigint;
  volRegime: VolRegime;
  lastUpdated: bigint;
  bump: number;
}

/** On-chain TraderAccount fields (micro-USDT, 6 decimals).
 *  balanceUsdt    = deposited − settled_losses + settled_winnings − withdrawn
 *  lockedUsdt     = sum of open-trade stakes; decremented on settlement
 *  withdrawable   = balanceUsdt − lockedUsdt  (computed off-chain)
 */
export interface TraderAccount {
  owner: PublicKey;
  balanceUsdt: bigint;
  lockedUsdt: bigint;
  bump: number;
}

export interface LpPosition {
  owner: PublicKey;
  lpShares: bigint;
  lastDepositAt: bigint;
  bump: number;
}

export interface BatchReceiptAccount {
  batchId: number[];
  settledAt: bigint;
  windowStart: bigint;
  windowEnd: bigint;
  numTrades: number;
  numWinners: number;
  numLosers: number;
  netPayoutUsdt: bigint;
  poolBalanceBefore: bigint;
  poolBalanceAfter: bigint;
  currentLiabilityBefore: bigint;
  totalWinnersPayout: bigint;
  totalLosersStake: bigint;
  keeperFee: bigint;
  protocolFee: bigint;
  pythCheckpointsHash: number[];
  payoutsCommitment: number[];
  tradesCommitment: number[];
  nullifierHash: number[];
  marketRegimeId: bigint;
  keeperPubkey: PublicKey;
  bump: number;
}

export interface NullifierAccount {
  nullifierHash: number[];
  bump: number;
}

export interface ClaimReceipt {
  bump: number;
}
