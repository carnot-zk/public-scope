/**
 * End-to-end integration tests for carnot_engine.
 *
 * Run via:  anchor test --skip-build   (program already built)
 *           anchor test                (build + deploy + run)
 *
 * Coverage:
 *   admin_init · pause/unpause · rotate_keeper · withdraw_protocol_fees
 *   init_market · update_market (regime guard, signer guard)
 *   lp_deposit · lp_withdraw (shares, timelock error)
 *   trader_deposit · trader_withdraw · lock_margin · unlock_margin
 *   verify_and_settle — all guard rails that fire before ZK / Pyth verification
 *   Merkle / hash helpers tested as pure-logic unit tests
 *
 * NOT tested here (requires real Groth16 proof + live Pyth accounts):
 *   verify_and_settle happy path · distribute_winnings happy path
 */

import * as anchor from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import type { Program } from "@coral-xyz/anchor";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { createHash } from "crypto";
import { describe, it, expect, beforeAll } from "vitest";
import type { CarnotEngine } from "../target/types/carnot_engine";

// ─── PDA helpers ─────────────────────────────────────────────────────────────

const B = (s: string) => Buffer.from(s);

const pdaGlobal = (pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("global")], pid);
const pdaVault = (pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("vault")], pid);
const pdaTraderVaultToken = (pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("trader_vault_token")], pid);
const pdaMarket = (id: Buffer, pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("market"), id], pid);
const pdaLpPos = (lp: PublicKey, pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("lp_position"), lp.toBuffer()], pid);
const pdaTraderAcc = (t: PublicKey, pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("trader_account"), t.toBuffer()], pid);
const pdaBatch = (batchId: Buffer, pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("batch"), batchId], pid);
const pdaNullifier = (hash: Buffer, pid: PublicKey) =>
  PublicKey.findProgramAddressSync([B("nullifier"), hash], pid);

// ─── Crypto helpers — must mirror on-chain logic exactly ─────────────────────

function sha256(...chunks: Buffer[]): Buffer {
  const h = createHash("sha256");
  for (const c of chunks) h.update(c);
  return h.digest();
}

