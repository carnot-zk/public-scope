import { PublicKey } from "@solana/web3.js";
import {
  CARNOT_SEED_BATCH,
  CARNOT_SEED_CLAIM_RECEIPT,
  CARNOT_SEED_GLOBAL,
  CARNOT_SEED_LP_POSITION,
  CARNOT_SEED_MARKET,
  CARNOT_SEED_NULLIFIER,
  CARNOT_SEED_TRADER_ACCOUNT,
  CARNOT_SEED_TRADER_VAULT_TOKEN,
  CARNOT_SEED_VAULT,
} from "../constants/seeds";

export function findCarnotGlobalStatePda(
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_GLOBAL)],
    programId,
  );
}

export function findCarnotVaultStatePda(
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_VAULT)],
    programId,
  );
}

/** SPL token vault PDA for pooled trader margin (seed: `trader_vault_token`). */
export function findCarnotTraderVaultTokenPda(
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_TRADER_VAULT_TOKEN)],
    programId,
  );
}

export function findCarnotMarketStatePda(
  marketId: Uint8Array,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_MARKET), Buffer.from(marketId)],
    programId,
  );
}

export function findCarnotBatchReceiptPda(
  batchId: Buffer,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_BATCH), batchId],
    programId,
  );
}

export function findCarnotNullifierPda(
  nullifierHash: Buffer,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_NULLIFIER), nullifierHash],
    programId,
  );
}

export function findCarnotLpPositionPda(
  lpPubkey: PublicKey,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_LP_POSITION), lpPubkey.toBuffer()],
    programId,
  );
}

export function findCarnotTraderAccountPda(
  traderPubkey: PublicKey,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_TRADER_ACCOUNT), traderPubkey.toBuffer()],
    programId,
  );
}

export function findCarnotClaimReceiptPda(
  batchId: Buffer,
  tradeId: Buffer,
  programId: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(CARNOT_SEED_CLAIM_RECEIPT), batchId, tradeId],
    programId,
  );
}
