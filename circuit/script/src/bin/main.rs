//! Settlement CLI: `--execute` (no proof), `--prove-compressed` (fast, not for chain), `--prove` (Groth16 for Solana).
//! Use `SP1_PROVER=network` + `SP1_PRIVATE_KEY` for faster Groth16 (see Succinct docs). `--out-json` writes keeper JSON.

use anyhow::{anyhow, bail, ensure, Context, Result};
use clap::Parser;
use carnot_lib::{
    compute_payouts_merkle_root, compute_trade_commitment, hash_nullifier_from_sorted_trade_ids,
    hash_payout_leaf, hash_pyth_checkpoints, AggregatorInputs, ChunkOutputs, ChunkProofInput,
    CircuitChunkInputs, CircuitInputs, Direction, N_PYTH_CHECKPOINTS, OhlcTick, PythCheckpoint,
    TradeRecord,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use sp1_sdk::{
    include_elf, Elf, HashableKey, ProveRequest, Prover, ProverClient, ProvingKey,
    SP1ProofWithPublicValues, SP1PublicValues, SP1Stdin,
};
use tokio::task::JoinSet;

const CARNOT_ELF: Elf = include_elf!("carnot-settlement-circuit");
const CARNOT_AGGREGATOR_ELF: Elf = include_elf!("carnot-aggregator-circuit");
const BTC_USD_PYTH_FEED_ID_HEX: &str = "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run all circuit constraints without generating a proof (instant).
    #[arg(long)]
    execute: bool,

    /// Generate an on-chain-ready Groth16 proof.
    /// Slow on CPU (~10-30 min). For GPU speed set SP1_PROVER=network + SP1_PRIVATE_KEY=<key>.
    #[arg(long)]
    prove: bool,

    /// Compressed proof (faster local run; not for on-chain verification).
    #[arg(long = "prove-compressed")]
    prove_compressed: bool,

    /// Write a JSON file for the TypeScript keeper: Groth16 limbs + all public values for `verify_and_settle`.
    #[arg(long, value_name = "PATH")]
    out_json: Option<PathBuf>,

    /// Prove chunks and aggregate into one final proof.
    #[arg(long)]
    chunked: bool,

    /// Number of trades per chunk when using --chunked.
    #[arg(long, default_value_t = 500)]
    chunk_size: usize,

    /// Path to a batch-data JSON file in the backend API format.
    /// When provided, circuit inputs are loaded from this file instead of fixture files.
    /// The file is written by the keeper after fetching from the backend settlement API.
    #[arg(long = "data-json", value_name = "PATH")]
    data_json: Option<PathBuf>,
}

#[derive(Deserialize)]
struct FixtureTrade {
    trade_id: String,
    trader_pubkey: String,
    direction: Direction,
    entry_price: u64,
    exit_price: u64,
    stake_usdt: u64,
    multiplier_bps: u32,
    window_start: i64,
    window_end: i64,
    band_lower: u64,
    band_upper: u64,
    max_payout_usdt: u64,
}

/// JSON `proof_system` field for keeper / on-chain consumers.
#[derive(Debug, Clone, Copy, Serialize)]
enum SettlementProofSystem {
    #[serde(rename = "execute")]
    Execute,
    #[serde(rename = "groth16")]
    Groth16,
    #[serde(rename = "core")]
    Core,
    #[serde(rename = "compressed")]
    Compressed,
}

/// JSON shape for `verify_and_settle` (Solana) — field names match the Anchor instruction args.
#[derive(Debug, Serialize)]
struct SettlementProofJson {
    proof_system: SettlementProofSystem,
    /// When false, `proof_a`/`proof_b`/`proof_c`/`proof_nonce` are absent — do not submit on-chain.
    #[serde(default)]
    suitable_for_onchain: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    proof_a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_b: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_c: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_nonce: Option<String>,

    /// SP1 Groth16 public inputs (hex strings), when `proof_system == "groth16"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    groth16_public_inputs: Option<Vec<String>>,

    public_outputs_hash: String,
    batch_id: String,
    window_start: i64,
    window_end: i64,
    pyth_checkpoints_hash: String,
    net_payout_usdt: i64,
    pool_balance_before: u64,
    pool_balance_after: u64,
    num_trades: u32,
    nullifier_hash: String,
    keeper_fee: u64,
    current_liability_before: u64,
    protocol_fee: u64,
    num_winners: u32,
    num_losers: u32,
    total_winners_payout: u64,
    total_losers_stake: u64,
    market_regime_id: u64,
    payouts_commitment: String,
    trades_commitment: String,
    /// Pyth checkpoint Solana accounts — passed through from the API for the keeper's
    /// `verify_and_settle` remaining accounts.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pyth_checkpoint_accounts: Vec<String>,
}

#[derive(Debug, Clone)]
struct PublicOutputs {
    public_outputs_hash: [u8; 32],
    batch_id: [u8; 32],
    window_start: i64,
    window_end: i64,
    num_trades: u32,
    market_regime_id: u64,
    pyth_checkpoints_hash: [u8; 32],
    pool_balance_before: u64,
    pool_balance_after: u64,
    current_liability_before: u64,
    net_payout: i64,
    keeper_fee: u64,
    protocol_fee: u64,
    num_winners: u32,
    num_losers: u32,
    total_winners_payout: u64,
    total_losers_stake: u64,
    payouts_commitment: [u8; 32],
    trades_commitment: [u8; 32],
    nullifier_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, Serialize)]
enum AggregatedProofSystem {
    #[serde(rename = "aggregated-groth16")]
    AggregatedGroth16,
}

