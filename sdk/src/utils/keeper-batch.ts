import { createHash } from 'crypto';

/**
 * Canonical keeper batch id: SHA-256 of
 *   le32(windowStart) || le32(windowEnd) || utf8(marketId)
 *
 * Must match the id the keeper uses when calling `GET /internal/batch/:batchId/data`
 * so `materializePendingBatch` creates the same row the prover and chain expect.
 */
export function computeKeeperBatchId(
  marketId: string,
  windowStart: number,
  windowEnd: number,
): string {
  const marketBytes = Buffer.from(marketId, 'utf8');
  const buf = Buffer.alloc(8 + marketBytes.length);
  buf.writeUInt32LE(windowStart >>> 0, 0);
  buf.writeUInt32LE(windowEnd >>> 0, 4);
  marketBytes.copy(buf, 8);
  return createHash('sha256').update(buf).digest('hex');
}
