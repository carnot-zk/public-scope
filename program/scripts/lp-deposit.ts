/**
 * Fund the LP pool by calling lp_deposit instruction.
 * Usage: bun scripts/lp-deposit.ts --wallet <path> --amount <usdt-amount>
 *   --wallet  path to signer keypair JSON (default: /Users/krieger/solana-deploy-devnet.json)
 *   --amount  USDT amount (integer, default: 3)
 */
import * as anchor from "@coral-xyz/anchor";
import BN from "bn.js";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { getAssociatedTokenAddress } from "@solana/spl-token";
import { USDT_MINT } from "@carnot/sdk";
import {
  CARNOT_ENGINE_PROGRAM_ID,
  findGlobalState,
  findVaultState,
  findLpPosition,
} from "../ts-sdk/pda";
import { keypairFromWalletJsonPath, loadIdl } from "./utils";
import type { CarnotEngine } from "../target/types/carnot_engine";

const USDT_MINT_DEVNET = new PublicKey(USDT_MINT.devnet);
const RPC = "https://api.devnet.solana.com";

function parseArgs() {
  const args = process.argv.slice(2);
  let walletPath = "/Users/krieger/solana-deploy-devnet.json";
  let amount = 3; // USDT
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--wallet" && args[i + 1]) walletPath = args[++i];
    if (args[i] === "--amount" && args[i + 1]) amount = parseInt(args[++i], 10);
  }
  return { walletPath, amount };
}

async function main() {
  const { walletPath, amount } = parseArgs();
  const microAmount = BigInt(amount) * BigInt(1_000_000); // USDT has 6 decimals

  const kp = keypairFromWalletJsonPath(walletPath);
  console.log(`Signer: ${kp.publicKey.toBase58()}`);

  const conn = new Connection(RPC, "confirmed");
  const wallet = new anchor.Wallet(kp);
  const provider = new anchor.AnchorProvider(conn, wallet, {
    commitment: "confirmed",
  });

  const { idl } = loadIdl();
  const program = new anchor.Program<CarnotEngine>(idl, provider);

  const [globalState] = findGlobalState(CARNOT_ENGINE_PROGRAM_ID);
  const [vaultState] = findVaultState(CARNOT_ENGINE_PROGRAM_ID);
  const [lpPosition] = findLpPosition(kp.publicKey, CARNOT_ENGINE_PROGRAM_ID);

  const lpVaultTokenAccount = await getAssociatedTokenAddress(
    USDT_MINT_DEVNET,
    vaultState,
    true,
  );
  const lpTokenAccount = await getAssociatedTokenAddress(USDT_MINT_DEVNET, kp.publicKey);

  console.log(`Depositing ${amount} USDT (${microAmount} micro) into LP pool...`);
  console.log(`  globalState:         ${globalState.toBase58()}`);
  console.log(`  vaultState:          ${vaultState.toBase58()}`);
  console.log(`  lpPosition:          ${lpPosition.toBase58()}`);
  console.log(`  lpTokenAccount:      ${lpTokenAccount.toBase58()}`);
  console.log(`  lpVaultTokenAccount: ${lpVaultTokenAccount.toBase58()}`);

  const tx = await program.methods
    .lpDeposit(new BN(microAmount.toString()))
    .accounts({
      lp: kp.publicKey,
      lpTokenAccount,
      lpVaultTokenAccount,
    })
    .signers([kp])
    .rpc({ commitment: "confirmed" });

  console.log(`lp_deposit tx: ${tx}`);
  console.log(
    `Explorer: https://explorer.solana.com/tx/${tx}?cluster=devnet`,
  );

  const vault = await program.account.vaultState.fetch(vaultState);
  console.log(
    `Pool lpTotalDeposited after deposit: ${vault.lpTotalDeposited.toString()} micro-USDT`,
  );
}

main().catch((err) => {
  console.error("Error:", err);
  process.exit(1);
});
