/** Groth16 proof points (BN254 curve). */
export interface Groth16Proof {
  /** G1 point (32 bytes each). */
  a: [Uint8Array, Uint8Array];
  /** G2 point (32 bytes each, 2 per coordinate). */
  b: [[Uint8Array, Uint8Array], [Uint8Array, Uint8Array]];
  /** G1 point. */
  c: [Uint8Array, Uint8Array];
}
