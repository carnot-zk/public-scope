import * as anchor from "@coral-xyz/anchor";
import BN from "bn.js";
import { Connection, PublicKey } from "@solana/web3.js";
import {
  findGlobalState,
  findVaultState,
  findMarketState,
  findTraderVaultToken,
} from "../ts-sdk/pda";
import {
  loadIdl,
  loadWallet,
  parseAnchorWalletPath,
  ataFor,
} from "./utils";
import type { CarnotEngine } from "../target/types/carnot_engine";

const CLUSTER_CHOICES = ["devnet", "mainnet", "mainnet-beta"] as const;
type Cluster = (typeof CLUSTER_CHOICES)[number];

function isCluster(value: string): value is Cluster {
  return (CLUSTER_CHOICES as readonly string[]).includes(value);
}

function inferClusterFromEnv(): Cluster {
  return process.env.ANCHOR_PROVIDER_URL?.includes("mainnet")
    ? "mainnet"
    : "devnet";
}

function parseClusterFlag(value: string | undefined): Cluster {
  if (value === undefined) {
    throw new Error("--cluster requires a value: devnet | mainnet | mainnet-beta");
  }
  if (!isCluster(value)) {
    throw new Error(
      `Invalid --cluster "${value}". Use devnet | mainnet | mainnet-beta.`,
    );
  }
  return value;
}

interface Args {
  authority: string;
  keeper: string;
  cluster: Cluster;
  /** Keypair JSON path; overrides ANCHOR_WALLET and Anchor.toml wallet */
  walletPath?: string;
  programId?: string;
  usdtMint?: string;
  marketIdHex?: string;
  protocolFeeBps?: number;
  fortressSpreadBps?: number;
  maxMultiplierBps?: number;
  skipMarketUpdate: boolean;
  skipAdminInit: boolean;
}

const DEFAULT_RPC: Record<Cluster, string> = {
  devnet: "https://api.devnet.solana.com",
  mainnet: "https://api.mainnet-beta.solana.com",
  "mainnet-beta": "https://api.mainnet-beta.solana.com",
};

const DEFAULT_MIN_BATCH_INTERVAL_SECS = 900;
const DEFAULT_MIN_REGIME_UPDATE_INTERVAL_SECS = 5;
const DEFAULT_REGIME_ID = 1;
const DEFAULT_PROTOCOL_FEE_BPS = 100;
const DEFAULT_FORTRESS_SPREAD_BPS = 150;
const DEFAULT_MAX_MULTIPLIER_BPS = 30_000;

function parseArgs(): Args {
  const raw = process.argv.slice(2);
  const out: Args = {
    authority: "",
    keeper: "",
    cluster: inferClusterFromEnv(),
    skipMarketUpdate: false,
    skipAdminInit: false,
  };

  for (let i = 0; i < raw.length; i++) {
    const k = raw[i];
    const v = raw[i + 1];
    if (k === "--authority") out.authority = v;
    else if (k === "--keeper") out.keeper = v;
    else if (k === "--cluster") out.cluster = parseClusterFlag(v);
    else if (k === "--wallet") out.walletPath = v;
    else if (k === "--program-id") out.programId = v;
    else if (k === "--usdt-mint") out.usdtMint = v;
    else if (k === "--market-id") out.marketIdHex = v;
    else if (k === "--protocol-fee-bps") out.protocolFeeBps = Number(v);
    else if (k === "--fortress-spread-bps") out.fortressSpreadBps = Number(v);
    else if (k === "--max-multiplier-bps") out.maxMultiplierBps = Number(v);
    else if (k === "--skip-market-update") out.skipMarketUpdate = true;
    else if (k === "--skip-admin-init") out.skipAdminInit = true;
  }

  if (!out.authority || !out.keeper) {
    throw new Error(
      "Usage: bun scripts/init-admin.ts --authority <PUBKEY> --keeper <PUBKEY>" +
        " [--wallet <KEYPAIR.json>] [--cluster devnet|mainnet] [--program-id <PUBKEY>]" +
        " [--usdt-mint <PUBKEY>] [--market-id <HEX>] [--protocol-fee-bps N]" +
        " [--fortress-spread-bps N] [--max-multiplier-bps N] [--skip-market-update]" +
        " [--skip-admin-init]",
    );
  }
  return out;
}

