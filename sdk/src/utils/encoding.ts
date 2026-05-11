/**
 * Shared hex/bytes encoding helpers used across keeper and backend.
 * All helpers accept hex strings with or without a leading "0x" prefix.
 */

/** Strip a leading "0x" prefix from a hex string, if present. */
export function stripHexPrefix(hex: string): string {
  return hex.startsWith("0x") ? hex.slice(2) : hex;
}

/**
 * Convert a hex string to a Uint8Array.
 * Accepts both "0x…" and bare hex strings.
 */
export function hexToBytes(hex: string): Uint8Array {
  return new Uint8Array(Buffer.from(stripHexPrefix(hex), "hex"));
}

/**
 * Convert a hex string to a Node.js Buffer.
 * Accepts both "0x…" and bare hex strings.
 */
export function hexToBuffer(hex: string): Buffer {
  return Buffer.from(stripHexPrefix(hex), "hex");
}

/**
 * Convert a Buffer or Uint8Array to a "0x"-prefixed hex string.
 */
export function bytesToHex(bytes: Buffer | Uint8Array): string {
  return `0x${Buffer.from(bytes).toString("hex")}`;
}