#[derive(Debug, Serialize)]
struct AggregatedProofJson {
    proof_system: AggregatedProofSystem,
    suitable_for_verify_and_settle: bool,
    batch_id: String,
    total_chunks: u32,
    aggregated_public_outputs_hash: String,
    chunk_public_outputs_hashes: Vec<String>,
    proof_a: String,
    proof_b: String,
    proof_c: String,
    proof_nonce: String,
    groth16_public_inputs: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    sp1_sdk::utils::setup_logger();
    // Optional `.env` in cwd — missing file is normal.
    let _ = dotenv::dotenv();

    let args = Args::parse();
    let mode_count = [args.execute, args.prove, args.prove_compressed]
        .iter()
        .filter(|&&b| b)
        .count();
    if mode_count != 1 {
        bail!("specify exactly one of --execute, --prove, or --prove-compressed (see --help)");
    }

    if std::env::var("NETWORK_PRIVATE_KEY").is_err() {
        if let Ok(key) = std::env::var("SP1_PRIVATE_KEY") {
            // SAFETY: before spawning tasks; SP1 reads NETWORK_PRIVATE_KEY.
            unsafe { std::env::set_var("NETWORK_PRIVATE_KEY", key) };
        }
    }

    let client = ProverClient::from_env().await;
    let (inputs, pyth_checkpoint_accounts) = if let Some(ref path) = args.data_json {
        load_circuit_inputs_from_api_json(path).with_context(|| path.display().to_string())?
    } else {
        (load_circuit_inputs().context("load default fixture inputs")?, vec![])
    };
    let expected_outputs =
        compute_public_outputs(&inputs).context("derive expected public outputs from inputs")?;

    if args.chunked {
        if !args.prove {
            bail!("--chunked currently requires --prove to produce a final Groth16 proof");
        }
        run_chunked_prove(&client, &args, inputs).await?;
        return Ok(());
    }

    if args.execute {
        let mut stdin = SP1Stdin::new();
        stdin.write(&inputs);
        let (mut public_values, report) = client
            .execute(CARNOT_ELF, stdin)
            .await
            .context("SP1 execute settlement ELF")?;
        println!("Program executed successfully.");
        let committed_hash = decode_public_outputs_hash(&mut public_values);
        assert_eq!(committed_hash, expected_outputs.public_outputs_hash);
        let outputs = expected_outputs.clone();
        println!("{outputs:#?}");
        println!("Number of cycles: {}", report.total_instruction_count());
        if let Some(path) = args.out_json.as_ref() {
            let mut json = settlement_json_execute(&outputs);
            json.pyth_checkpoint_accounts = pyth_checkpoint_accounts.clone();
            write_json(path, &json).with_context(|| path.display().to_string())?;
        }
    } else if args.prove_compressed {
        println!("Generating compressed proof (fast local mode — not on-chain ready)...");
        let pk = client
            .setup(CARNOT_ELF)
            .await
            .context("setup settlement ELF (compressed prove)")?;
        let mut stdin = SP1Stdin::new();
        stdin.write(&inputs);
        let proof = client
            .prove(&pk, stdin)
            .compressed()
            .await
            .context("generate compressed proof")?;
        println!("Successfully generated compressed proof!");

        let mut pv = proof.public_values.clone();
        let committed_hash = decode_public_outputs_hash(&mut pv);
        assert_eq!(committed_hash, expected_outputs.public_outputs_hash);
        let outputs = expected_outputs.clone();
        println!("{outputs:#?}");

        client
            .verify(&proof, pk.verifying_key(), None)
            .context("verify compressed proof")?;
        println!("Successfully verified compressed proof.");

        if let Some(path) = args.out_json.as_ref() {
            let mut json = settlement_json_compressed(&outputs);
            json.pyth_checkpoint_accounts = pyth_checkpoint_accounts.clone();
            write_json(path, &json).with_context(|| path.display().to_string())?;
        }
    } else {
        let pk = client
            .setup(CARNOT_ELF)
            .await
            .context("setup settlement ELF (Groth16 prove)")?;
        let mut stdin = SP1Stdin::new();
        stdin.write(&inputs);
        let groth16_attempt = client.prove(&pk, stdin).groth16().await;

        match groth16_attempt {
            Ok(proof) => {
                println!("Successfully generated Groth16 proof!");

                let groth16 = proof
                    .proof
                    .try_as_groth_16_ref()
                    .context("proof is not Groth16 (unexpected for .groth16() prove)")?;
                let raw = hex::decode(&groth16.raw_proof).context("decode raw_proof hex")?;
                println!("raw_proof length: {} bytes", raw.len());
                let (proof_a, proof_b, proof_c, proof_nonce) =
                    sp1_groth16_limbs_from_raw(&raw, &groth16.public_inputs)?;

                println!("proof_a    (hex): {}", hex::encode(&proof_a));
                println!("proof_b    (hex): {}", hex::encode(&proof_b));
                println!("proof_c    (hex): {}", hex::encode(&proof_c));
                println!("proof_nonce(hex): {}", hex::encode(&proof_nonce));

                let mut pv = proof.public_values.clone();
                let committed_hash = decode_public_outputs_hash(&mut pv);
                assert_eq!(committed_hash, expected_outputs.public_outputs_hash);
                let outputs = expected_outputs.clone();
                println!("{outputs:#?}");

                client
                    .verify(&proof, pk.verifying_key(), None)
                    .context("verify Groth16 proof")?;
                println!("Successfully verified Groth16 proof.");

                if let Some(path) = args.out_json.as_ref() {
                    let mut json = settlement_json_groth16(
                        &proof_a,
                        &proof_b,
                        &proof_c,
                        &proof_nonce,
                        &groth16.public_inputs,
                        &outputs,
                    );
                    json.pyth_checkpoint_accounts = pyth_checkpoint_accounts.clone();
                    write_json(path, &json).with_context(|| path.display().to_string())?;
                }
            }
            Err(err) => {
                eprintln!("Groth16 proof unavailable ({err}). Falling back to core proof.");
                let mut stdin = SP1Stdin::new();
                stdin.write(&inputs);
                let proof = client
                    .prove(&pk, stdin)
                    .core()
                    .await
                    .context("generate core fallback proof")?;
                println!("Successfully generated core proof.");

                let mut pv = proof.public_values.clone();
                let committed_hash = decode_public_outputs_hash(&mut pv);
                assert_eq!(committed_hash, expected_outputs.public_outputs_hash);
                let outputs = expected_outputs.clone();
                println!("{outputs:#?}");

                client
                    .verify(&proof, pk.verifying_key(), None)
                    .context("verify core fallback proof")?;
                println!("Successfully verified core proof.");

                if let Some(path) = args.out_json.as_ref() {
                    let mut json = settlement_json_core(&outputs);
                    json.pyth_checkpoint_accounts = pyth_checkpoint_accounts.clone();
                    write_json(path, &json).with_context(|| path.display().to_string())?;
                }
            }
        }
    }
    Ok(())
}

