//! Export the 896-byte Groth16 verifying-key blob required by `admin_init`.
//!
//! Run: `cargo run --bin vkey`
//!
//! Output is a single hex string (1792 chars = 896 bytes) printed to stdout.
//!
//! Blob layout (big-endian, Solana alt_bn128 format):
//!   [  0.. 32) sp1_vkey_hash — SHA256-based hash of this ELF (program-specific)
//!   [ 32.. 64) vk_root       — SP1 version constant (VK merkle root)
//!   [ 64..128) alpha_g1      — G1, 64 bytes
//!   [128..256) beta_g2       — G2, 128 bytes
//!   [256..384) gamma_g2      — G2, 128 bytes
//!   [384..512) delta_g2      — G2, 128 bytes
//!   [512..576) ic0_g1        — IC[0], G1, 64 bytes
//!   [576..640) ic1_g1        — IC[1], G1, 64 bytes  (sp1_vkey_hash coefficient)
//!   [640..704) ic2_g1        — IC[2], G1, 64 bytes  (public_values_hash coefficient)
//!   [704..768) ic3_g1        — IC[3], G1, 64 bytes  (exit_code coefficient)
//!   [768..832) ic4_g1        — IC[4], G1, 64 bytes  (vk_root coefficient)
//!   [832..896) ic5_g1        — IC[5], G1, 64 bytes  (proof_nonce coefficient)
//!
//! The IC points ([512..896]) are from the SP1 version-level Groth16 VK and are
//! identical for all programs using the same SP1 version. Only sp1_vkey_hash at
//! [0..32] is program-specific.

use anyhow::{anyhow, ensure, Context, Result};
use ark_bn254::{G1Affine, G2Affine};
use ark_serialize::CanonicalSerialize;
use sp1_sdk::{include_elf, Elf, HashableKey, Prover, ProverClient, ProvingKey};
use sp1_verifier::{load_ark_groth16_verifying_key_from_bytes, GROTH16_VK_BYTES, VK_ROOT_BYTES};

const CARNOT_ELF: Elf = include_elf!("carnot-settlement-circuit");

#[tokio::main]
async fn main() -> Result<()> {
    let client = ProverClient::builder().mock().build().await;
    let pk = client
        .setup(CARNOT_ELF)
        .await
        .context("failed to setup ELF for vkey export")?;
    let sp1_vkey_hash: [u8; 32] = pk.verifying_key().bytes32_raw();

    let ark_vk = load_ark_groth16_verifying_key_from_bytes(&GROTH16_VK_BYTES)
        .map_err(|e| anyhow!("failed to decompress Groth16 VK: {e}"))?;

    ensure!(
        ark_vk.gamma_abc_g1.len() == 6,
        "expected 6 IC points for 5 public inputs; got {}",
        ark_vk.gamma_abc_g1.len()
    );

    let mut blob = Vec::with_capacity(896);

    blob.extend_from_slice(&sp1_vkey_hash);
    blob.extend_from_slice(&*VK_ROOT_BYTES);
    blob.extend_from_slice(&g1_to_solana_be(&ark_vk.alpha_g1).context("alpha_g1")?);
    blob.extend_from_slice(&g2_to_solana_be(&ark_vk.beta_g2).context("beta_g2")?);
    blob.extend_from_slice(&g2_to_solana_be(&ark_vk.gamma_g2).context("gamma_g2")?);
    blob.extend_from_slice(&g2_to_solana_be(&ark_vk.delta_g2).context("delta_g2")?);
    for (i, ic) in ark_vk.gamma_abc_g1.iter().enumerate() {
        blob.extend_from_slice(
            &g1_to_solana_be(ic).with_context(|| format!("IC[{i}] g1"))?,
        );
    }

    ensure!(blob.len() == 896, "VK blob must be exactly 896 bytes, got {}", blob.len());
    println!("{}", hex::encode(&blob));
    Ok(())
}

/// Serialize a BN254 G1 affine point to Solana's alt_bn128 big-endian format (64 bytes).
///
/// ark stores coordinates as little-endian; Solana (and gnark) uses big-endian.
/// Format: [x_BE(32), y_BE(32)]
fn g1_to_solana_be(point: &G1Affine) -> Result<[u8; 64]> {
    let mut buf = Vec::with_capacity(64);
    point
        .serialize_uncompressed(&mut buf)
        .map_err(|e| anyhow!("G1 serialize_uncompressed: {e}"))?;
    ensure!(buf.len() == 64, "G1 uncompressed must be 64 bytes, got {}", buf.len());
    let mut x: [u8; 32] = buf[0..32].try_into().expect("length checked");
    let mut y: [u8; 32] = buf[32..64].try_into().expect("length checked");
    x.reverse();
    y.reverse();
    let mut result = [0u8; 64];
    result[0..32].copy_from_slice(&x);
    result[32..64].copy_from_slice(&y);
    Ok(result)
}

/// Serialize a BN254 G2 affine point to Solana's alt_bn128 big-endian format (128 bytes).
///
/// ark stores Fq2 as [c0_LE(32), c1_LE(32)].
/// Solana (and gnark) uses [c1_BE(32), c0_BE(32)] for each coordinate.
/// Format: [x.c1_BE(32), x.c0_BE(32), y.c1_BE(32), y.c0_BE(32)]
fn g2_to_solana_be(point: &G2Affine) -> Result<[u8; 128]> {
    let mut buf = Vec::with_capacity(128);
    point
        .serialize_uncompressed(&mut buf)
        .map_err(|e| anyhow!("G2 serialize_uncompressed: {e}"))?;
    ensure!(buf.len() == 128, "G2 uncompressed must be 128 bytes, got {}", buf.len());
    let mut xc0: [u8; 32] = buf[0..32].try_into().expect("length checked");
    let mut xc1: [u8; 32] = buf[32..64].try_into().expect("length checked");
    let mut yc0: [u8; 32] = buf[64..96].try_into().expect("length checked");
    let mut yc1: [u8; 32] = buf[96..128].try_into().expect("length checked");
    xc0.reverse();
    xc1.reverse();
    yc0.reverse();
    yc1.reverse();
    let mut result = [0u8; 128];
    result[0..32].copy_from_slice(&xc1);
    result[32..64].copy_from_slice(&xc0);
    result[64..96].copy_from_slice(&yc1);
    result[96..128].copy_from_slice(&yc0);
    Ok(result)
}
