import type { KeeperOhlcTickWire, KeeperTradeRecordWire } from "./trade";

/** On-chain keeper stake account */
export interface KeeperStake {
  keeperPubkey: string;
  stakeAmount: bigint;
  depositedAt: number;
  lastSubmission: number;
  totalRewardsEarned: bigint;
  isActive: boolean;
}

/** Redis payload used for off-chain keeper notification. */
export interface BatchReadyRedisPayload {
  batchId: string;
  windowStart: number;
  windowEnd: number;
  numTrades: number;
  poolBalanceBefore: string;
}

export type BatchReadySignal = BatchReadyRedisPayload;

// Represents a single verified point in time from Pyth.
export interface PythCheckpoint {
  price: string | number;
  conf: string | number;
  exponent: number;
  publishTime: number;
}

// Flat public outputs committed into the settlement circuit hash.
export interface KeeperPublicOutputs {
  batchId: string;
  windowStart: number;
  windowEnd: number;
  numTrades: number;
  marketMaxMultiplier?: number;
  marketRegimeId: number;
  netPayoutUsdt: string;
  poolBalanceBefore: string;
  poolBalanceAfter: string;
  currentLiabilityBefore: string;
  keeperFee: string;
  protocolFee: string;
  numWinners: number;
  numLosers: number;
  totalWinnersPayout: string;
  totalLosersStake: string;
  payoutsCommitment: string;
  tradesCommitment: string;
  nullifierHash: string;
  pythCheckpointsHash: string;
  publicOutputsHash: string;
}

export interface KeeperBatchDataResponse {
  batchId: string;
  /** Present on keeper API batch payloads for routing / verification. */
  marketId?: string;
  windowStart: number;
  windowEnd: number;
  trades: KeeperTradeRecordWire[];
  tradeCommitments: string[];
  sortedTradeIds: string[];
  ohlc: KeeperOhlcTickWire[];
  poolBalanceBefore: string;
  currentLiabilityBefore: string;
  keeperFeeBps: number;
  protocolFeeBps: number;
  marketMaxMultiplier: number;
  marketRegimeId: number;
  pythFeedId: string;
  pythCheckpoints: PythCheckpoint[];
  pythCheckpointAccounts?: string[];
  expectedPublicOutputs?: KeeperPublicOutputs;
  computed: KeeperPublicOutputs;
}

export interface KeeperWinnerProof {
  tradeId: string; // hex-encoded 32-byte SHA-256 of orderId
  traderPubkey: string; // base58 Solana pubkey
  payoutIndex: number;
  stakeUsdt: string; // micro-USDT stake (trusted from keeper, aggregate ZK-proven)
  winningsUsdt: string; // gross payout: "0" for losses, stake*multiplier/10000 for wins
  proofNodes: string[]; // hex-encoded SHA-256 nodes
}

export interface KeeperWinnerProofsResponse {
  batchId: string;
  winners: KeeperWinnerProof[];
}

export interface ConfirmBatchRequest {
  txHash: string;
  confirmedAt: number;
}

export interface ConfirmBatchResponse {
  success: boolean;
  batchId: string;
}

export type BatchDataResponse = KeeperBatchDataResponse;

export type KeeperJobType = "carnot";