fn hex32(b: &[u8; 32]) -> String {
    hex::encode(b)
}

/// First 256 bytes of decoded `raw_proof` are Groth16 limbs A||B||C; `public_inputs[4]` is proof_nonce (hex).
fn sp1_groth16_limbs_from_raw(raw: &[u8], public_inputs: &[String]) -> Result<([u8; 64], [u8; 128], [u8; 64], [u8; 32])> {
    ensure!(
        raw.len() >= 256,
        "raw_proof too short ({} bytes); expected at least 256",
        raw.len()
    );
    let mut proof_a = [0u8; 64];
    let mut proof_b = [0u8; 128];
    let mut proof_c = [0u8; 64];
    proof_a.copy_from_slice(&raw[0..64]);
    proof_b.copy_from_slice(&raw[64..192]);
    proof_c.copy_from_slice(&raw[192..256]);

    let proof_nonce_hex = public_inputs
        .get(4)
        .context("groth16.public_inputs must include index 4 (proof_nonce)")?;
    let stripped = proof_nonce_hex.trim_start_matches("0x");
    let padded = if stripped.len() % 2 != 0 {
        format!("0{stripped}")
    } else {
        stripped.to_string()
    };
    let proof_nonce_bytes = hex::decode(&padded).context("decode proof_nonce hex")?;
    ensure!(
        proof_nonce_bytes.len() <= 32,
        "proof_nonce decodes to {} bytes; max 32",
        proof_nonce_bytes.len()
    );
    let mut proof_nonce = [0u8; 32];
    let start = 32usize.saturating_sub(proof_nonce_bytes.len());
    proof_nonce[start..].copy_from_slice(&proof_nonce_bytes);

    Ok((proof_a, proof_b, proof_c, proof_nonce))
}

fn settlement_json_from_outputs(o: &PublicOutputs) -> (
    String,
    String,
    i64,
    i64,
    String,
    i64,
    u64,
    u64,
    u32,
    String,
    u64,
    u64,
    u64,
    u32,
    u32,
    u64,
    u64,
    u64,
    String,
    String,
) {
    (
        hex32(&o.public_outputs_hash),
        hex32(&o.batch_id),
        o.window_start,
        o.window_end,
        hex32(&o.pyth_checkpoints_hash),
        o.net_payout,
        o.pool_balance_before,
        o.pool_balance_after,
        o.num_trades,
        hex32(&o.nullifier_hash),
        o.keeper_fee,
        o.current_liability_before,
        o.protocol_fee,
        o.num_winners,
        o.num_losers,
        o.total_winners_payout,
        o.total_losers_stake,
        o.market_regime_id,
        hex32(&o.payouts_commitment),
        hex32(&o.trades_commitment),
    )
}

fn settlement_json_execute(o: &PublicOutputs) -> SettlementProofJson {
    let (
        public_outputs_hash,
        batch_id,
        window_start,
        window_end,
        pyth_checkpoints_hash,
        net_payout_usdt,
        pool_balance_before,
        pool_balance_after,
        num_trades,
        nullifier_hash,
        keeper_fee,
        current_liability_before,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        market_regime_id,
        payouts_commitment,
        trades_commitment,
    ) = settlement_json_from_outputs(o);

    SettlementProofJson {
        proof_system: SettlementProofSystem::Execute,
        suitable_for_onchain: false,
        proof_a: None,
        proof_b: None,
        proof_c: None,
        proof_nonce: None,
        groth16_public_inputs: None,
        public_outputs_hash,
        batch_id,
        window_start,
        window_end,
        pyth_checkpoints_hash,
        net_payout_usdt,
        pool_balance_before,
        pool_balance_after,
        num_trades,
        nullifier_hash,
        keeper_fee,
        current_liability_before,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        market_regime_id,
        payouts_commitment,
        trades_commitment,
        pyth_checkpoint_accounts: vec![],
    }
}

fn settlement_json_groth16(
    proof_a: &[u8; 64],
    proof_b: &[u8; 128],
    proof_c: &[u8; 64],
    proof_nonce: &[u8; 32],
    public_inputs: &[String],
    o: &PublicOutputs,
) -> SettlementProofJson {
    let (
        public_outputs_hash,
        batch_id,
        window_start,
        window_end,
        pyth_checkpoints_hash,
        net_payout_usdt,
        pool_balance_before,
        pool_balance_after,
        num_trades,
        nullifier_hash,
        keeper_fee,
        current_liability_before,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        market_regime_id,
        payouts_commitment,
        trades_commitment,
    ) = settlement_json_from_outputs(o);

    SettlementProofJson {
        proof_system: SettlementProofSystem::Groth16,
        suitable_for_onchain: true,
        proof_a: Some(hex::encode(proof_a)),
        proof_b: Some(hex::encode(proof_b)),
        proof_c: Some(hex::encode(proof_c)),
        proof_nonce: Some(hex::encode(proof_nonce)),
        groth16_public_inputs: Some(public_inputs.to_vec()),
        public_outputs_hash,
        batch_id,
        window_start,
        window_end,
        pyth_checkpoints_hash,
        net_payout_usdt,
        pool_balance_before,
        pool_balance_after,
        num_trades,
        nullifier_hash,
        keeper_fee,
        current_liability_before,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        market_regime_id,
        payouts_commitment,
        trades_commitment,
        pyth_checkpoint_accounts: vec![],
    }
}

