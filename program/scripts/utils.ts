/**
 * Shared utilities for carnot-programs scripts.
 */
import * as anchor from "@coral-xyz/anchor";
import * as fs from "node:fs";
import * as path from "node:path";
import { Keypair, PublicKey } from "@solana/web3.js";

const TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
);

const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey(
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
);

export function expandHome(p: string): string {
  return p.startsWith("~/") ? path.join(process.env.HOME ?? "", p.slice(2)) : p;
}

function isAnchorIdlJson(data: unknown): data is anchor.Idl {
  if (data === null || typeof data !== "object") return false;
  const o = data as Record<string, unknown>;
  if (typeof o.address !== "string") return false;
  if (!Array.isArray(o.instructions)) return false;
  const m = o.metadata;
  if (m === null || typeof m !== "object") return false;
  const md = m as Record<string, unknown>;
  return (
    typeof md.name === "string" &&
    typeof md.version === "string" &&
    typeof md.spec === "string"
  );
}

function isSolanaSecretKeyJson(data: unknown): data is number[] {
  if (!Array.isArray(data) || data.length !== 64) return false;
  return data.every(
    (x) => typeof x === "number" && Number.isInteger(x) && x >= 0 && x <= 255,
  );
}

/** Load a Solana keypair from the standard JSON secret-key array (64 bytes). */
export function keypairFromWalletJsonPath(walletPath: string): Keypair {
  const parsed: unknown = JSON.parse(
    fs.readFileSync(expandHome(walletPath), "utf8"),
  );
  if (!isSolanaSecretKeyJson(parsed)) {
    throw new Error(
      "Wallet file must be a JSON array of 64 integers in the range 0..255 (Solana keypair format).",
    );
  }
  return Keypair.fromSecretKey(Uint8Array.from(parsed));
}

/**
 * Load the carnot_engine IDL from the Anchor build output.
 * Resolves the program ID from the IDL's `address` field, falling back to
 * an explicit override when provided.
 */
export function loadIdl(explicitProgramId?: string): {
  idl: anchor.Idl;
  programId: PublicKey;
} {
  const idlPath = path.join(process.cwd(), "target", "idl", "carnot_engine.json");
  if (!fs.existsSync(idlPath)) {
    throw new Error(`IDL not found at ${idlPath}. Run "anchor build" first.`);
  }
  const parsed: unknown = JSON.parse(fs.readFileSync(idlPath, "utf8"));
  if (!isAnchorIdlJson(parsed)) {
    throw new Error(
      `Invalid Anchor IDL at ${idlPath}: expected address, metadata { name, version, spec }, and instructions[].`,
    );
  }
  const idl = parsed;
  const resolved = explicitProgramId ?? idl.address;
  if (!resolved) {
    throw new Error(
      "Could not resolve program id. Provide --program-id or ensure IDL contains `address`.",
    );
  }
  const programId = new PublicKey(resolved);
  idl.address = programId.toBase58();
  return { idl, programId };
}

export function loadWallet(walletPath: string): anchor.Wallet {
  return new anchor.Wallet(keypairFromWalletJsonPath(walletPath));
}

export function parseAnchorWalletPath(): string | undefined {
  const toml = path.join(process.cwd(), "Anchor.toml");
  if (!fs.existsSync(toml)) return undefined;
  return fs.readFileSync(toml, "utf8").match(/wallet\s*=\s*"([^"]+)"/)?.[1];
}

export function ataFor(owner: PublicKey, mint: PublicKey): PublicKey {
  const [ata] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  return ata;
}