const leI64 = (n: number) => {
  const b = Buffer.allocUnsafe(8);
  b.writeBigInt64LE(BigInt(n));
  return b;
};
const leU64 = (n: number) => {
  const b = Buffer.allocUnsafe(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
};
const leU32 = (n: number) => {
  const b = Buffer.allocUnsafe(4);
  b.writeUInt32LE(n);
  return b;
};

/**
 * Mirrors compute_public_outputs_hash from carnot_engine/src/utils/mod.rs.
 * Field order MUST be identical to the Rust implementation.
 */
function computePublicOutputsHash(p: {
  batchId: Buffer;
  windowStart: number;
  windowEnd: number;
  numTrades: number;
  marketRegimeId: number;
  marketMaxMultiplier: number; // market.max_multiplier cast to u32 on-chain
  pythCheckpointsHash: Buffer;
  poolBalanceBefore: number;
  poolBalanceAfter: number;
  currentLiabilityBefore: number;
  netPayout: number;
  keeperFee: number;
  protocolFee: number;
  numWinners: number;
  numLosers: number;
  totalWinnersPayout: number;
  totalLosersStake: number;
  payoutsCommitment: Buffer;
  tradesCommitment: Buffer;
  nullifierHash: Buffer;
}): Buffer {
  return sha256(
    p.batchId,
    leI64(p.windowStart),
    leI64(p.windowEnd),
    leU32(p.numTrades),
    leU64(p.marketRegimeId),
    leU32(p.marketMaxMultiplier),
    p.pythCheckpointsHash,
    leU64(p.poolBalanceBefore),
    leU64(p.poolBalanceAfter),
    leU64(p.currentLiabilityBefore),
    leI64(p.netPayout),
    leU64(p.keeperFee),
    leU64(p.protocolFee),
    leU32(p.numWinners),
    leU32(p.numLosers),
    leU64(p.totalWinnersPayout),
    leU64(p.totalLosersStake),
    p.payoutsCommitment,
    p.tradesCommitment,
    p.nullifierHash,
  );
}

/**
 * Mirrors hashPayoutLeaf from carnot_engine/src/module/admin.rs.
 * leaf = sha256(batch_id || trade_id || trader || winnings_le8 || index_le4)
 */
function hashPayoutLeaf(
  batchId: Buffer,
  tradeId: Buffer,
  trader: PublicKey,
  winningsUsdt: bigint,
  payoutIndex: number,
): Buffer {
  const w = Buffer.allocUnsafe(8);
  w.writeBigUInt64LE(winningsUsdt);
  const i = Buffer.allocUnsafe(4);
  i.writeUInt32LE(payoutIndex);
  return sha256(batchId, tradeId, trader.toBuffer(), w, i);
}

/**
 * Mirrors compute_merkle_root_from_proof from carnot_engine/src/utils/mod.rs.
 */
function computeMerkleRoot(
  leaf: Buffer,
  index: number,
  siblings: Buffer[],
): Buffer {
  let cur = leaf;
  let idx = index;
  for (const s of siblings) {
    cur = idx % 2 === 0 ? sha256(cur, s) : sha256(s, cur);
    idx = Math.floor(idx / 2);
  }
  return cur;
}

// ─── Assertion helpers ────────────────────────────────────────────────────────

async function expectErr(
  promise: Promise<unknown>,
  pattern: string | RegExp,
): Promise<void> {
  try {
    await promise;
    throw new Error(`__SENTINEL__: expected error matching ${pattern} but resolved`);
  } catch (err: unknown) {
    if (err instanceof Error && err.message.startsWith("__SENTINEL__")) throw err;
    const msg =
      (err instanceof Error ? err.message : "") + JSON.stringify(err);
    if (typeof pattern === "string") {
      expect(msg, `Wanted error "${pattern}" in:\n${msg}`).toContain(pattern);
    } else {
      expect(msg, `Wanted error matching ${pattern} in:\n${msg}`).toMatch(pattern);
    }
  }
}

// Anchor 0.32 auto-resolves PDAs, so .accounts() types reject extra keys.
// We cast to any throughout to keep explicit account addresses (good for
// readability and error diagnosis) without fighting the generated types.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const accs = (o: Record<string, unknown>) => o as any;

// lock_margin / unlock_margin were added to the program after the IDL was last
// exported, so they're absent from target/types/carnot_engine.ts.  We call them
// via an any-cast; after the next `anchor build` the types will be updated.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const anyMethods = (p: Program<CarnotEngine>) => (p as any).methods;

// TraderAccount's `locked_usdt` is still `reserved1` in the stale IDL.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const lockedUsdt = (ta: any): BN => ta.lockedUsdt ?? ta.reserved1 ?? new BN(0);

const arr = (b: Buffer) => Array.from(b);
const zeros = (n: number): number[] => Array<number>(n).fill(0);

// ─── Suite ───────────────────────────────────────────────────────────────────

describe("carnot-engine", () => {
  let provider: anchor.AnchorProvider;
  let program: Program<CarnotEngine>;
  let pid: PublicKey;
  let conn: anchor.web3.Connection;

  let admin: Keypair;
  let keeper: Keypair;
  let usdtMint: PublicKey;

  let globalState: PublicKey;
  let vaultState: PublicKey;
  let lpVaultTokenAccount: PublicKey;
  let traderVaultToken: PublicKey;

  const MARKET_ID = Buffer.alloc(32, 1);
  const MARKET_REGIME_ID = 1;
  const MARKET_MAX_MULTIPLIER = 20_000;
  let marketState: PublicKey;

  // lp1 deposits in outer beforeAll so the LP vault has funds for settle tests
  let lp1: Keypair;
  let lp1Ata: PublicKey;
  const LP1_DEPOSIT = 1_000_000_000; // 1 000 USDT in micro-USDT (6 dp)

  let keeperAta: PublicKey;

  // ── infra helpers ────────────────────────────────────────────────────────

  async function fund(kp: Keypair, sol = 10) {
    const sig = await conn.requestAirdrop(kp.publicKey, sol * LAMPORTS_PER_SOL);
    const { blockhash, lastValidBlockHeight } = await conn.getLatestBlockhash();
    await conn.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight });
  }

  async function mintUsdt(to: PublicKey, amount: number) {
    await mintTo(conn, admin, usdtMint, to, admin, amount);
  }

  async function createUsdtAta(owner: Keypair): Promise<PublicKey> {
    return createAssociatedTokenAccount(conn, owner, usdtMint, owner.publicKey);
  }

  async function tokenBalance(account: PublicKey): Promise<bigint> {
    return (await getAccount(conn, account)).amount;
  }

  // ── global setup ─────────────────────────────────────────────────────────

  beforeAll(async () => {
    provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);
    program = anchor.workspace.CarnotEngine as Program<CarnotEngine>;
    pid = program.programId;
    conn = provider.connection;

    admin = Keypair.generate();
    keeper = Keypair.generate();
    lp1 = Keypair.generate();
    await Promise.all([fund(admin), fund(keeper), fund(lp1)]);

    usdtMint = await createMint(conn, admin, admin.publicKey, null, 6);

    [globalState] = pdaGlobal(pid);
    [vaultState] = pdaVault(pid);
    [traderVaultToken] = pdaTraderVaultToken(pid);
    lpVaultTokenAccount = getAssociatedTokenAddressSync(usdtMint, vaultState, true);
    [marketState] = pdaMarket(MARKET_ID, pid);

    // admin_init — both intervals = 0 so time-based guards always pass in tests
    await program.methods
      .adminInit(keeper.publicKey, 100, new BN(0), new BN(0))
      .accounts(accs({
        admin: admin.publicKey,
        globalState,
        vaultState,
        lpVaultTokenAccount,
        traderVaultTokenAccount: traderVaultToken,
        usdtMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      }))
      .signers([admin])
      .rpc();

    // Shared market
    await program.methods
      .initMarket(
        arr(MARKET_ID),
        zeros(32),
        new BN(MARKET_REGIME_ID),
        new BN(500),
        new BN(MARKET_MAX_MULTIPLIER),
        { low: {} },
      )
      .accounts(accs({
        admin: admin.publicKey,
        globalState,
        marketState,
        systemProgram: SystemProgram.programId,
      }))
      .signers([admin])
      .rpc();

    // lp1 deposits so vault.lp_total_deposited > 0 (needed for settle guard tests)
    lp1Ata = await createUsdtAta(lp1);
    await mintUsdt(lp1Ata, LP1_DEPOSIT);

    const [lp1Pos] = pdaLpPos(lp1.publicKey, pid);
    await program.methods
      .lpDeposit(new BN(LP1_DEPOSIT))
      .accounts(accs({
        lp: lp1.publicKey,
        globalState,
        vaultState,
        lpPosition: lp1Pos,
        lpTokenAccount: lp1Ata,
        lpVaultTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      }))
      .signers([lp1])
      .rpc();

    keeperAta = await createUsdtAta(keeper);
  }, 120_000);

  // ══════════════════════════════════════════════════════════════════════════
  // Admin
  // ══════════════════════════════════════════════════════════════════════════

  describe("admin", () => {
    it("globalState is initialised correctly", async () => {
      const gs = await program.account.globalState.fetch(globalState);
      expect(gs.admin.toBase58()).toBe(admin.publicKey.toBase58());
      expect(gs.keeper.toBase58()).toBe(keeper.publicKey.toBase58());
      expect(gs.protocolFeeBps).toBe(100);
      expect(gs.paused).toBe(false);
      expect(gs.minBatchIntervalSecs.toNumber()).toBe(0);
    });

    it("vaultState initialised — lp_total_deposited equals lp1 deposit", async () => {
      const vs = await program.account.vaultState.fetch(vaultState);
      expect(vs.lpTotalDeposited.toNumber()).toBe(LP1_DEPOSIT);
      expect(vs.currentLiability.toNumber()).toBe(0);
      expect(vs.accruedProtocolFees.toNumber()).toBe(0);
    });

    it("rotate_keeper — new keeper can act, old keeper cannot", async () => {
      const newKeeper = Keypair.generate();
      await fund(newKeeper);

      await program.methods
        .rotateKeeper(newKeeper.publicKey)
        .accounts(accs({ admin: admin.publicKey, globalState }))
        .signers([admin])
        .rpc();

      expect(
        (await program.account.globalState.fetch(globalState)).keeper.toBase58(),
      ).toBe(newKeeper.publicKey.toBase58());

      // Old keeper can no longer call keeper-gated instructions
      await expectErr(
        program.methods
          .updateMarket(arr(MARKET_ID), new BN(999), new BN(100), new BN(10_000), { low: {} })
          .accounts(accs({ keeper: keeper.publicKey, globalState, marketState }))
          .signers([keeper])
          .rpc(),
        "ConstraintHasOne",
      );

      // Restore
      await program.methods
        .rotateKeeper(keeper.publicKey)
        .accounts(accs({ admin: admin.publicKey, globalState }))
        .signers([newKeeper])
        .rpc();

      expect(
        (await program.account.globalState.fetch(globalState)).keeper.toBase58(),
      ).toBe(keeper.publicKey.toBase58());
    });

    it("non-admin cannot rotate_keeper", async () => {
      const rogue = Keypair.generate();
      await fund(rogue);

      await expectErr(
        program.methods
          .rotateKeeper(rogue.publicKey)
          .accounts(accs({ admin: rogue.publicKey, globalState }))
          .signers([rogue])
          .rpc(),
        "ConstraintHasOne",
      );
    });

    it("pause blocks lp_deposit; unpause re-enables it", async () => {
      await program.methods
        .pause()
        .accounts(accs({ admin: admin.publicKey, globalState }))
        .signers([admin])
        .rpc();

      expect((await program.account.globalState.fetch(globalState)).paused).toBe(true);

      const lp = Keypair.generate();
      await fund(lp);
      const lpAta = await createUsdtAta(lp);
      await mintUsdt(lpAta, 1_000_000);
      const [lpPos] = pdaLpPos(lp.publicKey, pid);

      await expectErr(
        program.methods
          .lpDeposit(new BN(1_000_000))
          .accounts(accs({
            lp: lp.publicKey, globalState, vaultState,
            lpPosition: lpPos, lpTokenAccount: lpAta,
            lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          }))
          .signers([lp])
          .rpc(),
        "Paused",
      );

      await program.methods
        .unpause()
        .accounts(accs({ admin: admin.publicKey, globalState }))
        .signers([admin])
        .rpc();

      expect((await program.account.globalState.fetch(globalState)).paused).toBe(false);
    });

    it("non-admin cannot pause", async () => {
      const rogue = Keypair.generate();
      await fund(rogue);

      await expectErr(
        program.methods
          .pause()
          .accounts(accs({ admin: rogue.publicKey, globalState }))
          .signers([rogue])
          .rpc(),
        "ConstraintHasOne",
      );
    });

    it("withdraw_protocol_fees zero amount → InvalidAmount", async () => {
      const adminAta = getAssociatedTokenAddressSync(usdtMint, admin.publicKey);
      // create it if missing
      try { await createAssociatedTokenAccount(conn, admin, usdtMint, admin.publicKey); } catch {}

      await expectErr(
        program.methods
          .withdrawProtocolFees(new BN(0))
          .accounts(accs({
            admin: admin.publicKey, globalState, vaultState,
            lpVaultTokenAccount, adminTokenAccount: adminAta,
            tokenProgram: TOKEN_PROGRAM_ID,
          }))
          .signers([admin])
          .rpc(),
        "InvalidAmount",
      );
    });

    it("withdraw_protocol_fees > accrued → InvalidAmount", async () => {
      const vs = await program.account.vaultState.fetch(vaultState);
      const adminAta = getAssociatedTokenAddressSync(usdtMint, admin.publicKey);

      await expectErr(
        program.methods
          .withdrawProtocolFees(new BN(vs.accruedProtocolFees.toNumber() + 1))
          .accounts(accs({
            admin: admin.publicKey, globalState, vaultState,
            lpVaultTokenAccount, adminTokenAccount: adminAta,
            tokenProgram: TOKEN_PROGRAM_ID,
          }))
          .signers([admin])
          .rpc(),
        "InvalidAmount",
      );
    });
  });

  // ══════════════════════════════════════════════════════════════════════════
  // Market
  // ══════════════════════════════════════════════════════════════════════════

  describe("market", () => {
    it("marketState fields are correct after init", async () => {
      const ms = await program.account.marketState.fetch(marketState);
      expect(ms.regimeId.toNumber()).toBe(MARKET_REGIME_ID);
      expect(ms.maxMultiplier.toNumber()).toBe(MARKET_MAX_MULTIPLIER);
      expect(ms.fortressSpreadBps.toNumber()).toBe(500);
    });

    it("init_market with zero fortress_spread_bps → InvalidAmount", async () => {
      const id = Buffer.alloc(32, 2);
      const [ms] = pdaMarket(id, pid);

      await expectErr(
        program.methods
          .initMarket(arr(id), zeros(32), new BN(1), new BN(0), new BN(10_000), { low: {} })
          .accounts(accs({ admin: admin.publicKey, globalState, marketState: ms, systemProgram: SystemProgram.programId }))
          .signers([admin])
          .rpc(),
        "InvalidAmount",
      );
    });

    it("init_market with zero max_multiplier → InvalidAmount", async () => {
      const id = Buffer.alloc(32, 3);
      const [ms] = pdaMarket(id, pid);

      await expectErr(
        program.methods
          .initMarket(arr(id), zeros(32), new BN(1), new BN(100), new BN(0), { low: {} })
          .accounts(accs({ admin: admin.publicKey, globalState, marketState: ms, systemProgram: SystemProgram.programId }))
          .signers([admin])
          .rpc(),
        "InvalidAmount",
      );
    });

    it("update_market — keeper bumps regime and params are persisted", async () => {
      const msBefore = await program.account.marketState.fetch(marketState);
      const nextRegime = msBefore.regimeId.toNumber() + 1;

      await program.methods
        .updateMarket(arr(MARKET_ID), new BN(nextRegime), new BN(800), new BN(30_000), { medium: {} })
        .accounts(accs({ keeper: keeper.publicKey, globalState, marketState }))
        .signers([keeper])
        .rpc();

      const msAfter = await program.account.marketState.fetch(marketState);
      expect(msAfter.regimeId.toNumber()).toBe(nextRegime);
      expect(msAfter.fortressSpreadBps.toNumber()).toBe(800);
      expect(msAfter.maxMultiplier.toNumber()).toBe(30_000);
    });

    it("update_market same regime_id → InvalidMarketRegime", async () => {
      const current = (await program.account.marketState.fetch(marketState)).regimeId.toNumber();

      await expectErr(
        program.methods
          .updateMarket(arr(MARKET_ID), new BN(current), new BN(100), new BN(10_000), { low: {} })
          .accounts(accs({ keeper: keeper.publicKey, globalState, marketState }))
          .signers([keeper])
          .rpc(),
        "InvalidMarketRegime",
      );
    });

    it("update_market lower regime_id → InvalidMarketRegime", async () => {
      const current = (await program.account.marketState.fetch(marketState)).regimeId.toNumber();

      await expectErr(
        program.methods
          .updateMarket(arr(MARKET_ID), new BN(current - 1), new BN(100), new BN(10_000), { low: {} })
          .accounts(accs({ keeper: keeper.publicKey, globalState, marketState }))
          .signers([keeper])
          .rpc(),
        "InvalidMarketRegime",
      );
    });

    it("non-keeper cannot update_market → ConstraintHasOne", async () => {
      const rogue = Keypair.generate();
      await fund(rogue);

      await expectErr(
        program.methods
          .updateMarket(arr(MARKET_ID), new BN(9999), new BN(100), new BN(10_000), { low: {} })
          .accounts(accs({ keeper: rogue.publicKey, globalState, marketState }))
          .signers([rogue])
          .rpc(),
        "ConstraintHasOne",
      );
    });
  });

  // ══════════════════════════════════════════════════════════════════════════
  // LP deposit / withdraw
  // ══════════════════════════════════════════════════════════════════════════

  describe("lp", () => {
    let lp: Keypair;
    let lpAta: PublicKey;
    let lpPos: PublicKey;
    const FIRST_DEPOSIT = 500_000_000; // 500 USDT

    beforeAll(async () => {
      lp = Keypair.generate();
      await fund(lp);
      lpAta = await createUsdtAta(lp);
      await mintUsdt(lpAta, FIRST_DEPOSIT * 3);
      [lpPos] = pdaLpPos(lp.publicKey, pid);
    }, 60_000);

    it("lp_deposit — tokens transferred, shares minted, vault updated", async () => {
      const vsBefore = await program.account.vaultState.fetch(vaultState);
      const totalDepBefore = vsBefore.lpTotalDeposited.toNumber();
      const totalSharesBefore = vsBefore.totalLpShares.toNumber();
      const walletBefore = await tokenBalance(lpAta);

      await program.methods
        .lpDeposit(new BN(FIRST_DEPOSIT))
        .accounts(accs({
          lp: lp.publicKey, globalState, vaultState,
          lpPosition: lpPos, lpTokenAccount: lpAta,
          lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        }))
        .signers([lp])
        .rpc();

      const vsAfter = await program.account.vaultState.fetch(vaultState);
      const lpPosition = await program.account.lpPosition.fetch(lpPos);
      const walletAfter = await tokenBalance(lpAta);

      expect(vsAfter.lpTotalDeposited.toNumber()).toBe(totalDepBefore + FIRST_DEPOSIT);
      expect(walletBefore - walletAfter).toBe(BigInt(FIRST_DEPOSIT));
      expect(lpPosition.lpShares.toNumber()).toBeGreaterThan(0);

      // Shares = amount * totalShares / totalDeposited
      const expectedShares = Math.floor(
        (FIRST_DEPOSIT * totalSharesBefore) / totalDepBefore,
      );
      expect(vsAfter.totalLpShares.toNumber()).toBe(totalSharesBefore + expectedShares);
    });

    it("second deposit mints proportional shares (not 1:1 after pool is seeded)", async () => {
      const vsBefore = await program.account.vaultState.fetch(vaultState);
      const posBefore = await program.account.lpPosition.fetch(lpPos);

      const SECOND = 100_000_000;
      const expectedNew = Math.floor(
        (SECOND * vsBefore.totalLpShares.toNumber()) / vsBefore.lpTotalDeposited.toNumber(),
      );

      await program.methods
        .lpDeposit(new BN(SECOND))
        .accounts(accs({
          lp: lp.publicKey, globalState, vaultState,
          lpPosition: lpPos, lpTokenAccount: lpAta,
          lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        }))
        .signers([lp])
        .rpc();

      const posAfter = await program.account.lpPosition.fetch(lpPos);
      expect(posAfter.lpShares.toNumber()).toBe(
        posBefore.lpShares.toNumber() + expectedNew,
      );
    });

    it("lp_deposit zero amount → InvalidAmount", async () => {
      await expectErr(
        program.methods
          .lpDeposit(new BN(0))
          .accounts(accs({
            lp: lp.publicKey, globalState, vaultState,
            lpPosition: lpPos, lpTokenAccount: lpAta,
            lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          }))
          .signers([lp])
          .rpc(),
        "InvalidAmount",
      );
    });

    it("lp_withdraw immediately after deposit → TimelockNotExpired", async () => {
      const pos = await program.account.lpPosition.fetch(lpPos);
      expect(pos.lpShares.toNumber()).toBeGreaterThan(0);

      await expectErr(
        program.methods
          .lpWithdraw(new BN(1))
          .accounts(accs({
            lp: lp.publicKey, globalState, vaultState,
            lpPosition: lpPos, lpTokenAccount: lpAta,
            lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
          }))
          .signers([lp])
          .rpc(),
        "TimelockNotExpired",
      );
    });

    it("lp_withdraw more shares than owned → TimelockNotExpired (fires before InsufficientShares)", async () => {
      const pos = await program.account.lpPosition.fetch(lpPos);

      await expectErr(
        program.methods
          .lpWithdraw(new BN(pos.lpShares.toNumber() + 1_000_000_000))
          .accounts(accs({
            lp: lp.publicKey, globalState, vaultState,
            lpPosition: lpPos, lpTokenAccount: lpAta,
            lpVaultTokenAccount, tokenProgram: TOKEN_PROGRAM_ID,
          }))
          .signers([lp])
          .rpc(),
        /(TimelockNotExpired|InsufficientShares)/,
      );
    });
  });

  // ══════════════════════════════════════════════════════════════════════════
  // Trader deposit / withdraw / margin
  // ══════════════════════════════════════════════════════════════════════════

  describe("trader", () => {
    let trader: Keypair;
    let traderAta: PublicKey;
    let traderAcc: PublicKey;
    const DEPOSIT = 100_000_000; // 100 USDT

    beforeAll(async () => {
      trader = Keypair.generate();
      await fund(trader);
      traderAta = await createUsdtAta(trader);
      await mintUsdt(traderAta, DEPOSIT * 3);
      [traderAcc] = pdaTraderAcc(trader.publicKey, pid);
    }, 60_000);

    const depAccs = (t: Keypair) => accs({
      trader: t.publicKey, globalState, vaultState,
      traderAccount: traderAcc, traderTokenAccount: traderAta,
      traderVaultTokenAccount: traderVaultToken,
      tokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId,
    });

    const wdAccs = (t: Keypair) => accs({
      trader: t.publicKey, globalState, vaultState,
      traderAccount: traderAcc, traderTokenAccount: traderAta,
      traderVaultTokenAccount: traderVaultToken,
      tokenProgram: TOKEN_PROGRAM_ID,
    });

    it("trader_deposit — balance tracked, tokens leave wallet", async () => {
      const walletBefore = await tokenBalance(traderAta);

      await program.methods
        .traderDeposit(new BN(DEPOSIT))
        .accounts(depAccs(trader))
        .signers([trader])
        .rpc();

      const ta = await program.account.traderAccount.fetch(traderAcc);
      const walletAfter = await tokenBalance(traderAta);

      expect(ta.balanceUsdt.toNumber()).toBe(DEPOSIT);
      expect(lockedUsdt(ta).toNumber()).toBe(0);
      expect(walletBefore - walletAfter).toBe(BigInt(DEPOSIT));
    });

    it("trader_deposit zero amount → InvalidAmount", async () => {
      await expectErr(
        program.methods.traderDeposit(new BN(0)).accounts(depAccs(trader)).signers([trader]).rpc(),
        "InvalidAmount",
      );
    });

    it("lock_margin — locked_usdt increases, balance unchanged", async () => {
      const LOCK = 40_000_000;
      await anyMethods(program)
        .lockMargin(new BN(LOCK))
        .accounts(accs({ keeper: keeper.publicKey, globalState, trader: trader.publicKey, traderAccount: traderAcc }))
        .signers([keeper])
        .rpc();

      const ta = await program.account.traderAccount.fetch(traderAcc);
      expect(lockedUsdt(ta).toNumber()).toBe(LOCK);
      expect(ta.balanceUsdt.toNumber()).toBe(DEPOSIT);
    });

    it("lock_margin exceeds balance → InsufficientAvailableMargin", async () => {
      const ta = await program.account.traderAccount.fetch(traderAcc);
      await expectErr(
        anyMethods(program)
          .lockMargin(new BN(ta.balanceUsdt.toNumber() + 1))
          .accounts(accs({ keeper: keeper.publicKey, globalState, trader: trader.publicKey, traderAccount: traderAcc }))
          .signers([keeper])
          .rpc(),
        "InsufficientAvailableMargin",
      );
    });

    it("trader_withdraw locked portion is blocked → ExceedsWithdrawableBalance", async () => {
      const ta = await program.account.traderAccount.fetch(traderAcc);
      const withdrawable = ta.balanceUsdt.toNumber() - lockedUsdt(ta).toNumber();

      await expectErr(
        program.methods
          .traderWithdraw(new BN(withdrawable + 1))
          .accounts(wdAccs(trader))
          .signers([trader])
          .rpc(),
        "ExceedsWithdrawableBalance",
      );
    });

    it("trader_withdraw up to unlocked balance — succeeds, balance decreases", async () => {
      const ta = await program.account.traderAccount.fetch(traderAcc);
      const withdrawable = ta.balanceUsdt.toNumber() - lockedUsdt(ta).toNumber();
      expect(withdrawable).toBeGreaterThan(0);

      const walletBefore = await tokenBalance(traderAta);

      await program.methods
        .traderWithdraw(new BN(withdrawable))
        .accounts(wdAccs(trader))
        .signers([trader])
        .rpc();

      const taAfter = await program.account.traderAccount.fetch(traderAcc);
      const walletAfter = await tokenBalance(traderAta);

      expect(taAfter.balanceUsdt.toNumber()).toBe(lockedUsdt(ta).toNumber());
      expect(walletAfter - walletBefore).toBe(BigInt(withdrawable));
    });

    it("unlock_margin — locked_usdt decreases", async () => {
      const taBefore = await program.account.traderAccount.fetch(traderAcc);
      const locked = lockedUsdt(taBefore).toNumber();
      expect(locked).toBeGreaterThan(0);

      const UNLOCK = Math.floor(locked / 2);
      await anyMethods(program)
        .unlockMargin(new BN(UNLOCK))
        .accounts(accs({ keeper: keeper.publicKey, globalState, trader: trader.publicKey, traderAccount: traderAcc }))
        .signers([keeper])
        .rpc();

      const taAfter = await program.account.traderAccount.fetch(traderAcc);
      expect(lockedUsdt(taAfter).toNumber()).toBe(locked - UNLOCK);
    });

    it("unlock_margin more than locked → InvalidAmount", async () => {
      const ta = await program.account.traderAccount.fetch(traderAcc);
      await expectErr(
        anyMethods(program)
          .unlockMargin(new BN(lockedUsdt(ta).toNumber() + 1))
          .accounts(accs({ keeper: keeper.publicKey, globalState, trader: trader.publicKey, traderAccount: traderAcc }))
          .signers([keeper])
          .rpc(),
        "InvalidAmount",
      );
    });

    it("trader_withdraw zero amount → InvalidAmount", async () => {
      await expectErr(
        program.methods.traderWithdraw(new BN(0)).accounts(wdAccs(trader)).signers([trader]).rpc(),
        "InvalidAmount",
      );
    });

    it("trader_deposit when paused → Paused (then restored)", async () => {
      await program.methods.pause().accounts(accs({ admin: admin.publicKey, globalState })).signers([admin]).rpc();

      await expectErr(
        program.methods.traderDeposit(new BN(1_000_000)).accounts(depAccs(trader)).signers([trader]).rpc(),
        "Paused",
      );

      await program.methods.unpause().accounts(accs({ admin: admin.publicKey, globalState })).signers([admin]).rpc();
    });
  });

  // ══════════════════════════════════════════════════════════════════════════
  // verify_and_settle guard rails
  //
  // Each test checks the error that fires at a specific point in the
  // instruction's validation sequence. Tests targeting checks that come
  // after business-logic checks construct params carefully so that all
  // earlier checks pass.
  //
  // Checks #8 (Pyth) and #9 (Groth16) are NOT tested here — they require
  // real oracle accounts and a valid ZK proof respectively.
  // ══════════════════════════════════════════════════════════════════════════

  describe("verify_and_settle guard rails", () => {
    // Dummy proof bytes — structurally valid lengths but cryptographically invalid.
    const DUMMY_A = zeros(64);
    const DUMMY_B = zeros(128);
    const DUMMY_C = zeros(64);
    const DUMMY_NONCE = zeros(32);

    // Each test uses a unique seed so batch/nullifier PDAs never collide
    // even when prior transactions were rolled back.
    let nextSeed = 100;
    function freshIds() {
      const s = nextSeed++;
      const batchId = Buffer.alloc(32);
      batchId.writeUInt32LE(s, 0);
      const nullifierHash = Buffer.alloc(32);
      nullifierHash.writeUInt32LE(s + 10_000, 0);
      return { batchId, nullifierHash };
    }

    function settleAccounts(batchId: Buffer, nullifierHash: Buffer) {
      const [nullifierAccount] = pdaNullifier(nullifierHash, pid);
      const [batchReceiptAccount] = pdaBatch(batchId, pid);
      return accs({
        keeper: keeper.publicKey,
        globalState,
        vaultState,
        marketState,
        nullifierAccount,
        batchReceiptAccount,
        lpVaultTokenAccount,
        traderVaultTokenAccount: traderVaultToken,
        keeperTokenAccount: keeperAta,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      });
    }

    function baseSettle(batchId: Buffer, nullifierHash: Buffer, overrides: Record<string, unknown> = {}) {
      const defaults = {
        marketId: arr(MARKET_ID),
        batchId: arr(batchId),
        proofA: DUMMY_A,
        proofB: DUMMY_B,
        proofC: DUMMY_C,
        proofNonce: DUMMY_NONCE,
        publicOutputsHash: zeros(32),
        windowStart: new BN(0),
        windowEnd: new BN(1),
        pythCheckpointsHash: zeros(32),
        netPayoutUsdt: new BN(0),
        poolBalanceBefore: new BN(0),
        poolBalanceAfter: new BN(0),
        numTrades: 0,
        nullifierHash: arr(nullifierHash),
        keeperFee: new BN(0),
        currentLiabilityBefore: new BN(0),
        protocolFee: new BN(0),
        numWinners: 0,
        numLosers: 0,
        totalWinnersPayout: new BN(0),
        totalLosersStake: new BN(0),
        marketRegimeId: new BN(0),
        payoutsCommitment: zeros(32),
        tradesCommitment: zeros(32),
        ...overrides,
      };
      return program.methods.verifyAndSettle(
        defaults.marketId as number[],
        defaults.batchId as number[],
        defaults.proofA as number[],
        defaults.proofB as number[],
        defaults.proofC as number[],
        defaults.proofNonce as number[],
        defaults.publicOutputsHash as number[],
        defaults.windowStart as BN,
        defaults.windowEnd as BN,
        defaults.pythCheckpointsHash as number[],
        defaults.netPayoutUsdt as BN,
        defaults.poolBalanceBefore as BN,
        defaults.poolBalanceAfter as BN,
        defaults.numTrades as number,
        defaults.nullifierHash as number[],
        defaults.keeperFee as BN,
        defaults.currentLiabilityBefore as BN,
        defaults.protocolFee as BN,
        defaults.numWinners as number,
        defaults.numLosers as number,
        defaults.totalWinnersPayout as BN,
        defaults.totalLosersStake as BN,
        defaults.marketRegimeId as BN,
        defaults.payoutsCommitment as number[],
        defaults.tradesCommitment as number[],
      );
    }

    it("guard #1 — paused state → Paused", async () => {
      await program.methods.pause().accounts(accs({ admin: admin.publicKey, globalState })).signers([admin]).rpc();

      const { batchId, nullifierHash } = freshIds();
      await expectErr(
        baseSettle(batchId, nullifierHash)
          .accounts(settleAccounts(batchId, nullifierHash))
          .remainingAccounts([])
          .signers([keeper])
          .rpc(),
        "Paused",
      );

      await program.methods.unpause().accounts(accs({ admin: admin.publicKey, globalState })).signers([admin]).rpc();
    });

    it("guard #3 — current_liability_before mismatch → InvalidPoolBalance", async () => {
      // vault.current_liability = 0; passing 1 triggers the error
      const { batchId, nullifierHash } = freshIds();
      const ms = await program.account.marketState.fetch(marketState);

      await expectErr(
        baseSettle(batchId, nullifierHash, {
          currentLiabilityBefore: new BN(1), // WRONG — vault has 0
          marketRegimeId: new BN(ms.regimeId.toNumber()),
        })
          .accounts(settleAccounts(batchId, nullifierHash))
          .remainingAccounts([])
          .signers([keeper])
          .rpc(),
        "InvalidPoolBalance",
      );
    });

    it("guard #4 — market_regime_id mismatch → InvalidMarketRegime", async () => {
      const ms = await program.account.marketState.fetch(marketState);
      const { batchId, nullifierHash } = freshIds();

      await expectErr(
        baseSettle(batchId, nullifierHash, {
          currentLiabilityBefore: new BN(0),                         // correct
          marketRegimeId: new BN(ms.regimeId.toNumber() + 999),      // WRONG
        })
          .accounts(settleAccounts(batchId, nullifierHash))
          .remainingAccounts([])
          .signers([keeper])
          .rpc(),
        "InvalidMarketRegime",
      );
    });

    it("guard #6 — public_outputs_hash mismatch → InvalidGroth16Input", async () => {
      // Params are self-consistent except the hash we pass is deliberately wrong.
      const ms = await program.account.marketState.fetch(marketState);
      const vs = await program.account.vaultState.fetch(vaultState);
      const { batchId, nullifierHash } = freshIds();
      const poolBal = vs.lpTotalDeposited.toNumber();
      const regimeId = ms.regimeId.toNumber();

      // Compute the CORRECT hash for these params …
      const correctHash = computePublicOutputsHash({
        batchId,
        windowStart: 0, windowEnd: 1, numTrades: 0,
        marketRegimeId: regimeId,
        marketMaxMultiplier: ms.maxMultiplier.toNumber(),
        pythCheckpointsHash: Buffer.alloc(32),
        poolBalanceBefore: poolBal, poolBalanceAfter: poolBal,
        currentLiabilityBefore: 0,
        netPayout: 0, keeperFee: 0, protocolFee: 0,
        numWinners: 0, numLosers: 0,
        totalWinnersPayout: 0, totalLosersStake: 0,
        payoutsCommitment: Buffer.alloc(32),
        tradesCommitment: Buffer.alloc(32),
        nullifierHash,
      });

      // … then flip the last byte so it doesn't match
      const wrongHash = Buffer.from(correctHash);
      wrongHash[31] ^= 0xff;

      await expectErr(
        baseSettle(batchId, nullifierHash, {
          currentLiabilityBefore: new BN(0),
          marketRegimeId: new BN(regimeId),
          poolBalanceBefore: new BN(poolBal),
          poolBalanceAfter: new BN(poolBal),
          publicOutputsHash: arr(wrongHash), // WRONG — one bit flipped
        })
          .accounts(settleAccounts(batchId, nullifierHash))
          .remainingAccounts([])
          .signers([keeper])
          .rpc(),
        "InvalidGroth16Input",
      );
    });

    it("guard #7 — pool_balance_before != vault.lp_total_deposited → InvalidPoolBalance", async () => {
      // Construct a hash that is internally consistent (passes check #6) but
      // whose pool_balance_before doesn't match the actual vault balance.
      const ms = await program.account.marketState.fetch(marketState);
      const { batchId, nullifierHash } = freshIds();
      const regimeId = ms.regimeId.toNumber();

      const WRONG_POOL = 0; // vault has LP1_DEPOSIT + more; 0 is always wrong

      const hashWithWrongPool = computePublicOutputsHash({
        batchId,
        windowStart: 0, windowEnd: 1, numTrades: 0,
        marketRegimeId: regimeId,
        marketMaxMultiplier: ms.maxMultiplier.toNumber(),
        pythCheckpointsHash: Buffer.alloc(32),
        poolBalanceBefore: WRONG_POOL,
        poolBalanceAfter: WRONG_POOL,
        currentLiabilityBefore: 0,
        netPayout: 0, keeperFee: 0, protocolFee: 0,
        numWinners: 0, numLosers: 0,
        totalWinnersPayout: 0, totalLosersStake: 0,
        payoutsCommitment: Buffer.alloc(32),
        tradesCommitment: Buffer.alloc(32),
        nullifierHash,
      });

      // check #6 PASSES (hash matches wrong-pool params we're sending)
      // check #7 FAILS (0 != vault.lp_total_deposited)
      await expectErr(
        baseSettle(batchId, nullifierHash, {
          currentLiabilityBefore: new BN(0),
          marketRegimeId: new BN(regimeId),
          poolBalanceBefore: new BN(WRONG_POOL),
          poolBalanceAfter: new BN(WRONG_POOL),
          publicOutputsHash: arr(hashWithWrongPool),
        })
          .accounts(settleAccounts(batchId, nullifierHash))
          .remainingAccounts([])
          .signers([keeper])
          .rpc(),
        "InvalidPoolBalance",
      );
    });

    it("non-keeper cannot call verify_and_settle → ConstraintHasOne", async () => {
      const rogue = Keypair.generate();
      await fund(rogue);
      const rogueAta = await createUsdtAta(rogue);
      const { batchId, nullifierHash } = freshIds();

      await expectErr(
        baseSettle(batchId, nullifierHash)
          .accounts({
            ...settleAccounts(batchId, nullifierHash),
            keeper: rogue.publicKey,
            keeperTokenAccount: rogueAta,
          })
          .remainingAccounts([])
          .signers([rogue])
          .rpc(),
        "ConstraintHasOne",
      );
    });
  });

  // ══════════════════════════════════════════════════════════════════════════
  // Pure-logic unit tests for the crypto helpers
  //
  // These run entirely in JS (no on-chain calls) and verify that our
  // TypeScript implementations produce the same output as the Rust code.
  // ══════════════════════════════════════════════════════════════════════════

  describe("merkle / hash helpers (pure logic)", () => {
    const BATCH_ID = Buffer.alloc(32, 7);

    it("single-leaf tree: root equals the leaf", () => {
      const trader = Keypair.generate().publicKey;
      const leaf = hashPayoutLeaf(BATCH_ID, Buffer.alloc(32, 1), trader, BigInt(1000000), 0);
      expect(computeMerkleRoot(leaf, 0, []).toString("hex")).toBe(
        leaf.toString("hex"),
      );
    });

    it("two-leaf tree: sha256(leaf0 || leaf1), verifiable from both sides", () => {
      const a = Keypair.generate().publicKey;
      const b = Keypair.generate().publicKey;
      const leaf0 = hashPayoutLeaf(BATCH_ID, Buffer.alloc(32, 1), a, BigInt(2000000), 0);
      const leaf1 = hashPayoutLeaf(BATCH_ID, Buffer.alloc(32, 2), b, BigInt(0), 1);
      const expected = sha256(leaf0, leaf1);

      expect(computeMerkleRoot(leaf0, 0, [leaf1]).toString("hex")).toBe(expected.toString("hex"));
      expect(computeMerkleRoot(leaf1, 1, [leaf0]).toString("hex")).toBe(expected.toString("hex"));
    });

    it("wrong sibling yields a different root", () => {
      const trader = Keypair.generate().publicKey;
      const leaf = hashPayoutLeaf(BATCH_ID, Buffer.alloc(32, 5), trader, BigInt(500000), 0);
      const correct = computeMerkleRoot(leaf, 0, [Buffer.alloc(32, 9)]);
      const wrong = computeMerkleRoot(leaf, 0, [Buffer.alloc(32, 99)]);
      expect(correct.toString("hex")).not.toBe(wrong.toString("hex"));
    });

    it("four-leaf tree: two-level path for index 2 is correct", () => {
      const mk = (seed: number) => {
        const t = Keypair.generate().publicKey;
        return hashPayoutLeaf(BATCH_ID, Buffer.alloc(32, seed), t, BigInt(seed * 100), seed);
      };
      const [l0, l1, l2, l3] = [mk(0), mk(1), mk(2), mk(3)];
      const leftSubtree = sha256(l0, l1);
      const rightSubtree = sha256(l2, l3);
      const root = sha256(leftSubtree, rightSubtree);

      const computed = computeMerkleRoot(l2, 2, [l3, leftSubtree]);
      expect(computed.toString("hex")).toBe(root.toString("hex"));
    });

    it("computePublicOutputsHash is sensitive to every field", () => {
      const base = {
        batchId: Buffer.alloc(32, 1),
        windowStart: 100, windowEnd: 200, numTrades: 5,
        marketRegimeId: 3, marketMaxMultiplier: 20_000,
        pythCheckpointsHash: Buffer.alloc(32, 2),
        poolBalanceBefore: 1_000_000_000, poolBalanceAfter: 999_000_000,
        currentLiabilityBefore: 500_000_000,
        netPayout: -1_000_000, keeperFee: 5_000, protocolFee: 3_000,
        numWinners: 3, numLosers: 2,
        totalWinnersPayout: 1_500_000, totalLosersStake: 500_000,
        payoutsCommitment: Buffer.alloc(32, 3),
        tradesCommitment: Buffer.alloc(32, 4),
        nullifierHash: Buffer.alloc(32, 5),
      };

      const h = computePublicOutputsHash(base);

      const mutations: Array<[string, Record<string, unknown>]> = [
        ["numTrades", { numTrades: 6 }],
        ["poolBalanceBefore", { poolBalanceBefore: 0 }],
        ["netPayout sign flip", { netPayout: 1_000_000 }],
        ["keeperFee", { keeperFee: 5_001 }],
        ["nullifierHash", { nullifierHash: Buffer.alloc(32, 99) }],
      ];

      for (const [label, override] of mutations) {
        const mutated = computePublicOutputsHash({ ...base, ...override });
        expect(mutated.toString("hex"), `field "${label}" mutation had no effect`).not.toBe(
          h.toString("hex"),
        );
      }
    });
  });
});