fn settlement_json_core(o: &PublicOutputs) -> SettlementProofJson {
    let mut j = settlement_json_execute(o);
    j.proof_system = SettlementProofSystem::Core;
    j
}

fn settlement_json_compressed(o: &PublicOutputs) -> SettlementProofJson {
    let mut j = settlement_json_execute(o);
    j.proof_system = SettlementProofSystem::Compressed;
    j
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let pretty = serde_json::to_string_pretty(value).context("serialize settlement JSON")?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create directory {}", parent.display()))?;
        }
    }
    fs::write(path, &pretty).with_context(|| format!("write {}", path.display()))?;
    eprintln!("Wrote JSON to {}", path.display());
    Ok(())
}

#[derive(Debug)]
struct ProvedChunk {
    idx: u32,
    proof: SP1ProofWithPublicValues,
    proof_input: ChunkProofInput,
}

async fn run_chunked_prove(client: &impl Prover, args: &Args, base_inputs: CircuitInputs) -> Result<()> {
    ensure!(args.chunk_size > 0, "--chunk-size must be > 0");
    let chunked_inputs = split_into_chunk_inputs(base_inputs, args.chunk_size)?;
    let total_chunks = chunked_inputs.len() as u32;
    ensure!(total_chunks > 0, "no chunks to prove (empty trades?)");

    println!(
        "Chunked proving enabled: {} chunk(s), chunk_size={}",
        total_chunks, args.chunk_size
    );

    let mut join_set = JoinSet::new();
    for chunk in chunked_inputs {
        join_set.spawn(async move {
            let chunk_idx = chunk.chunk_index;
            let chunk_outputs = compute_public_outputs(&chunk.inner)
                .with_context(|| format!("chunk {chunk_idx}: compute_public_outputs"))?;
            let summary = to_chunk_outputs(&chunk, &chunk_outputs);

            let chunk_client = ProverClient::from_env().await;
            let pk = chunk_client
                .setup(CARNOT_ELF)
                .await
                .with_context(|| format!("chunk {chunk_idx}: setup settlement ELF"))?;
            let mut stdin = SP1Stdin::new();
            stdin.write(&chunk.inner);
            let proof = chunk_client
                .prove(&pk, stdin)
                .compressed()
                .await
                .with_context(|| format!("chunk {chunk_idx}: generate compressed proof"))?;

            let mut pv = proof.public_values.clone();
            let committed_hash = decode_public_outputs_hash(&mut pv);
            ensure!(
                committed_hash == chunk_outputs.public_outputs_hash,
                "chunk {chunk_idx}: committed hash mismatch (guest vs host)"
            );

            let recursion_proof = proof
                .proof
                .try_as_compressed_ref()
                .with_context(|| format!("chunk {chunk_idx}: expected compressed chunk proof"))?;
            let vk_digest = recursion_proof.vk.hash_u32();
            let pv_vec = proof.public_values.hash();
            let pv_digest: [u8; 32] = pv_vec
                .as_slice()
                .try_into()
                .map_err(|_| anyhow!("chunk {chunk_idx}: public values hash must be 32 bytes"))?;

            Ok(ProvedChunk {
                idx: chunk_idx,
                proof,
                proof_input: ChunkProofInput {
                    vk_digest,
                    pv_digest,
                    outputs: summary,
                },
            })
        });
    }

    let mut proved_chunks = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        let chunk = joined.context("chunk proving task panicked or cancelled")??;
        proved_chunks.push(chunk);
    }
    proved_chunks.sort_by_key(|c| c.idx);

    let aggregator_inputs = AggregatorInputs {
        batch_id: proved_chunks[0].proof_input.outputs.batch_id,
        total_chunks,
        chunks: proved_chunks.iter().map(|c| c.proof_input.clone()).collect(),
    };

    let agg_pk = client
        .setup(CARNOT_AGGREGATOR_ELF)
        .await
        .map_err(|e| anyhow!("setup aggregator ELF: {e:?}"))?;
    let mut agg_stdin = SP1Stdin::new();
    agg_stdin.write(&aggregator_inputs);

    for proved_chunk in &proved_chunks {
        let recursion = proved_chunk
            .proof
            .proof
            .try_as_compressed_ref()
            .context("expected compressed proof for aggregation")?
            .clone();
        let vk = recursion.vk.clone();
        agg_stdin.write_proof(*recursion, vk);
    }

    let agg_proof = client
        .prove(&agg_pk, agg_stdin)
        .groth16()
        .await
        .map_err(|e| anyhow!("generate aggregated Groth16 proof: {e:?}"))?;

    client
        .verify(&agg_proof, agg_pk.verifying_key(), None)
        .map_err(|e| anyhow!("verify aggregated Groth16 proof: {e:?}"))?;
    println!("Successfully generated and verified aggregated Groth16 proof.");

    if let Some(path) = args.out_json.as_ref() {
        let groth16 = agg_proof
            .proof
            .try_as_groth_16_ref()
            .context("aggregated proof is not Groth16")?;
        let raw = hex::decode(&groth16.raw_proof).context("decode aggregated raw_proof hex")?;
        let (proof_a, proof_b, proof_c, proof_nonce) =
            sp1_groth16_limbs_from_raw(&raw, &groth16.public_inputs)?;

        let mut agg_pv = agg_proof.public_values.clone();
        let aggregated_public_outputs_hash = decode_public_outputs_hash(&mut agg_pv);
        let json = AggregatedProofJson {
            proof_system: AggregatedProofSystem::AggregatedGroth16,
            suitable_for_verify_and_settle: false,
            batch_id: hex::encode(aggregator_inputs.batch_id),
            total_chunks: aggregator_inputs.total_chunks,
            aggregated_public_outputs_hash: hex::encode(aggregated_public_outputs_hash),
            chunk_public_outputs_hashes: aggregator_inputs
                .chunks
                .iter()
                .map(|c| hex::encode(c.outputs.public_outputs_hash))
                .collect(),
            proof_a: hex::encode(proof_a),
            proof_b: hex::encode(proof_b),
            proof_c: hex::encode(proof_c),
            proof_nonce: hex::encode(proof_nonce),
            groth16_public_inputs: groth16.public_inputs.to_vec(),
        };
        write_json(path, &json).with_context(|| path.display().to_string())?;
    }
    Ok(())
}

