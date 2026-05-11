import type { AxiosResponse } from "axios";
import axios, { isAxiosError } from "axios";
import { BorshAccountsCoder } from "@coral-xyz/anchor";
import type * as anchor from "@coral-xyz/anchor";
import type { BN } from "@coral-xyz/anchor";
import { Keypair, PublicKey } from "@solana/web3.js";
import type {
  BatchDataResponse,
  ConfirmBatchRequest,
  KeeperWinnerProofsResponse,
} from "@carnot/sdk";
import { hexToBuffer } from "@carnot/sdk";
import { findCarnotGlobalStatePda } from "@carnot/sdk";
import { generateSettlementProof } from "../prover";
import { submitSettlement, distributeWinnings } from "../submitter";
import { config, markets } from "../config";
import { recordReward } from "../reward-tracker";
import type { BatchTriggerSignal } from "../types";
import {
  carnotInternalBearerHeaders,
  describeError,
  isBatchTooSoonLikeMessage,
} from "../helpers";

/** Fields read from on-chain GlobalState for batch cooldown (Borsh-decoded). */
interface CarnotGlobalStateCooldown {
  minBatchIntervalSecs: BN;
  lastBatchAt: BN;
}

async function fetchGlobalStateCooldown(
  program: anchor.Program,
  pda: PublicKey,
): Promise<CarnotGlobalStateCooldown> {
  const info = await program.provider.connection.getAccountInfo(pda);
  if (!info?.data.length) {
    throw new Error("Global state account missing or empty");
  }
  const coder = new BorshAccountsCoder(program.idl);
  return coder.decode<CarnotGlobalStateCooldown>("GlobalState", info.data);
}

/** Fetch batch data from the Carnot API, prove, verify_and_settle, then distribute per-trade winnings. */
export async function runSettlementJob(
  signal: BatchTriggerSignal,
  program: anchor.Program,
  keeper: Keypair,
): Promise<"settled" | "not_ready" | "cooldown"> {
  const marketConfig = markets.find(
    (market) => market.marketId === signal.marketId,
  );
  if (!marketConfig) {
    console.error(
      `[settlement] Unknown marketId in signal: ${signal.marketId}`,
    );
    return "not_ready";
  }

  const startMs = Date.now();
  const nowSecs = Math.floor(startMs / 1000);
  if (
    typeof signal.windowEnd === "number" &&
    Number.isFinite(signal.windowEnd) &&
    nowSecs < signal.windowEnd
  ) {
    const remaining = signal.windowEnd - nowSecs;
    console.log(
      `[settlement] Batch ${signal.batchId} window not closed yet; waiting ${remaining}s before proving`,
    );
    return "not_ready";
  }
  console.log(
    `[settlement] Starting batch ${signal.batchId} (${signal.marketId})`,
  );

  const [globalStatePda] = findCarnotGlobalStatePda(program.programId);
  const globalState = await fetchGlobalStateCooldown(program, globalStatePda);
  const chainNowSecs = BigInt(Math.floor(Date.now() / 1000));
  const minIntervalSecs = BigInt(globalState.minBatchIntervalSecs.toString());
  const lastBatchAtSecs = BigInt(globalState.lastBatchAt.toString());
  const elapsedSecs =
    chainNowSecs >= lastBatchAtSecs ? chainNowSecs - lastBatchAtSecs : 0n;
  if (elapsedSecs < minIntervalSecs) {
    const remaining = minIntervalSecs - elapsedSecs;
    console.log(
      `[settlement] Cooldown active for batch ${signal.batchId}; skipping prove for now (${remaining}s remaining)`,
    );
    return "cooldown";
  }

  let batchData: BatchDataResponse;
  try {
    const response = await axios.get<BatchDataResponse>(
      `${config.CARNOT_API_URL}/internal/batch/${signal.batchId}/data`,
      {
        params: {
          marketId: signal.marketId,
          windowStart: signal.windowStart,
          windowEnd: signal.windowEnd,
        },
        headers: carnotInternalBearerHeaders(),
        timeout: 10_000,
      },
    );
    batchData = response.data;
    if (
      batchData.marketId !== undefined &&
      batchData.marketId !== signal.marketId
    ) {
      console.error(
        `[settlement] Batch market mismatch: signal=${signal.marketId}, payload=${batchData.marketId}`,
      );
      return "not_ready";
    }
  } catch (err: unknown) {
    if (isAxiosError(err)) {
      if (err.response?.status === 404) return "not_ready";
      // 5xx: treat as transient; watcher will poll again.
      if ((err.response?.status ?? 0) >= 500) {
        console.warn(
          `[settlement] Backend error fetching batch ${signal.batchId}: ${err.message}`,
        );
        return "not_ready";
      }
    }
    const msg = describeError(err);
    console.error(`[settlement] Failed to fetch batch data: ${msg}`);
    throw err;
  }

  const batchWindowEnd = Number(batchData.windowEnd ?? signal.windowEnd ?? 0);
  if (
    Number.isFinite(batchWindowEnd) &&
    Math.floor(Date.now() / 1000) < batchWindowEnd
  ) {
    const remaining = batchWindowEnd - Math.floor(Date.now() / 1000);
    console.log(
      `[settlement] Batch ${signal.batchId} payload window not closed yet; waiting ${remaining}s before proving`,
    );
    return "not_ready";
  }

  console.log(
    `[settlement] Generating proof for ${batchData.trades.length} trades...`,
  );
  let proofResult;
  try {
    proofResult = await generateSettlementProof(batchData);
  } catch (err: unknown) {
    console.error(
      `[settlement] Proof generation failed: ${describeError(err)}`,
    );
    throw err;
  }
  console.log(`[settlement] Proof generated in ${Date.now() - startMs}ms`);

  const batchIdBuffer = hexToBuffer(signal.batchId);
  let txSig: string | null;
  try {
    txSig = await submitSettlement(
      program,
      keeper,
      marketConfig.pythFeedIdHex,
      batchIdBuffer,
      proofResult,
    );
  } catch (err: unknown) {
    const msg = describeError(err);
    if (isBatchTooSoonLikeMessage(msg)) {
      console.warn(
        `[settlement] On-chain cooldown hit after proving for ${signal.batchId}; will retry later without exiting`,
      );
      return "cooldown";
    }
    if (msg.includes("InvalidPoolBalance")) {
      console.warn(
        `[settlement] Vault state changed during proving for ${signal.batchId}; will retry`,
      );
      return "not_ready";
    }
    throw err;
  }

  if (txSig) {
    await confirmBatchOnBackend(signal.batchId, txSig);
    const fee = BigInt(proofResult.publicOutputs.keeperFee);
    await recordReward(signal.batchId, fee, txSig);
    console.log(
      `[settlement] Won batch ${signal.batchId}. Fee: ${fee} micro-USDT. Tx: ${txSig}`,
    );

    await distributeAllTrades(signal.batchId, batchIdBuffer, program, keeper);
  }
  return "settled";
}

