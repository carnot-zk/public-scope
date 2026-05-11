import "dotenv/config";
import { z } from "zod";
import {
  CARNOT_PYTH_USD_FEED_IDS,
  PROGRAM_IDS,
  getCarnotPythUsdFeedIdForDefaultMarket,
  getUsdtMint,
  normalizeSolanaNetwork,
  type SolanaNetwork,
} from "@carnot/sdk";

import type { KeeperMarketConfig } from "./types";

const schema = z.object({
  SOLANA_RPC_URL: z.string().url(),
  SOLANA_WS_URL: z.string().url(),
  SOLANA_COMMITMENT: z
    .enum(["processed", "confirmed", "finalized"])
    .default("confirmed"),
  NETWORK: z.enum(["devnet", "mainnet", "mainnet-beta"]).default("devnet"),
  KEEPER_KEYPAIR: z.string().min(32),
  CARNOT_API_URL: z.string().url(),
  CARNOT_INTERNAL_API_KEY: z.string().min(16),
  USDT_MINT: z.string().min(32).optional(),
  SP1_PROVER_BINARY: z.string().default("../prover/carnot"),
  SP1_PROVER: z.enum(["local", "network"]).default("local"),
  SP1_PRIVATE_KEY: z.string().optional(),
  MIN_PROOF_WAIT_MS: z.coerce.number().default(90_000),
  BATCH_WINDOW_SECS: z.coerce.number().default(900),
  BATCH_POLL_INTERVAL_MS: z.coerce.number().default(30_000),
  MARKETS_CONFIG: z
    .string()
    .default(`btcusdt:${CARNOT_PYTH_USD_FEED_IDS.BTC_USD}`),
});

/** Environment after Zod parse (USDT_MINT may be filled from network default). */
type KeeperConfig = z.infer<typeof schema> & { USDT_MINT: string };

function loadConfig(): KeeperConfig {
  const parsed = schema.safeParse(process.env);
  if (!parsed.success) {
    const issues = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`);
    console.error("Invalid environment variables:\n" + issues.join("\n"));
    process.exit(1);
  }
  const data = parsed.data;
  if (data.CARNOT_API_URL.startsWith("http://")) {
    console.warn(
      "[config] WARNING: CARNOT_API_URL uses plain HTTP — API key will be sent unencrypted. Use HTTPS in production.",
    );
  }
  const network = normalizeSolanaNetwork(data.NETWORK);
  return {
    ...data,
    USDT_MINT: data.USDT_MINT ?? getUsdtMint(network),
  };
}

export const config = loadConfig();

export const programIds =
  PROGRAM_IDS[normalizeSolanaNetwork(config.NETWORK) as SolanaNetwork];

function parseMarketsConfig(value: string): KeeperMarketConfig[] {
  const entries = value
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  if (entries.length === 0) {
    throw new Error("MARKETS_CONFIG must include at least one market entry");
  }
  return entries.map((entry) => {
    const segments = entry.split(":");
    const marketId = segments[0]?.trim();
    if (!marketId) {
      throw new Error(`Invalid MARKETS_CONFIG entry: ${entry}`);
    }
    let pythFeedIdHexRaw = segments.slice(1).join(":").trim();
    if (!pythFeedIdHexRaw) {
      const fromSdk = getCarnotPythUsdFeedIdForDefaultMarket(marketId);
      if (!fromSdk) {
        throw new Error(
          `MARKETS_CONFIG entry "${entry}" needs marketId:feedHex or a built-in marketId (btcusdt, solusdt)`,
        );
      }
      pythFeedIdHexRaw = fromSdk;
    }
    const normalizedHex = pythFeedIdHexRaw.startsWith("0x")
      ? pythFeedIdHexRaw.slice(2)
      : pythFeedIdHexRaw;
    if (!/^[0-9a-fA-F]{64}$/.test(normalizedHex)) {
      throw new Error(`Invalid 32-byte feed hex for market ${marketId}`);
    }
    return {
      marketId,
      pythFeedIdHex: normalizedHex.toLowerCase(),
    };
  });
}

export const markets = parseMarketsConfig(config.MARKETS_CONFIG);
