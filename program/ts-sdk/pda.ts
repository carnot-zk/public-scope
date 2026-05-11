import { PublicKey } from "@solana/web3.js";
import {
  PROGRAM_IDS,
  findCarnotBatchReceiptPda,
  findCarnotClaimReceiptPda,
  findCarnotGlobalStatePda,
  findCarnotLpPositionPda,
  findCarnotMarketStatePda,
  findCarnotNullifierPda,
  findCarnotTraderAccountPda,
  findCarnotTraderVaultTokenPda,
  findCarnotVaultStatePda,
} from "@carnot/sdk";

/** Canonical engine program id (from `PROGRAM_IDS`; same value on devnet/mainnet today). */
export const CARNOT_ENGINE_PROGRAM_ID = new PublicKey(
  PROGRAM_IDS.devnet.carnot,
);

export {
  findCarnotBatchReceiptPda,
  findCarnotClaimReceiptPda,
  findCarnotGlobalStatePda,
  findCarnotLpPositionPda,
  findCarnotMarketStatePda,
  findCarnotNullifierPda,
  findCarnotTraderAccountPda,
  findCarnotTraderVaultTokenPda,
  findCarnotVaultStatePda,
} from "@carnot/sdk";

/** Legacy alias; prefer `findCarnotGlobalStatePda`. */
export function findGlobalState(
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotGlobalStatePda(programId);
}

/** Legacy alias; prefer `findCarnotVaultStatePda`. */
export function findVaultState(
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotVaultStatePda(programId);
}

/** Legacy alias; prefer `findCarnotTraderVaultTokenPda`. */
export function findTraderVaultToken(
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotTraderVaultTokenPda(programId);
}

/** Legacy alias; prefer `findCarnotMarketStatePda`. */
export function findMarketState(
  marketId: Buffer | Uint8Array,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotMarketStatePda(Buffer.from(marketId), programId);
}

/** Legacy alias; prefer `findCarnotTraderAccountPda`. */
export function findTraderAccount(
  trader: PublicKey,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotTraderAccountPda(trader, programId);
}

/** Legacy alias; prefer `findCarnotLpPositionPda`. */
export function findLpPosition(
  lp: PublicKey,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotLpPositionPda(lp, programId);
}

/** Legacy alias; prefer `findCarnotNullifierPda`. */
export function findNullifierAccount(
  nullifierHash: Buffer | Uint8Array,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotNullifierPda(Buffer.from(nullifierHash), programId);
}

/** Legacy alias; prefer `findCarnotBatchReceiptPda`. */
export function findBatchReceipt(
  batchId: Buffer | Uint8Array,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotBatchReceiptPda(Buffer.from(batchId), programId);
}

/** Legacy alias; prefer `findCarnotClaimReceiptPda`. */
export function findClaimReceipt(
  batchId: Buffer | Uint8Array,
  tradeId: Buffer | Uint8Array,
  programId: PublicKey = CARNOT_ENGINE_PROGRAM_ID,
): [PublicKey, number] {
  return findCarnotClaimReceiptPda(
    Buffer.from(batchId),
    Buffer.from(tradeId),
    programId,
  );
}