fn split_into_chunk_inputs(base_inputs: CircuitInputs, chunk_size: usize) -> Result<Vec<CircuitChunkInputs>> {
    let total_chunks = base_inputs.trades.len().div_ceil(chunk_size) as u32;
    let mut out = Vec::with_capacity(total_chunks as usize);

    let mut rolling_pool_balance = base_inputs.pool_balance_before;
    let mut offset = 0usize;
    let mut chunk_index = 0u32;
    while offset < base_inputs.trades.len() {
        let end = (offset + chunk_size).min(base_inputs.trades.len());
        let trades = base_inputs.trades[offset..end].to_vec();
        let trade_commitments = trades.iter().map(compute_trade_commitment).collect();
        let mut sorted_trade_ids: Vec<[u8; 32]> = trades.iter().map(|t| t.trade_id).collect();
        sorted_trade_ids.sort();
        let current_liability_before = trades.iter().map(|t| t.max_payout_usdt).sum();

        let chunk_input = CircuitInputs {
            trades,
            trade_commitments,
            sorted_trade_ids,
            ohlc: base_inputs.ohlc.clone(),
            batch_id: base_inputs.batch_id,
            window_start: base_inputs.window_start,
            window_end: base_inputs.window_end,
            pool_balance_before: rolling_pool_balance,
            current_liability_before,
            keeper_fee_bps: base_inputs.keeper_fee_bps,
            protocol_fee_bps: base_inputs.protocol_fee_bps,
            market_max_multiplier: base_inputs.market_max_multiplier,
            market_regime_id: base_inputs.market_regime_id,
            pyth_feed_id: base_inputs.pyth_feed_id,
            pyth_checkpoints: base_inputs.pyth_checkpoints.clone(),
        };

        let chunk_outputs = compute_public_outputs(&chunk_input)
            .with_context(|| format!("split_into_chunk_inputs: chunk_index {chunk_index}"))?;
        rolling_pool_balance = chunk_outputs.pool_balance_after;

        out.push(CircuitChunkInputs {
            chunk_index,
            total_chunks,
            inner: chunk_input,
        });

        chunk_index += 1;
        offset = end;
    }

    Ok(out)
}

