import type { Groth16Proof } from "./zk";

/** Public inputs committed on-chain as part of the Carnot ZK proof. */
export interface CarnotPublicInputs {
  /** SHA-256 of (window_start_le || window_end_le || platform_id) */
  batchId: Uint8Array;
  windowStart: number;
  windowEnd: number;
  /** SHA-256 of the 3 Pyth checkpoints covering this window */
  pythCheckpointsHash: Uint8Array;
  /** Net payout from pool to winners, signed (negative = pool gains) in micro-USDT */
  netPayoutUsdt: bigint;
  poolBalanceBefore: bigint;
  poolBalanceAfter: bigint;
  numTrades: number;
  /** SHA-256 of (batch_id || sorted trade_ids) — prevents replay */
  nullifierHash: Uint8Array;
  /** Fee earned by the winning keeper in micro-USDT */
  keeperFee: bigint;
}

/** Full Carnot submission payload */
export interface CarnotSubmission {
  batchId: Uint8Array;
  proof: Groth16Proof;
  publicInputs: CarnotPublicInputs;
}

/** Canonical verify_and_settle arguments expected by Carnot. */
export interface CarnotVerifyAndSettleArgs {
  marketId: Uint8Array;
  batchId: Uint8Array;
  proofA: Uint8Array;
  proofB: Uint8Array;
  proofC: Uint8Array;
  proofNonce: Uint8Array;
  publicOutputsHash: Uint8Array;
  windowStart: number;
  windowEnd: number;
  pythCheckpointsHash: Uint8Array;
  netPayoutUsdt: bigint;
  poolBalanceBefore: bigint;
  poolBalanceAfter: bigint;
  numTrades: number;
  nullifierHash: Uint8Array;
  keeperFee: bigint;
  currentLiabilityBefore: bigint;
  protocolFee: bigint;
  numWinners: number;
  numLosers: number;
  totalWinnersPayout: bigint;
  totalLosersStake: bigint;
  marketRegimeId: bigint;
  payoutsCommitment: Uint8Array;
  tradesCommitment: Uint8Array;
}
