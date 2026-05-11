export interface CarnotDeposit {
  account: string;
  amount: string;
}

export interface CarnotWithdrawal {
  account: string;
  amount: string;
}

export type CarnotOutcome = "WIN" | "LOSS";

export interface CarnotTradeResult {
  account: string;
  betId: string;
  outcome: CarnotOutcome;
  payout: string;
  originalStake: string;
}

export interface CarnotBatch {
  batchId: string;
  windowStart: number;
  windowEnd: number;
  deposits: CarnotDeposit[];
  withdrawals: CarnotWithdrawal[];
  trades: CarnotTradeResult[];
}

export interface CarnotBatchesResponse {
  batches: CarnotBatch[];
}

export interface CarnotErrorResponse {
  error: string;
  message: string;
  retryable: boolean;
}

export interface CommitCarnotBatchRequest {
  txHash: string;
  merkleRoot: string;
  committedAt: number;
}

export interface CommitCarnotBatchResponse {
  success: boolean;
  batchId: string;
}

export interface CarnotCommittedBatch {
  batchId: string;
  txHash: string;
  merkleRoot: string;
  committedAt: number;
  createdAt: string;
}