fn to_chunk_outputs(chunk: &CircuitChunkInputs, outputs: &PublicOutputs) -> ChunkOutputs {
    ChunkOutputs {
        chunk_index: chunk.chunk_index,
        total_chunks: chunk.total_chunks,
        batch_id: outputs.batch_id,
        window_start: outputs.window_start,
        window_end: outputs.window_end,
        num_trades: outputs.num_trades,
        net_payout: outputs.net_payout,
        pool_balance_before: outputs.pool_balance_before,
        pool_balance_after: outputs.pool_balance_after,
        current_liability_before: outputs.current_liability_before,
        keeper_fee: outputs.keeper_fee,
        protocol_fee: outputs.protocol_fee,
        num_winners: outputs.num_winners,
        num_losers: outputs.num_losers,
        total_winners_payout: outputs.total_winners_payout,
        total_losers_stake: outputs.total_losers_stake,
        market_regime_id: outputs.market_regime_id,
        payouts_commitment: outputs.payouts_commitment,
        trades_commitment: outputs.trades_commitment,
        nullifier_hash: outputs.nullifier_hash,
        public_outputs_hash: outputs.public_outputs_hash,
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Backend may emit numeric fields as JSON numbers or quoted decimal strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum JsonU64 {
    Num(u64),
    Str(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonI64 {
    Num(i64),
    Str(String),
}

fn de_str_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match JsonU64::deserialize(d)? {
        JsonU64::Num(n) => Ok(n),
        JsonU64::Str(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

fn de_str_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match JsonI64::deserialize(d)? {
        JsonI64::Num(n) => Ok(n),
        JsonI64::Str(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiTrade {
    trade_id: String,
    trader_pubkey: String,
    direction: Direction,
    #[serde(deserialize_with = "de_str_u64")]
    entry_price: u64,
    #[serde(deserialize_with = "de_str_u64")]
    exit_price: u64,
    #[serde(deserialize_with = "de_str_u64")]
    stake_usdt: u64,
    multiplier_bps: u32,
    window_start: i64,
    window_end: i64,
    #[serde(deserialize_with = "de_str_u64")]
    band_lower: u64,
    #[serde(deserialize_with = "de_str_u64")]
    band_upper: u64,
    #[serde(deserialize_with = "de_str_u64")]
    max_payout_usdt: u64,
}

#[derive(serde::Deserialize)]
struct ApiOhlcTick {
    ts: i64,
    #[serde(deserialize_with = "de_str_u64")]
    open: u64,
    #[serde(deserialize_with = "de_str_u64")]
    high: u64,
    #[serde(deserialize_with = "de_str_u64")]
    low: u64,
    #[serde(deserialize_with = "de_str_u64")]
    close: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPythCheckpoint {
    #[serde(deserialize_with = "de_str_i64")]
    price: i64,
    #[serde(deserialize_with = "de_str_u64")]
    conf: u64,
    exponent: i32,
    publish_time: i64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiBatchData {
    batch_id: String,
    window_start: i64,
    window_end: i64,
    trades: Vec<ApiTrade>,
    ohlc: Vec<ApiOhlcTick>,
    #[serde(deserialize_with = "de_str_u64")]
    pool_balance_before: u64,
    #[serde(deserialize_with = "de_str_u64")]
    current_liability_before: u64,
    keeper_fee_bps: u64,
    protocol_fee_bps: u64,
    market_max_multiplier: u32,
    market_regime_id: u64,
    pyth_feed_id: String,
    pyth_checkpoints: Vec<ApiPythCheckpoint>,
    #[serde(default)]
    pyth_checkpoint_accounts: Vec<String>,
}

/// Load `--data-json` (batch payload + checkpoint account pubkeys for the keeper).
fn load_circuit_inputs_from_api_json(path: &PathBuf) -> Result<(CircuitInputs, Vec<String>)> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read --data-json {}", path.display()))?;
    let api: ApiBatchData =
        serde_json::from_str(&raw).context("parse --data-json as ApiBatchData")?;

    fn strip(s: &str) -> &str {
        s.strip_prefix("0x").unwrap_or(s)
    }

    let mut trades: Vec<TradeRecord> = api
        .trades
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Ok(TradeRecord {
                trade_id: decode_hex_32(strip(&t.trade_id))
                    .with_context(|| format!("trades[{i}].trade_id"))?,
                trader_pubkey: decode_hex_32(strip(&t.trader_pubkey))
                    .with_context(|| format!("trades[{i}].trader_pubkey"))?,
                direction: t.direction,
                entry_price: t.entry_price,
                exit_price: t.exit_price,
                stake_usdt: t.stake_usdt,
                multiplier_bps: t.multiplier_bps,
                window_start: t.window_start,
                window_end: t.window_end,
                band_lower: t.band_lower,
                band_upper: t.band_upper,
                max_payout_usdt: t.max_payout_usdt,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    trades.sort_by(|a, b| {
        a.trader_pubkey
            .cmp(&b.trader_pubkey)
            .then_with(|| a.trade_id.cmp(&b.trade_id))
    });
    let mut sorted_trade_ids: Vec<[u8; 32]> = trades.iter().map(|t| t.trade_id).collect();
    sorted_trade_ids.sort();
    let trade_commitments = trades.iter().map(compute_trade_commitment).collect();

    let ohlc: Vec<OhlcTick> = api
        .ohlc
        .iter()
        .map(|o| OhlcTick { ts: o.ts, open: o.open, high: o.high, low: o.low, close: o.close })
        .collect();

    let pyth_checkpoints: Vec<PythCheckpoint> = api
        .pyth_checkpoints
        .iter()
        .map(|cp| PythCheckpoint {
            price: cp.price,
            conf: cp.conf,
            exponent: cp.exponent,
            publish_time: cp.publish_time,
        })
        .collect();

    let current_liability_before = api.current_liability_before;

    let inputs = CircuitInputs {
        trades,
        trade_commitments,
        sorted_trade_ids,
        ohlc,
        batch_id: decode_hex_32(strip(&api.batch_id)).context("batch_id hex")?,
        window_start: api.window_start,
        window_end: api.window_end,
        pool_balance_before: api.pool_balance_before,
        current_liability_before,
        keeper_fee_bps: api.keeper_fee_bps,
        protocol_fee_bps: api.protocol_fee_bps,
        market_max_multiplier: api.market_max_multiplier,
        market_regime_id: api.market_regime_id,
        pyth_feed_id: decode_hex_32(strip(&api.pyth_feed_id)).context("pyth_feed_id hex")?,
        pyth_checkpoints,
    };

    Ok((inputs, api.pyth_checkpoint_accounts))
}

fn load_circuit_inputs() -> Result<CircuitInputs> {
    let mut trades = load_sample_trades()?;
    trades.sort_by(|a, b| {
        a.trader_pubkey
            .cmp(&b.trader_pubkey)
            .then_with(|| a.trade_id.cmp(&b.trade_id))
    });
    let mut sorted_trade_ids: Vec<[u8; 32]> = trades.iter().map(|t| t.trade_id).collect();
    sorted_trade_ids.sort();
    let trade_commitments = trades.iter().map(compute_trade_commitment).collect();
    let current_liability_before = trades.iter().map(|t| t.max_payout_usdt).sum();

    Ok(CircuitInputs {
        trades,
        trade_commitments,
        sorted_trade_ids,
        ohlc: load_sample_ohlc()?,
        batch_id: [0xaa; 32],
        window_start: 1000,
        window_end: 1008,
        pool_balance_before: 10_000_000,
        current_liability_before,
        keeper_fee_bps: 100,
        protocol_fee_bps: 200,
        market_max_multiplier: 20_000,
        market_regime_id: 7,
        pyth_feed_id: decode_hex_32(BTC_USD_PYTH_FEED_ID_HEX).context("fixture pyth_feed_id")?,
        // Fixture checkpoints: expo -8, values chosen to sit near sample OHLC closes within B6 bound.
        pyth_checkpoints: vec![
            PythCheckpoint { price: 102_000, conf: 50, exponent: -8, publish_time: 1000 },
            PythCheckpoint { price: 102_200, conf: 50, exponent: -8, publish_time: 1004 },
            PythCheckpoint { price:  90_000, conf: 50, exponent: -8, publish_time: 1008 },
        ],
    })
}

fn load_sample_trades() -> Result<Vec<TradeRecord>> {
    let path = fixtures_dir().join("sample_trades.json");
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let fixture: Vec<FixtureTrade> =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;

    fixture
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            Ok(TradeRecord {
                trade_id: decode_hex_32(&t.trade_id)
                    .with_context(|| format!("fixture trades[{i}].trade_id"))?,
                trader_pubkey: decode_hex_32(&t.trader_pubkey)
                    .with_context(|| format!("fixture trades[{i}].trader_pubkey"))?,
                direction: t.direction,
                entry_price: t.entry_price,
                exit_price: t.exit_price,
                stake_usdt: t.stake_usdt,
                multiplier_bps: t.multiplier_bps,
                window_start: t.window_start,
                window_end: t.window_end,
                band_lower: t.band_lower,
                band_upper: t.band_upper,
                max_payout_usdt: t.max_payout_usdt,
            })
        })
        .collect::<Result<Vec<_>>>()
}

fn load_sample_ohlc() -> Result<Vec<OhlcTick>> {
    let path = fixtures_dir().join("sample_ohlc.json");
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str::<Vec<OhlcTick>>(&raw).with_context(|| format!("parse {}", path.display()))
}

fn decode_public_outputs_hash(public_values: &mut SP1PublicValues) -> [u8; 32] {
    public_values.read()
}

fn compute_public_outputs(inputs: &CircuitInputs) -> Result<PublicOutputs> {
    let trades = &inputs.trades;
    let trade_commitments = &inputs.trade_commitments;
    let sorted_trade_ids = &inputs.sorted_trade_ids;
    let ohlc = &inputs.ohlc;

    let num_trades = trades.len();
    assert!(!trades.is_empty());
    assert_eq!(trade_commitments.len(), num_trades);
    assert_eq!(sorted_trade_ids.len(), num_trades);

    if !ohlc.is_empty() {
        assert!(ohlc[0].ts <= inputs.window_start);
        assert!(ohlc[ohlc.len() - 1].ts >= inputs.window_end);
    }

    assert_eq!(
        inputs.pyth_checkpoints.len(),
        N_PYTH_CHECKPOINTS,
        "expected {} Pyth checkpoints (circuit B4)",
        N_PYTH_CHECKPOINTS
    );

    for i in 0..ohlc.len() {
        let tick = &ohlc[i];
        if i > 0 {
            assert!(ohlc[i - 1].ts < tick.ts);
        }
        assert!(tick.low <= tick.open);
        assert!(tick.low <= tick.close);
        assert!(tick.high >= tick.open);
        assert!(tick.high >= tick.close);
        assert!(tick.low <= tick.high);
    }

    let pyth_checkpoints_hash = hash_pyth_checkpoints(&inputs.pyth_checkpoints);

    for pair in sorted_trade_ids.windows(2) {
        assert!(pair[0] < pair[1]);
    }

    let mut trades_id_xor = [0u8; 32];
    for t in trades {
        for j in 0..32 {
            trades_id_xor[j] ^= t.trade_id[j];
        }
    }
    let mut sorted_id_xor = [0u8; 32];
    for id in sorted_trade_ids {
        for j in 0..32 {
            sorted_id_xor[j] ^= id[j];
        }
    }
    assert_eq!(trades_id_xor, sorted_id_xor);

    for pair in trades.windows(2) {
        assert!(pair[0].trader_pubkey <= pair[1].trader_pubkey);
    }

    let start_idx = ohlc.partition_point(|tick| tick.ts < inputs.window_start);
    let end_idx = ohlc.partition_point(|tick| tick.ts <= inputs.window_end);
    assert!(start_idx <= end_idx);
    assert!(end_idx <= ohlc.len());
    if start_idx > 0 {
        assert!(ohlc[start_idx - 1].ts < inputs.window_start);
    }
    if start_idx < ohlc.len() {
        assert!(ohlc[start_idx].ts >= inputs.window_start);
    }
    if end_idx > 0 {
        assert!(ohlc[end_idx - 1].ts <= inputs.window_end);
    }
    if end_idx < ohlc.len() {
        assert!(ohlc[end_idx].ts > inputs.window_end);
    }
    let mut trades_commitment_hasher = Sha256::new();
    let mut payout_leaves = Vec::with_capacity(num_trades);
    let mut num_winners: u32 = 0;
    let mut num_losers: u32 = 0;
    let mut total_winners_payout: u64 = 0;
    let mut total_losers_stake: u64 = 0;
    let mut net_payout: i64 = 0;
    let mut settled_liability: u64 = 0;

    for (trade, &stored_commitment) in trades.iter().zip(trade_commitments) {
        let computed = compute_trade_commitment(trade);
        assert_eq!(computed, stored_commitment);
        assert!(trade.multiplier_bps >= 10_000);
        assert!(trade.multiplier_bps <= inputs.market_max_multiplier);
        assert!(trade.window_start < trade.window_end);
        assert!(trade.window_start >= inputs.window_start);
        assert!(trade.window_end <= inputs.window_end);
        assert!(trade.stake_usdt > 0);

        trades_commitment_hasher.update(&stored_commitment);

        let gross_payout: u64 = (trade.stake_usdt as u128)
            .checked_mul(trade.multiplier_bps as u128)
            .context("C4: stake * multiplier overflow")?
            .checked_div(10_000)
            .context("C4: gross payout divide by BPS")?
            .try_into()
            .map_err(|_| anyhow!("C4: gross payout exceeds u64"))?;
        assert_eq!(gross_payout, trade.max_payout_usdt);
        settled_liability = settled_liability
            .checked_add(trade.max_payout_usdt)
            .context("C4: settled_liability accumulator overflow")?;

        let settle_idx = ohlc.partition_point(|t| t.ts <= trade.window_end);
        let settlement_close = if settle_idx == 0 {
            ohlc[0].close
        } else {
            ohlc[settle_idx - 1].close
        };
        let won = carnot_lib::verify_trade_outcome_close_in_band(trade, settlement_close);

        let payout: u64;
        if won {
            let gain = gross_payout
                .checked_sub(trade.stake_usdt)
                .context("C: winner gross below stake")?;
            net_payout = net_payout
                .checked_add(gain as i64)
                .context("C: net_payout accumulator overflow")?;
            total_winners_payout = total_winners_payout
                .checked_add(gross_payout)
                .context("C: total_winners_payout overflow")?;
            num_winners += 1;
            payout = gross_payout;
        } else {
            net_payout = net_payout
                .checked_sub(trade.stake_usdt as i64)
                .context("C: net_payout accumulator underflow")?;
            total_losers_stake = total_losers_stake
                .checked_add(trade.stake_usdt)
                .context("C: total_losers_stake overflow")?;
            num_losers += 1;
            payout = 0;
        }

        let payout_index = payout_leaves.len() as u32;
        payout_leaves.push(hash_payout_leaf(
            &inputs.batch_id,
            &trade.trade_id,
            &trade.trader_pubkey,
            payout,
            payout_index,
        ));
    }
    // Mirrors guest: liability sum is not yet wired into host JSON (Copy — read for side-effect clarity).
    let _ = settled_liability;

    let trades_commitment: [u8; 32] = trades_commitment_hasher.finalize().into();
    let payouts_commitment = compute_payouts_merkle_root(&payout_leaves);

    let net_abs = net_payout.unsigned_abs();
    let keeper_fee: u64 = (net_abs as u128)
        .checked_mul(inputs.keeper_fee_bps as u128)
        .context("D3: |net_payout| * keeper_fee_bps overflow")?
        .checked_div(10_000)
        .context("D3: keeper fee divide by BPS")?
        .try_into()
        .map_err(|_| anyhow!("D3: keeper fee exceeds u64"))?;

    let protocol_fee: u64 = if net_payout < 0 {
        (net_abs as u128)
            .checked_mul(inputs.protocol_fee_bps as u128)
            .context("D4: |net_payout| * protocol_fee_bps overflow")?
            .checked_div(10_000)
            .context("D4: protocol fee divide by BPS")?
            .try_into()
            .map_err(|_| anyhow!("D4: protocol fee exceeds u64"))?
    } else {
        0
    };

    let pool_balance_after: u64 = if net_payout >= 0 {
        inputs
            .pool_balance_before
            .checked_sub(net_payout as u64)
            .context("D1: pool_balance_after underflow (net payout)")?
            .checked_sub(keeper_fee)
            .context("D1: pool_balance_after underflow (keeper fee)")?
    } else {
        inputs
            .pool_balance_before
            .checked_add(net_abs)
            .context("D1: pool_balance_after overflow (LP pays winners)")?
            .checked_sub(keeper_fee)
            .context("D1: pool_balance_after underflow (keeper fee on LP loss)")?
    };

    assert_eq!((num_winners + num_losers) as usize, num_trades);

    let nullifier_hash = hash_nullifier_from_sorted_trade_ids(&inputs.batch_id, sorted_trade_ids);

    let public_outputs_hash: [u8; 32] = {
        let mut h = Sha256::new();
        h.update(&inputs.batch_id);
        h.update(&inputs.window_start.to_le_bytes());
        h.update(&inputs.window_end.to_le_bytes());
        h.update(&(num_trades as u32).to_le_bytes());
        h.update(&inputs.market_regime_id.to_le_bytes());
        h.update(&inputs.market_max_multiplier.to_le_bytes());
        h.update(&pyth_checkpoints_hash);
        h.update(&inputs.pool_balance_before.to_le_bytes());
        h.update(&pool_balance_after.to_le_bytes());
        h.update(&inputs.current_liability_before.to_le_bytes());
        h.update(&net_payout.to_le_bytes());
        h.update(&keeper_fee.to_le_bytes());
        h.update(&protocol_fee.to_le_bytes());
        h.update(&num_winners.to_le_bytes());
        h.update(&num_losers.to_le_bytes());
        h.update(&total_winners_payout.to_le_bytes());
        h.update(&total_losers_stake.to_le_bytes());
        h.update(&payouts_commitment);
        h.update(&trades_commitment);
        h.update(&nullifier_hash);
        h.finalize().into()
    };

    Ok(PublicOutputs {
        public_outputs_hash,
        batch_id: inputs.batch_id,
        window_start: inputs.window_start,
        window_end: inputs.window_end,
        num_trades: num_trades as u32,
        market_regime_id: inputs.market_regime_id,
        pyth_checkpoints_hash,
        pool_balance_before: inputs.pool_balance_before,
        pool_balance_after,
        current_liability_before: inputs.current_liability_before,
        net_payout,
        keeper_fee,
        protocol_fee,
        num_winners,
        num_losers,
        total_winners_payout,
        total_losers_stake,
        payouts_commitment,
        trades_commitment,
        nullifier_hash,
    })
}

fn decode_hex_32(raw: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(raw).with_context(|| format!("invalid hex '{raw}'"))?;
    ensure!(
        bytes.len() == 32,
        "hex must decode to 32 bytes (got {}): '{raw}'",
        bytes.len()
    );
    let mut out = [0_u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
