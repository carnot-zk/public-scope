import * as anchor from '@coral-xyz/anchor';
import { PublicKey, Keypair } from '@solana/web3.js';
import {
  findCarnotBatchReceiptPda,
  findCarnotNullifierPda,
  findCarnotGlobalStatePda,
  findCarnotMarketStatePda,
  findCarnotVaultStatePda,
  findCarnotTraderAccountPda,
  findCarnotTraderVaultTokenPda,
  findCarnotClaimReceiptPda,
  hexToBuffer,
  N_PYTH_CHECKPOINTS,
} from '@carnot/sdk';
import type { KeeperWinnerProof } from '@carnot/sdk';
import type { ProofResult } from "./types";
import { config, programIds } from './config';
import { describeError } from './helpers';

const CARNOT_PROGRAM_ID = new PublicKey(programIds.carnot);
const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL',
);
const TRADER_VAULT_TOKEN_SEED = Buffer.from('trader_vault_token');

/** verify_and_settle; null tx if another keeper settled first. */
export async function submitSettlement(
  program: anchor.Program,
  keeper: Keypair,
  marketIdHex: string,
  batchId: Buffer,
  result: ProofResult,
): Promise<string | null> {
  const marketIdBytes = hexToBuffer(marketIdHex);
  if (marketIdBytes.length !== 32) {
    throw new Error(`Expected 32-byte market id, got ${marketIdBytes.length} bytes`);
  }
  const pub = result.publicOutputs;
  const nullifierHash = hexToBuffer(pub.nullifierHash);
  const payoutsCommitment = hexToBuffer(pub.payoutsCommitment);
  const tradesCommitment = hexToBuffer(pub.tradesCommitment);
  const pythCheckpointsHash = hexToBuffer(pub.pythCheckpointsHash);
  const publicOutputsHash = hexToBuffer(pub.publicOutputsHash);
  const usdtMint = new PublicKey(config.USDT_MINT);

  const [batchPda] = findCarnotBatchReceiptPda(batchId, CARNOT_PROGRAM_ID);
  const [nullifierPda] = findCarnotNullifierPda(nullifierHash, CARNOT_PROGRAM_ID);
  const [globalStatePda] = findCarnotGlobalStatePda(CARNOT_PROGRAM_ID);
  const [vaultStatePda] = findCarnotVaultStatePda(CARNOT_PROGRAM_ID);
  const [marketStatePda] = findCarnotMarketStatePda(marketIdBytes, CARNOT_PROGRAM_ID);
  const [traderVaultTokenPda] = findCarnotTraderVaultTokenPda(CARNOT_PROGRAM_ID);
  const lpVaultTokenAccount = findAssociatedTokenAddress(vaultStatePda, usdtMint);
  const keeperTokenAccount = findAssociatedTokenAddress(keeper.publicKey, usdtMint);

  const proofAFlat = flattenG1(result.proof.a);
  const proofBFlat = flattenG2(result.proof.b);
  const proofCFlat = flattenG1(result.proof.c);
  if (result.pythCheckpointAccounts.length !== N_PYTH_CHECKPOINTS) {
    throw new Error(
      `verify_and_settle requires ${N_PYTH_CHECKPOINTS} Pyth checkpoint accounts, got ${result.pythCheckpointAccounts.length}`,
    );
  }

  try {
    const tx = await program.methods
      .verifyAndSettle(
        Array.from(marketIdBytes),
        Array.from(batchId),
        Array.from(proofAFlat),
        Array.from(proofBFlat),
        Array.from(proofCFlat),
        Array.from(result.proofNonce),
        Array.from(publicOutputsHash),
        new anchor.BN(pub.windowStart),
        new anchor.BN(pub.windowEnd),
        Array.from(pythCheckpointsHash),
        new anchor.BN(pub.netPayoutUsdt),
        new anchor.BN(pub.poolBalanceBefore),
        new anchor.BN(pub.poolBalanceAfter),
        pub.numTrades,
        Array.from(nullifierHash),
        new anchor.BN(pub.keeperFee),
        new anchor.BN(pub.currentLiabilityBefore),
        new anchor.BN(pub.protocolFee),
        pub.numWinners,
        pub.numLosers,
        new anchor.BN(pub.totalWinnersPayout),
        new anchor.BN(pub.totalLosersStake),
        new anchor.BN(pub.marketRegimeId),
        Array.from(payoutsCommitment),
        Array.from(tradesCommitment),
      )
      .accounts({
        keeper: keeper.publicKey,
        globalState: globalStatePda,
        vaultState: vaultStatePda,
        marketState: marketStatePda,
        nullifierAccount: nullifierPda,
        batchReceiptAccount: batchPda,
        lpVaultTokenAccount: lpVaultTokenAccount,
        traderVaultTokenAccount: traderVaultTokenPda,
        keeperTokenAccount: keeperTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .remainingAccounts(
        result.pythCheckpointAccounts.map((account) => ({
          pubkey: new PublicKey(account),
          isWritable: false,
          isSigner: false,
        })),
      )
      .signers([keeper])
      .rpc({ commitment: 'confirmed' });

    console.log(`Settlement submitted: ${tx}`);
    return tx;
  } catch (e: unknown) {
    const msg = describeError(e);
    if (msg.includes('AlreadySettled') || msg.includes('already in use')) {
      console.log('Lost race — batch already settled by another keeper');
      return null;
    }
    throw e;
  }
}

/** Per-trade balance credit after settle; claim_receipt makes retries idempotent. */
export async function distributeWinnings(
  program: anchor.Program,
  keeper: Keypair,
  batchId: Buffer,
  proof: KeeperWinnerProof,
): Promise<string> {
  const traderPubkey = parseTraderPublicKey(proof.traderPubkey);
  const tradeIdBytes = hexStringToBuffer(proof.tradeId, 'tradeId');

  const [globalStatePda] = findCarnotGlobalStatePda(CARNOT_PROGRAM_ID);
  const [batchPda] = findCarnotBatchReceiptPda(batchId, CARNOT_PROGRAM_ID);
  const [traderAccountPda] = findCarnotTraderAccountPda(traderPubkey, CARNOT_PROGRAM_ID);
  const [claimReceiptPda] = findCarnotClaimReceiptPda(batchId, tradeIdBytes, CARNOT_PROGRAM_ID);

  const tx = await program.methods
    .distributeWinnings(
      Array.from(batchId),
      Array.from(tradeIdBytes),
      new anchor.BN(proof.stakeUsdt),
      proof.payoutIndex,
      new anchor.BN(proof.winningsUsdt),
      proof.proofNodes.map((n: string) => Array.from(hexStringToBuffer(n, 'proofNode'))),
    )
    .accounts({
      claimer: keeper.publicKey,
      trader: traderPubkey,
      globalState: globalStatePda,
      batchReceiptAccount: batchPda,
      claimReceipt: claimReceiptPda,
      traderAccount: traderAccountPda,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .signers([keeper])
    .rpc({ commitment: 'confirmed' });

  return tx;
}

function parseTraderPublicKey(value: string): PublicKey {
  try {
    return new PublicKey(value);
  } catch {
    const normalized = stripHexPrefix(value);
    if (!/^[0-9a-fA-F]+$/.test(normalized)) {
      throw new Error(`Invalid traderPubkey format: ${value}`);
    }
    const bytes = Buffer.from(normalized, 'hex');
    if (bytes.length !== 32) {
      throw new Error(
        `Unsupported traderPubkey byte length ${bytes.length}; expected 32-byte Solana pubkey`,
      );
    }
    // Backend may send hex-encoded 32-byte pubkey for loser proofs.
    return new PublicKey(bytes);
  }
}

function hexStringToBuffer(value: string, label: string): Buffer {
  const normalized = stripHexPrefix(value);
  if (!/^[0-9a-fA-F]+$/.test(normalized)) {
    throw new Error(`Invalid ${label}: expected hex string`);
  }
  if (normalized.length % 2 !== 0) {
    throw new Error(`Invalid ${label}: odd-length hex string`);
  }
  return Buffer.from(normalized, 'hex');
}

function stripHexPrefix(value: string): string {
  return value.startsWith('0x') || value.startsWith('0X') ? value.slice(2) : value;
}

function findAssociatedTokenAddress(owner: PublicKey, mint: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID,
  )[0];
}


function flattenG1(g1: [Uint8Array, Uint8Array]): Uint8Array {
  const out = new Uint8Array(64);
  out.set(g1[0], 0);
  out.set(g1[1], 32);
  return out;
}

function flattenG2(
  g2: [[Uint8Array, Uint8Array], [Uint8Array, Uint8Array]],
): Uint8Array {
  const out = new Uint8Array(128);
  out.set(g2[0][0], 0);
  out.set(g2[0][1], 32);
  out.set(g2[1][0], 64);
  out.set(g2[1][1], 96);
  return out;
}
