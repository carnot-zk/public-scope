/** Spawns `carnot --prove`: reads keeper-written batch JSON, writes settlement Groth16 output. */

import { spawn } from "child_process";
import fs from "fs";
import os from "os";
import path from "path";
import type { BatchDataResponse } from "@carnot/sdk";
import { hexToBytes, N_PYTH_CHECKPOINTS } from "@carnot/sdk";
import { config } from "./config";
import { describeError } from "./helpers";
import type {
  KeeperPublicOutputs,
  ProofResult,
  SettlementProverJsonOutput,
} from "./types";

function getProverEnv(): NodeJS.ProcessEnv {
  const merged: NodeJS.ProcessEnv = { ...process.env };

  // SP1 SDK expects "cpu" for local proving; keeper config uses enum "local".
  const proverMode =
    config.SP1_PROVER === "local" ? "cpu" : config.SP1_PROVER;
  merged.SP1_PROVER = proverMode;
  if (proverMode === "network") {
    if (!merged.NETWORK_PRIVATE_KEY && config.SP1_PRIVATE_KEY) {
      merged.NETWORK_PRIVATE_KEY = config.SP1_PRIVATE_KEY;
    }
  } else {
    // Local prover mode must not export network auth variables.
    delete merged.NETWORK_PRIVATE_KEY;
  }
  merged.RUST_LOG = merged.RUST_LOG ?? "sp1_sdk=debug,sp1_prover=debug";
  merged.RUST_BACKTRACE = "1";
  console.log(
    `[prover-env] SP1_PROVER=${merged.SP1_PROVER} NETWORK_PRIVATE_KEY=${merged.NETWORK_PRIVATE_KEY ? "***set***" : "MISSING"}`,
  );
  return merged;
}

function getBinaryPath(): string {
  const bin = config.SP1_PROVER_BINARY;
  return path.isAbsolute(bin) ? bin : path.resolve(process.cwd(), bin);
}

function parseProofResult(json: SettlementProverJsonOutput): ProofResult {
  if (!json.suitable_for_onchain) {
    throw new Error(
      `Proof not suitable for on-chain submission (proof_system=${json.proof_system}). ` +
        "Need Groth16 — ensure SP1_PROVER=network and NETWORK_PRIVATE_KEY are set.",
    );
  }
  if (!json.proof_a || !json.proof_b || !json.proof_c || !json.proof_nonce) {
    throw new Error(
      "Settlement JSON missing Groth16 proof fields (proof_a/b/c/nonce)",
    );
  }
  if (
    !json.pyth_checkpoint_accounts ||
    json.pyth_checkpoint_accounts.length !== N_PYTH_CHECKPOINTS
  ) {
    throw new Error(
      `Expected exactly ${N_PYTH_CHECKPOINTS} pyth_checkpoint_accounts in proof output, got ${
        json.pyth_checkpoint_accounts?.length ?? 0
      }. Ensure --data-json has pythCheckpointAccounts.`,
    );
  }

  // Groth16 limbs: G1 a/c 64B, G2 b 128B → nested Uint8Array layout for verify_and_settle.
  const aBytes = hexToBytes(json.proof_a);
  const bBytes = hexToBytes(json.proof_b);
  const cBytes = hexToBytes(json.proof_c);
  const nonceBytes = hexToBytes(json.proof_nonce);

  const publicOutputs: KeeperPublicOutputs = {
    publicOutputsHash: json.public_outputs_hash,
    batchId: json.batch_id,
    windowStart: json.window_start,
    windowEnd: json.window_end,
    pythCheckpointsHash: json.pyth_checkpoints_hash,
    netPayoutUsdt: String(json.net_payout_usdt),
    poolBalanceBefore: String(json.pool_balance_before),
    poolBalanceAfter: String(json.pool_balance_after),
    numTrades: json.num_trades,
    nullifierHash: json.nullifier_hash,
    keeperFee: String(json.keeper_fee),
    currentLiabilityBefore: String(json.current_liability_before),
    protocolFee: String(json.protocol_fee),
    numWinners: json.num_winners,
    numLosers: json.num_losers,
    totalWinnersPayout: String(json.total_winners_payout),
    totalLosersStake: String(json.total_losers_stake),
    marketRegimeId: json.market_regime_id,
    payoutsCommitment: json.payouts_commitment,
    tradesCommitment: json.trades_commitment,
  };

  return {
    proof: {
      a: [aBytes.slice(0, 32), aBytes.slice(32, 64)] as [
        Uint8Array,
        Uint8Array,
      ],
      b: [
        [bBytes.slice(0, 32), bBytes.slice(32, 64)] as [Uint8Array, Uint8Array],
        [bBytes.slice(64, 96), bBytes.slice(96, 128)] as [
          Uint8Array,
          Uint8Array,
        ],
      ],
      c: [cBytes.slice(0, 32), cBytes.slice(32, 64)] as [
        Uint8Array,
        Uint8Array,
      ],
    },
    proofNonce: nonceBytes,
    publicOutputs,
    pythCheckpointAccounts: json.pyth_checkpoint_accounts,
  };
}

