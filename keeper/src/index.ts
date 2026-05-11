import { Connection, Keypair } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import bs58 from "bs58";
import { carnotIdl } from "@carnot/sdk";
import { config, programIds } from "./config";
import { describeError, isBatchTooSoonLikeMessage } from "./helpers";
import { BatchWatcher } from "./watcher";
import { runSettlementJob } from "./jobs/settlement";
import type { BatchTriggerSignal } from "./types";

const SETTLED_TTL_MS = 48 * 60 * 60 * 1000; // 48 hours

const settled = new Map<string, number>(); // batchId → timestamp when settled

function markSettled(batchId: string): void {
  settled.set(batchId, Date.now());
  // Evict entries older than TTL on each insertion to bound memory usage.
  const cutoff = Date.now() - SETTLED_TTL_MS;
  for (const [id, ts] of settled) {
    if (ts < cutoff) settled.delete(id);
  }
}

function isSettled(batchId: string): boolean {
  const ts = settled.get(batchId);
  return ts !== undefined && Date.now() - ts < SETTLED_TTL_MS;
}

async function main() {
  console.log(`Carnot Keeper starting on ${config.NETWORK}...`);

  const connection = new Connection(config.SOLANA_RPC_URL, {
    wsEndpoint: config.SOLANA_WS_URL,
    commitment: config.SOLANA_COMMITMENT,
  });

  const keeperKeypair = Keypair.fromSecretKey(
    bs58.decode(config.KEEPER_KEYPAIR),
  );
  const wallet = new anchor.Wallet(keeperKeypair);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: config.SOLANA_COMMITMENT,
  });

  const idlWithAddress = {
    ...carnotIdl,
    address: programIds.carnot,
  } as anchor.Idl;
  const carnotProgram = new anchor.Program(idlWithAddress, provider);

  // Per-batchId tracking; allows concurrent settlement of different markets.
  const inProgress = new Set<string>();

  const watcher = new BatchWatcher(connection, carnotProgram);
  const unsubscribeBatches = watcher.watchForBatches(
    async (signal: BatchTriggerSignal) => {
      if (inProgress.has(signal.batchId) || isSettled(signal.batchId)) return;
      inProgress.add(signal.batchId);
      try {
        const result = await runSettlementJob(
          signal,
          carnotProgram,
          keeperKeypair,
        );
        if (result === "settled") {
          markSettled(signal.batchId);
        }
      } catch (err) {
        const msg = describeError(err);
        if (isBatchTooSoonLikeMessage(msg)) {
          console.warn(
            `[keeper] Cooldown active while settling ${signal.batchId}; skipping and retrying later`,
          );
          return;
        }
        console.error(`[keeper] Settlement failed: ${msg}`);
        console.error("[keeper] Stopping — fix the issue and restart.");
        process.exit(1);
      } finally {
        inProgress.delete(signal.batchId);
      }
    },
  );

  const unsubscribeSettled = watcher.watchSettledBatches((batchId) => {
    markSettled(batchId);
    inProgress.delete(batchId);
  });

  console.log(
    `Keeper active. Pubkey: ${keeperKeypair.publicKey.toBase58()}. Polling every ${config.BATCH_POLL_INTERVAL_MS}ms...`,
  );

  process.on("SIGINT", () => {
    console.log("Shutting down keeper...");
    unsubscribeBatches();
    unsubscribeSettled();
    process.exit(0);
  });
}

main().catch((err) => {
  console.error("Keeper crashed:", err);
  process.exit(1);
});