/**
 * winner-proofs includes losses. Fetch errors propagate. Per-trade RPC failures
 * (except idempotent “already in use”) log and continue so other trades can run.
 */
async function distributeAllTrades(
  batchId: string,
  batchIdBuffer: Buffer,
  program: anchor.Program,
  keeper: Keypair,
): Promise<void> {
  let proofs: KeeperWinnerProofsResponse;
  try {
    proofs = (
      await axios.get<KeeperWinnerProofsResponse>(
        `${config.CARNOT_API_URL}/internal/batch/${batchId}/winner-proofs`,
        {
          headers: carnotInternalBearerHeaders(),
          timeout: 15_000,
        },
      )
    ).data;
  } catch (err: unknown) {
    console.error(
      `[distribution] Failed to fetch trade proofs for ${batchId}: ${describeError(err)}`,
    );
    return;
  }

  console.log(
    `[distribution] Distributing ${proofs.winners.length} trades for batch ${batchId}`,
  );
  let succeeded = 0;
  let skipped = 0;

  for (const proof of proofs.winners) {
    try {
      const tx = await distributeWinnings(
        program,
        keeper,
        batchIdBuffer,
        proof,
      );
      console.log(
        `[distribution] Trade ${proof.tradeId} (${proof.winningsUsdt === "0" ? "LOSS" : "WIN"}): tx=${tx}`,
      );
      succeeded++;
    } catch (err: unknown) {
      const msg = describeError(err);
      if (msg.includes("already in use") || msg.includes("AlreadyInUse")) {
        skipped++;
        continue;
      }
      console.error(`[distribution] Failed for trade ${proof.tradeId}: ${msg}`);
    }
  }

  console.log(
    `[distribution] Batch ${batchId} complete: ${succeeded} distributed, ${skipped} already done, ${proofs.winners.length - succeeded - skipped} failed`,
  );
}

async function confirmBatchOnBackend(
  batchId: string,
  txSig: string,
): Promise<void> {
  const body: ConfirmBatchRequest = {
    txHash: txSig,
    confirmedAt: Math.floor(Date.now() / 1000),
  };
  try {
    await axios.post<void, AxiosResponse<void>, ConfirmBatchRequest>(
      `${config.CARNOT_API_URL}/internal/batch/${batchId}/confirm`,
      body,
      {
        headers: carnotInternalBearerHeaders(),
        timeout: 10_000,
      },
    );
  } catch (err: unknown) {
    console.warn(
      `[settlement] Settlement submitted but backend confirm failed for ${batchId}: ${describeError(err)}`,
    );
  }
}