export async function generateSettlementProof(
  batchData: BatchDataResponse,
): Promise<ProofResult> {
  const ts = Date.now();
  const inputPath = path.join(os.tmpdir(), `carnot-input-${ts}.json`);
  const outputPath = path.join(os.tmpdir(), `carnot-settlement-${ts}.json`);

  fs.writeFileSync(inputPath, JSON.stringify(batchData, null, 2), "utf8");
  console.log(`[prover] Wrote batch data → ${inputPath}`);

  const binaryPath = getBinaryPath();
  const args = ["--prove", "--data-json", inputPath, "--out-json", outputPath];
  console.log(`[prover] Spawning: ${binaryPath} ${args.join(" ")}`);

  const timeoutMs = Math.max(config.MIN_PROOF_WAIT_MS, 120_000); // floor: local proving often needs ≥2m

  let raw: string;
  try {
    await new Promise<void>((resolve, reject) => {
      const child = spawn(binaryPath, args, {
        stdio: ["ignore", "pipe", "pipe"],
        env: getProverEnv(),
      });

      let stderrBuf = "";
      child.stdout.on("data", (chunk: Buffer) =>
        process.stdout.write(`[carnot] ${chunk.toString()}`),
      );
      child.stderr.on("data", (chunk: Buffer) => {
        const text = chunk.toString();
        stderrBuf = (stderrBuf + text).slice(-4000);
        process.stderr.write(`[carnot] ${text}`);
      });

      const timer = setTimeout(() => {
        child.kill("SIGTERM");
        reject(new Error(`carnot binary timed out after ${timeoutMs}ms`));
      }, timeoutMs);

      child.on("error", (err: unknown) => {
        clearTimeout(timer);
        reject(
          new Error(
            `Failed to spawn carnot binary (${binaryPath}): ${describeError(err)}`,
          ),
        );
      });

      child.on("exit", (code: number | null, signal: NodeJS.Signals | null) => {
        clearTimeout(timer);
        if (code === 0) {
          resolve();
        } else {
          const why = signal
            ? `signal ${signal}`
            : `exit code ${String(code ?? "unknown")}`;
          const detail = stderrBuf ? `\nLast stderr:\n${stderrBuf}` : "";
          reject(new Error(`carnot binary exited with ${why}${detail}`));
        }
      });
    });

    if (!fs.existsSync(outputPath)) {
      throw new Error(
        `carnot binary exited 0 but did not write output JSON: ${outputPath}`,
      );
    }

    raw = fs.readFileSync(outputPath, "utf8");
    console.log(`[prover] Read settlement JSON ← ${outputPath}`);
  } finally {
    fs.rmSync(inputPath, { force: true });
    fs.rmSync(outputPath, { force: true });
  }

  const json = JSON.parse(raw) as SettlementProverJsonOutput;
  return parseProofResult(json);
}