function hexToBytes(hex: string): number[] {
  const normalized = hex.startsWith("0x") ? hex.slice(2) : hex;
  const buf = Buffer.from(normalized, "hex");
  if (buf.length !== 32)
    throw new Error(`Expected 32-byte hex value, got ${buf.length} bytes`);
  return [...buf];
}

async function main() {
  const args = parseArgs();

  const walletPath =
    args.walletPath ??
    process.env.ANCHOR_WALLET ??
    parseAnchorWalletPath() ??
    "~/.config/solana/id.json";
  const wallet = loadWallet(walletPath);
  const rpcUrl = process.env.ANCHOR_PROVIDER_URL ?? DEFAULT_RPC[args.cluster];
  const provider = new anchor.AnchorProvider(
    new Connection(rpcUrl, "confirmed"),
    wallet,
    {
      commitment: "confirmed",
    },
  );
  anchor.setProvider(provider);

  const signer = provider.wallet.publicKey;
  const authority = new PublicKey(args.authority);
  const keeper = new PublicKey(args.keeper);

  if (!signer.equals(authority)) {
    throw new Error(
      `Signer ${signer.toBase58()} does not match --authority ${authority.toBase58()}. ` +
        `Set ANCHOR_WALLET to the authority keypair.`,
    );
  }

  const { idl, programId } = loadIdl(args.programId);
  const program = new anchor.Program<CarnotEngine>(idl, provider);

  const protocolFeeBps = args.protocolFeeBps ?? DEFAULT_PROTOCOL_FEE_BPS;
  const fortressSpreadBps =
    args.fortressSpreadBps ?? DEFAULT_FORTRESS_SPREAD_BPS;
  const maxMultiplierBps = args.maxMultiplierBps ?? DEFAULT_MAX_MULTIPLIER_BPS;

  if (!args.marketIdHex) {
    throw new Error(
      "--market-id <32-byte-hex> is required (e.g. the Pyth BTC/USD feed ID)",
    );
  }
  const marketIdBytes = hexToBytes(args.marketIdHex);
  if (!args.usdtMint) {
    throw new Error("--usdt-mint <PUBKEY> is required");
  }
  const usdtMint = new PublicKey(args.usdtMint);

  const [globalState] = findGlobalState(programId);
  const [vaultState] = findVaultState(programId);
  const lpVaultTokenAccount = ataFor(vaultState, usdtMint);
  const [traderVaultTokenAccount] = findTraderVaultToken(programId);
  const [marketState] = findMarketState(Buffer.from(marketIdBytes), programId);

  console.log(`\nInitializing Carnot Engine`);
  console.log(`  programId:  ${programId.toBase58()}`);
  console.log(`  authority:  ${authority.toBase58()}`);
  console.log(`  keeper:     ${keeper.toBase58()}`);
  console.log(`  cluster:    ${args.cluster}`);
  console.log(`  usdtMint:   ${usdtMint.toBase58()}`);
  console.log(`  marketId:   ${args.marketIdHex}`);

  if (!args.skipAdminInit) {
    const initSig = await program.methods
      .adminInit(
        keeper,
        protocolFeeBps,
        new BN(DEFAULT_MIN_BATCH_INTERVAL_SECS),
        new BN(DEFAULT_MIN_REGIME_UPDATE_INTERVAL_SECS),
      )
      .accounts({
        admin: authority,
        usdtMint,
      })
      .rpc();
    console.log(`\nadminInit tx:   ${initSig}`);
  } else {
    console.log("\nSkipping adminInit (--skip-admin-init).");
  }

  const initMarketSig = await program.methods
    .initMarket(
      marketIdBytes,
      marketIdBytes,
      new BN(0),
      new BN(fortressSpreadBps),
      new BN(maxMultiplierBps),
      { medium: {} },
    )
    .accounts({
      admin: authority,
    })
    .rpc();
  console.log(`initMarket tx:  ${initMarketSig}`);

  if (!args.skipMarketUpdate) {
    if (signer.equals(keeper)) {
      const updateSig = await program.methods
        .updateMarket(
          marketIdBytes,
          new BN(DEFAULT_REGIME_ID),
          new BN(fortressSpreadBps),
          new BN(maxMultiplierBps),
          { medium: {} },
        )
        .accounts({ keeper })
        .rpc();
      console.log(`updateMarket tx: ${updateSig}`);
    } else {
      console.log(
        "Skipping updateMarket: signer is not keeper. Use --skip-market-update to suppress.",
      );
    }
  }

  console.log("\nDone.\n");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
