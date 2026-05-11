import { config } from "./config";

export function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function isBatchTooSoonLikeMessage(msg: string): boolean {
  return (
    msg.includes("BatchTooSoon") || msg.includes("BATCH_SUBMITTED_TOO_SOON")
  );
}

/** Bearer for Carnot backend `/internal/*` routes. */
export function carnotInternalBearerHeaders(): { Authorization: string } {
  return { Authorization: `Bearer ${config.CARNOT_INTERNAL_API_KEY}` };
}
