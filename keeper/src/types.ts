import type { Groth16Proof, KeeperPublicOutputs } from "@carnot/sdk";

export type { KeeperPublicOutputs } from "@carnot/sdk";

/** Parsed `MARKETS_CONFIG` entry — symbol plus 32-byte Pyth feed id (hex, no `0x`). */
export interface KeeperMarketConfig {
  marketId: string;
  pythFeedIdHex: string;
}

export interface BatchTriggerSignal {
  batchId: string;
  marketId: string;
  windowStart?: number;
  windowEnd?: number;
}

/** Groth16 + public fields for `verify_and_settle`. */
export interface ProofResult {
  proof: Groth16Proof;
  proofNonce: Uint8Array;
  publicOutputs: KeeperPublicOutputs;
  pythCheckpointAccounts: string[];
}

/** `carnot --prove --out-json` Groth16 settlement payload. */
export interface SettlementProverJsonOutput {
  proof_system: string;
  suitable_for_onchain: boolean;
  proof_a?: string;
  proof_b?: string;
  proof_c?: string;
  proof_nonce?: string;
  groth16_public_inputs?: string[];
  public_outputs_hash: string;
  batch_id: string;
  window_start: number;
  window_end: number;
  pyth_checkpoints_hash: string;
  net_payout_usdt: number;
  pool_balance_before: number;
  pool_balance_after: number;
  num_trades: number;
  nullifier_hash: string;
  keeper_fee: number;
  current_liability_before: number;
  protocol_fee: number;
  num_winners: number;
  num_losers: number;
  total_winners_payout: number;
  total_losers_stake: number;
  market_regime_id: number;
  payouts_commitment: string;
  trades_commitment: string;
  pyth_checkpoint_accounts: string[];
}
