import { Connection, type Logs } from "@solana/web3.js";
import { Program } from "@coral-xyz/anchor";
import { computeKeeperBatchId } from "@carnot/sdk";
import { config, markets } from "./config";
import type { BatchTriggerSignal } from "./types";

export class BatchWatcher {
  private connection: Connection;
  private carnotProgram: Program;
  private pollTimer?: NodeJS.Timeout;
  private readonly emittedAt = new Map<string, number>();
  private isPolling = false;

  constructor(connection: Connection, carnotProgram: Program) {
    this.connection = connection;
    this.carnotProgram = carnotProgram;
  }

  /** Poll the current batch window (no historical catch-up on cold start). */
  watchForBatches(onBatch: (signal: BatchTriggerSignal) => void): () => void {
    const emitCandidates = async () => {
      if (this.isPolling) return;
      this.isPolling = true;
      try {
        const now = Math.floor(Date.now() / 1000);
        const batch = config.BATCH_WINDOW_SECS;
        // Closed window end = floor(now/batch)*batch (not ceil: ceil skips the batch that just closed).
        // First window uses [0, batch) until now ≥ batch.
        const closedWindowEnd = Math.floor(now / batch) * batch;
        const windowEnd = closedWindowEnd === 0 ? batch : closedWindowEnd;
        const windowStart = windowEnd - batch;

        for (const market of markets) {
          const candidate: BatchTriggerSignal = {
            batchId: computeKeeperBatchId(
              market.marketId,
              windowStart,
              windowEnd,
            ),
            marketId: market.marketId,
            windowStart,
            windowEnd,
          };
          const previousEmit = this.emittedAt.get(candidate.batchId) ?? 0;
          // At most once per 2× poll interval — batch data may land late.
          if (Date.now() - previousEmit < config.BATCH_POLL_INTERVAL_MS * 2) {
            continue;
          }
          this.emittedAt.set(candidate.batchId, Date.now());
          onBatch(candidate);
        }
      } finally {
        this.isPolling = false;
      }
    };

    void emitCandidates();
    this.pollTimer = setInterval(() => {
      void emitCandidates();
    }, config.BATCH_POLL_INTERVAL_MS);
    return () => {
      if (this.pollTimer) {
        clearInterval(this.pollTimer);
      }
    };
  }

  /** Log subscription: detect on-chain settle (lost keeper race). */
  watchSettledBatches(onSettled: (batchId: string) => void): () => void {
    const subscriptionId = this.connection.onLogs(
      this.carnotProgram.programId,
      (logs: Logs) => {
        for (const log of logs.logs) {
          if (log.includes("SettlementBatchCommitted")) {
            const match = log.match(/batch_id: ([0-9a-f]{64})/);
            if (match) {
              onSettled(match[1]);
            }
          }
        }
      },
      config.SOLANA_COMMITMENT,
    );
    return () => {
      this.connection.removeOnLogsListener(subscriptionId);
    };
  }
}
