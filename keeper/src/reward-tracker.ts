import Database from 'better-sqlite3';
import * as fs from 'fs';
import * as path from 'path';

interface RewardRow {
  batch_id: string;
  fee_micro_usdt: string;
  tx_signature: string;
  earned_at: number;
}

interface TotalRow {
  total: string | null;
}

const DB_PATH =
  process.env.REWARD_DB_PATH ??
  path.join(process.env.HOME ?? '/tmp', '.carnot-keeper', 'rewards.db');

let _db: Database.Database | null = null;

function getDb(): Database.Database {
  if (!_db) {
    fs.mkdirSync(path.dirname(DB_PATH), { recursive: true });
    _db = new Database(DB_PATH);
    _db.exec(`
      CREATE TABLE IF NOT EXISTS rewards (
        batch_id TEXT PRIMARY KEY,
        fee_micro_usdt TEXT NOT NULL,
        tx_signature TEXT NOT NULL,
        earned_at INTEGER NOT NULL
      )
    `);
    process.on('exit', () => _db?.close());
  }
  return _db;
}

export async function recordReward(
  batchId: string,
  feeMicroUsdt: bigint,
  txSignature: string,
): Promise<void> {
  const db = getDb();
  db.prepare(
    'INSERT OR IGNORE INTO rewards (batch_id, fee_micro_usdt, tx_signature, earned_at) VALUES (?, ?, ?, ?)',
  ).run(batchId, feeMicroUsdt.toString(), txSignature, Math.floor(Date.now() / 1000));
}

export function getTotalRewards(): bigint {
  const db = getDb();
  const row = db.prepare('SELECT SUM(CAST(fee_micro_usdt AS INTEGER)) as total FROM rewards').get() as TotalRow | undefined;
  return BigInt(row?.total ?? 0);
}

export function getRecentRewards(limit = 20): Array<{ batchId: string; fee: bigint; txSig: string; earnedAt: number }> {
  const db = getDb();
  const rows = db.prepare('SELECT * FROM rewards ORDER BY earned_at DESC LIMIT ?').all(limit) as RewardRow[];
  return rows.map((r) => ({
    batchId: r.batch_id,
    fee: BigInt(r.fee_micro_usdt),
    txSig: r.tx_signature,
    earnedAt: r.earned_at,
  }));
}

// `node dist/reward-tracker.js` or ts-node: print totals.
if (require.main === module) {
  const total = getTotalRewards();
  console.log(`Total rewards earned: ${total} micro-USDT (${Number(total) / 1_000_000} USDT)`);
  const recent = getRecentRewards();
  if (recent.length) {
    console.log('\nRecent rewards:');
    for (const r of recent) {
      console.log(`  Batch ${r.batchId.slice(0, 8)}... | ${Number(r.fee) / 1_000_000} USDT | ${r.txSig.slice(0, 16)}...`);
    }
  }
}
