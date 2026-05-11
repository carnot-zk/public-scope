//! Persistent prover service for `carnot-keeper`.
//!
//! - Keeps SP1 setup keys warm in-memory for continuous proving.
//! - Accepts JSON requests over stdin (one request per line) and emits JSON responses on stdout.
//! - Supports one-shot mode via `--input-json` / `--out-json`.
//!
//! Request format (JSON line):
//! `{ "id": "optional-id", "action": "prove", "batchData": { ...keeper payload... } }`
//!
//! Health format:
//! `{ "id": "optional-id", "action": "health" }`

use anyhow::{anyhow, Context, Result};
use carnot_lib::{
    compute_trade_commitment, CircuitInputs, Direction, OhlcTick, PythCheckpoint, TradeRecord,
    N_PYTH_CHECKPOINTS,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sp1_sdk::{
    env::{EnvProver, EnvProvingKey},
    include_elf, Elf, ProveRequest, Prover, ProverClient, ProvingKey, SP1PublicValues, SP1Stdin,
};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

const CARNOT_ELF: Elf = include_elf!("carnot-settlement-circuit");

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Persistent Carnot keeper prover")]
struct Args {
    /// Process a single input JSON file and exit.
    #[arg(long, value_name = "PATH")]
    input_json: Option<PathBuf>,

    /// Optional output file for one-shot mode. If omitted, result is printed to stdout.
    #[arg(long, value_name = "PATH")]
    out_json: Option<PathBuf>,

    /// Maximum prove attempts before returning an error.
    #[arg(long, default_value_t = 3)]
    max_retries: u32,

    /// Base exponential backoff delay in milliseconds.
    #[arg(long, default_value_t = 2_000)]
    retry_base_delay_ms: u64,

    /// Max backoff cap in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    retry_max_delay_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IncomingRequest {
    Wrapped(ProverRequest),
    Direct(KeeperBatchDataWire),
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum ProverAction {
    #[default]
    Prove,
    Health,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProverRequest {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    action: ProverAction,
    #[serde(default)]
    batch_data: Option<KeeperBatchDataWire>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ProverResultBody {
    Health(HealthPayload),
    Proof(KeeperProofResult),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProverResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ProverResultBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeeperProofResult {
    proof: Groth16ProofHex,
    proof_nonce: String,
    public_outputs: KeeperPublicOutputsWire,
    pyth_checkpoint_accounts: Vec<String>,
    public_outputs_hash_committed: String,
    attempts: u32,
    duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Groth16ProofHex {
    a: [String; 2],
    b: [[String; 2]; 2],
    c: [String; 2],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthPayload {
    uptime_ms: u64,
    processed: u64,
    succeeded: u64,
    failed: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeeperBatchDataWire {
    batch_id: String,
    window_start: i64,
    window_end: i64,
    trades: Vec<KeeperTradeRecordWire>,
    trade_commitments: Vec<String>,
    sorted_trade_ids: Vec<String>,
    ohlc: Vec<KeeperOhlcTickWire>,
    pool_balance_before: String,
    current_liability_before: String,
    keeper_fee_bps: u64,
    protocol_fee_bps: u64,
    market_max_multiplier: u32,
    market_regime_id: u64,
    pyth_feed_id: String,
    pyth_checkpoints: Vec<PythCheckpointWire>,
    #[serde(default)]
    pyth_checkpoint_accounts: Vec<String>,
    #[serde(default)]
    expected_public_outputs: Option<KeeperPublicOutputsWire>,
    #[serde(default)]
    computed: Option<KeeperPublicOutputsWire>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeeperTradeRecordWire {
    trade_id: String,
    trader_pubkey: String,
    direction: Direction,
    entry_price: String,
    exit_price: String,
    stake_usdt: String,
    multiplier_bps: u32,
    window_start: i64,
    window_end: i64,
    band_lower: String,
    band_upper: String,
    max_payout_usdt: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeeperOhlcTickWire {
    ts: i64,
    open: String,
    high: String,
    low: String,
    close: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireI64 {
    Num(i64),
    Str(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireU64 {
    Num(u64),
    Str(String),
}

fn de_wire_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match WireI64::deserialize(d)? {
        WireI64::Num(n) => Ok(n),
        WireI64::Str(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

fn de_wire_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match WireU64::deserialize(d)? {
        WireU64::Num(n) => Ok(n),
        WireU64::Str(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PythCheckpointWire {
    #[serde(deserialize_with = "de_wire_i64")]
    price: i64,
    #[serde(deserialize_with = "de_wire_u64")]
    conf: u64,
    exponent: i32,
    publish_time: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeeperPublicOutputsWire {
    batch_id: String,
    window_start: i64,
    window_end: i64,
    num_trades: u32,
    market_regime_id: u64,
    net_payout_usdt: String,
    pool_balance_before: String,
    pool_balance_after: String,
    current_liability_before: String,
    keeper_fee: String,
    protocol_fee: String,
    num_winners: u32,
    num_losers: u32,
    total_winners_payout: String,
    total_losers_stake: String,
    payouts_commitment: String,
    trades_commitment: String,
    nullifier_hash: String,
    pyth_checkpoints_hash: String,
    public_outputs_hash: String,
}

#[derive(Debug, Default)]
struct ServiceMetrics {
    processed: u64,
    succeeded: u64,
    failed: u64,
}

struct ProverService {
    client: EnvProver,
    pk: EnvProvingKey,
    args: Args,
    started_at: Instant,
    metrics: ServiceMetrics,
}

impl ProverService {
    async fn new(args: Args) -> Result<Self> {
        let client = ProverClient::from_env().await;
        let pk = client
            .setup(CARNOT_ELF)
            .await
            .context("failed to setup settlement ELF")?;
        Ok(Self {
            client,
            pk,
            args,
            started_at: Instant::now(),
            metrics: ServiceMetrics::default(),
        })
    }

    fn health(&self) -> HealthPayload {
        HealthPayload {
            uptime_ms: self.started_at.elapsed().as_millis() as u64,
            processed: self.metrics.processed,
            succeeded: self.metrics.succeeded,
            failed: self.metrics.failed,
        }
    }

    async fn prove_with_retry(&mut self, batch: KeeperBatchDataWire) -> Result<KeeperProofResult> {
        self.metrics.processed += 1;
        let started = Instant::now();

        let (inputs, expected_outputs, expected_hash) = batch_to_circuit_inputs(batch.clone())?;
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 1..=self.args.max_retries {
            match self.prove_once(&inputs, &expected_hash, &expected_outputs, &batch, attempt).await {
                Ok(mut result) => {
                    result.duration_ms = started.elapsed().as_millis() as u64;
                    self.metrics.succeeded += 1;
                    return Ok(result);
                }
                Err(err) => {
                    eprintln!("[keeper-prover] attempt {attempt}/{} error: {err:#}", self.args.max_retries);
                    last_error = Some(err);
                    if attempt < self.args.max_retries {
                        let delay_ms = backoff_delay_ms(
                            attempt,
                            self.args.retry_base_delay_ms,
                            self.args.retry_max_delay_ms,
                        );
                        eprintln!(
                            "[keeper-prover] retrying in {delay_ms}ms..."
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        self.metrics.failed += 1;
        Err(match last_error {
            Some(e) => e,
            None => anyhow!(
                "proving failed with no error recorded (max_retries={})",
                self.args.max_retries
            ),
        })
    }

    async fn prove_once(
        &self,
        inputs: &CircuitInputs,
        expected_hash: &[u8; 32],
        expected_outputs: &KeeperPublicOutputsWire,
        batch: &KeeperBatchDataWire,
        attempt: u32,
    ) -> Result<KeeperProofResult> {
        let mut stdin = SP1Stdin::new();
        stdin.write(inputs);

        let proof = self
            .client
            .prove(&self.pk, stdin)
            .groth16()
            .await
            .context("failed to generate Groth16 proof")?;

        self.client
            .verify(&proof, self.pk.verifying_key(), None)
            .context("failed to verify Groth16 proof")?;

        let mut pv = proof.public_values.clone();
        let committed_hash = decode_public_outputs_hash(&mut pv);
        if committed_hash != *expected_hash {
            return Err(anyhow!(
                "public outputs hash mismatch: committed={}, expected={}",
                hex::encode(committed_hash),
                hex::encode(expected_hash),
            ));
        }

        let groth16 = proof
            .proof
            .try_as_groth_16_ref()
            .context("proof is not Groth16")?;
        let raw = hex::decode(&groth16.raw_proof).context("invalid groth16.raw_proof hex")?;
        if raw.len() < 256 {
            return Err(anyhow!("raw proof too short: {} bytes", raw.len()));
        }
        let proof_nonce = decode_proof_nonce(groth16.public_inputs.get(4))?;

        if batch.pyth_checkpoint_accounts.len() != N_PYTH_CHECKPOINTS {
            return Err(anyhow!(
                "batch payload must include exactly {} pythCheckpointAccounts entries; got {}",
                N_PYTH_CHECKPOINTS,
                batch.pyth_checkpoint_accounts.len()
            ));
        }

        Ok(KeeperProofResult {
            proof: split_groth16_hex(&raw),
            proof_nonce: hex::encode(proof_nonce),
            public_outputs: expected_outputs.clone(),
            pyth_checkpoint_accounts: batch.pyth_checkpoint_accounts.clone(),
            public_outputs_hash_committed: hex::encode(committed_hash),
            attempts: attempt,
            duration_ms: 0,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    sp1_sdk::utils::setup_logger();
    dotenv::dotenv().ok();

    if std::env::var("NETWORK_PRIVATE_KEY").is_err() {
        if let Ok(key) = std::env::var("SP1_PRIVATE_KEY") {
            // SAFETY: before background tasks; SP1 reads NETWORK_PRIVATE_KEY.
            unsafe { std::env::set_var("NETWORK_PRIVATE_KEY", key) };
        }
    }

    let args = Args::parse();
    let mut service = ProverService::new(args.clone()).await?;

    if let Some(input_path) = args.input_json.as_ref() {
        let raw = fs::read_to_string(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?;
        let incoming: IncomingRequest = serde_json::from_str(&raw).context("invalid input JSON")?;
        let response = handle_incoming(&mut service, incoming).await;

        if let Some(path) = args.out_json.as_ref() {
            write_json(path, &response)?;
        } else {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        return Ok(());
    }

    eprintln!("[keeper-prover] ready: warm proving key loaded, waiting for stdin JSON lines");
    run_stdio_server(service).await
}

async fn run_stdio_server(mut service: ProverService) -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = io::stdout();

    while let Some(line) = reader.next_line().await.context("stdin read failure")? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<IncomingRequest>(trimmed) {
            Ok(request) => handle_incoming(&mut service, request).await,
            Err(err) => ProverResponse {
                id: None,
                status: "error".to_string(),
                result: None,
                error: Some(format!("invalid request JSON: {err}")),
            },
        };

        let json = serde_json::to_string(&response).context("failed to serialize response")?;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle_incoming(service: &mut ProverService, incoming: IncomingRequest) -> ProverResponse {
    match incoming {
        IncomingRequest::Wrapped(req) => {
            let id = req.id;
            if req.action == ProverAction::Health {
                let payload = service.health();
                return ProverResponse {
                    id,
                    status: "ok".to_string(),
                    result: Some(ProverResultBody::Health(payload)),
                    error: None,
                };
            }

            match req.batch_data {
                Some(batch) => match service.prove_with_retry(batch).await {
                    Ok(result) => ProverResponse {
                        id,
                        status: "ok".to_string(),
                        result: Some(ProverResultBody::Proof(result)),
                        error: None,
                    },
                    Err(err) => ProverResponse {
                        id,
                        status: "error".to_string(),
                        result: None,
                        error: Some(err.to_string()),
                    },
                },
                None => ProverResponse {
                    id,
                    status: "error".to_string(),
                    result: None,
                    error: Some("missing batchData for action=prove".to_string()),
                },
            }
        }
        IncomingRequest::Direct(batch) => match service.prove_with_retry(batch).await {
            Ok(result) => ProverResponse {
                id: None,
                status: "ok".to_string(),
                result: Some(ProverResultBody::Proof(result)),
                error: None,
            },
            Err(err) => ProverResponse {
                id: None,
                status: "error".to_string(),
                result: None,
                error: Some(err.to_string()),
            },
        },
    }
}

fn split_groth16_hex(raw: &[u8]) -> Groth16ProofHex {
    let a0 = hex::encode(&raw[0..32]);
    let a1 = hex::encode(&raw[32..64]);
    let b00 = hex::encode(&raw[64..96]);
    let b01 = hex::encode(&raw[96..128]);
    let b10 = hex::encode(&raw[128..160]);
    let b11 = hex::encode(&raw[160..192]);
    let c0 = hex::encode(&raw[192..224]);
    let c1 = hex::encode(&raw[224..256]);
    Groth16ProofHex {
        a: [a0, a1],
        b: [[b00, b01], [b10, b11]],
        c: [c0, c1],
    }
}

fn decode_proof_nonce(input: Option<&String>) -> Result<[u8; 32]> {
    let raw = input.ok_or_else(|| anyhow!("missing groth16 public input index 4 (proof_nonce)"))?;
    let stripped = raw.trim_start_matches("0x");
    let padded = if stripped.len() % 2 != 0 {
        format!("0{stripped}")
    } else {
        stripped.to_string()
    };
    let bytes = hex::decode(&padded).context("invalid proof_nonce hex")?;
    if bytes.len() > 32 {
        return Err(anyhow!("proof_nonce too long: {} bytes", bytes.len()));
    }
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(out)
}

fn decode_public_outputs_hash(public_values: &mut SP1PublicValues) -> [u8; 32] {
    public_values.read()
}

fn parse_u64_string(field: &str, value: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .with_context(|| format!("invalid u64 in {field}: {value}"))
}

fn decode_hex_32(field: &str, value: &str) -> Result<[u8; 32]> {
    let stripped = value.trim_start_matches("0x");
    let bytes = hex::decode(stripped).with_context(|| format!("invalid hex in {field}"))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("{field} must be exactly 32 bytes"))
}

fn normalize_hex_32(field: &str, value: &str) -> Result<String> {
    Ok(hex::encode(decode_hex_32(field, value)?))
}

fn batch_to_circuit_inputs(
    batch: KeeperBatchDataWire,
) -> Result<(CircuitInputs, KeeperPublicOutputsWire, [u8; 32])> {
    let trades: Vec<TradeRecord> = batch
        .trades
        .iter()
        .map(|t| {
            Ok(TradeRecord {
                trade_id: decode_hex_32("trades[].tradeId", &t.trade_id)?,
                trader_pubkey: decode_hex_32("trades[].traderPubkey", &t.trader_pubkey)?,
                direction: t.direction.clone(),
                entry_price: parse_u64_string("trades[].entryPrice", &t.entry_price)?,
                exit_price: parse_u64_string("trades[].exitPrice", &t.exit_price)?,
                stake_usdt: parse_u64_string("trades[].stakeUsdt", &t.stake_usdt)?,
                multiplier_bps: t.multiplier_bps,
                window_start: t.window_start,
                window_end: t.window_end,
                band_lower: parse_u64_string("trades[].bandLower", &t.band_lower)?,
                band_upper: parse_u64_string("trades[].bandUpper", &t.band_upper)?,
                max_payout_usdt: parse_u64_string("trades[].maxPayoutUsdt", &t.max_payout_usdt)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if trades.is_empty() {
        return Err(anyhow!("batchData.trades must not be empty"));
    }

    if batch.trade_commitments.len() != trades.len() {
        return Err(anyhow!(
            "tradeCommitments length {} does not match trades length {}",
            batch.trade_commitments.len(),
            trades.len()
        ));
    }
    let trade_commitments: Vec<[u8; 32]> = batch
        .trade_commitments
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let provided = decode_hex_32("tradeCommitments[]", h)?;
            let computed = compute_trade_commitment(&trades[i]);
            if provided != computed {
                return Err(anyhow!("tradeCommitments[{i}] mismatch with computed commitment"));
            }
            Ok(provided)
        })
        .collect::<Result<Vec<_>>>()?;

    if batch.sorted_trade_ids.len() != trades.len() {
        return Err(anyhow!(
            "sortedTradeIds length {} does not match trades length {}",
            batch.sorted_trade_ids.len(),
            trades.len()
        ));
    }
    let sorted_trade_ids: Vec<[u8; 32]> = batch
        .sorted_trade_ids
        .iter()
        .map(|id| decode_hex_32("sortedTradeIds[]", id))
        .collect::<Result<Vec<_>>>()?;
    let mut expected_sorted: Vec<[u8; 32]> = trades.iter().map(|t| t.trade_id).collect();
    expected_sorted.sort();
    if sorted_trade_ids != expected_sorted {
        return Err(anyhow!(
            "sortedTradeIds mismatch: payload does not match sorted trade ids from trades[]"
        ));
    }

    let ohlc: Vec<OhlcTick> = batch
        .ohlc
        .iter()
        .map(|tick| {
            Ok(OhlcTick {
                ts: tick.ts,
                open: parse_u64_string("ohlc[].open", &tick.open)?,
                high: parse_u64_string("ohlc[].high", &tick.high)?,
                low: parse_u64_string("ohlc[].low", &tick.low)?,
                close: parse_u64_string("ohlc[].close", &tick.close)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let pyth_checkpoints: Vec<PythCheckpoint> = batch
        .pyth_checkpoints
        .iter()
        .map(|cp| {
            Ok(PythCheckpoint {
                price: cp.price,
                conf: cp.conf,
                exponent: cp.exponent,
                publish_time: cp.publish_time,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if pyth_checkpoints.len() != N_PYTH_CHECKPOINTS {
        return Err(anyhow!(
            "expected exactly {} pythCheckpoints entries; got {}",
            N_PYTH_CHECKPOINTS,
            pyth_checkpoints.len()
        ));
    }

    let expected_outputs = batch
        .expected_public_outputs
        .clone()
        .or(batch.computed.clone())
        .ok_or_else(|| anyhow!("batchData must include expectedPublicOutputs or computed"))?;
    let expected_hash = decode_hex_32("publicOutputs.publicOutputsHash", &expected_outputs.public_outputs_hash)?;

    let mut normalized_outputs = expected_outputs.clone();
    normalized_outputs.batch_id = normalize_hex_32("publicOutputs.batchId", &expected_outputs.batch_id)?;
    normalized_outputs.payouts_commitment =
        normalize_hex_32("publicOutputs.payoutsCommitment", &expected_outputs.payouts_commitment)?;
    normalized_outputs.trades_commitment =
        normalize_hex_32("publicOutputs.tradesCommitment", &expected_outputs.trades_commitment)?;
    normalized_outputs.nullifier_hash =
        normalize_hex_32("publicOutputs.nullifierHash", &expected_outputs.nullifier_hash)?;
    normalized_outputs.pyth_checkpoints_hash =
        normalize_hex_32("publicOutputs.pythCheckpointsHash", &expected_outputs.pyth_checkpoints_hash)?;
    normalized_outputs.public_outputs_hash =
        normalize_hex_32("publicOutputs.publicOutputsHash", &expected_outputs.public_outputs_hash)?;

    let inputs = CircuitInputs {
        trades,
        trade_commitments,
        sorted_trade_ids,
        ohlc,
        batch_id: decode_hex_32("batchId", &batch.batch_id)?,
        window_start: batch.window_start,
        window_end: batch.window_end,
        pool_balance_before: parse_u64_string("poolBalanceBefore", &batch.pool_balance_before)?,
        current_liability_before: parse_u64_string(
            "currentLiabilityBefore",
            &batch.current_liability_before,
        )?,
        keeper_fee_bps: batch.keeper_fee_bps,
        protocol_fee_bps: batch.protocol_fee_bps,
        market_max_multiplier: batch.market_max_multiplier,
        market_regime_id: batch.market_regime_id,
        pyth_feed_id: decode_hex_32("pythFeedId", &batch.pyth_feed_id)?,
        pyth_checkpoints,
    };

    Ok((inputs, normalized_outputs, expected_hash))
}

fn backoff_delay_ms(attempt: u32, base_ms: u64, cap_ms: u64) -> u64 {
    let exp = attempt.saturating_sub(1);
    let raw = base_ms.saturating_mul(2u64.saturating_pow(exp));
    raw.min(cap_ms)
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let pretty = serde_json::to_string_pretty(value)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut file = fs::File::create(path)?;
    file.write_all(pretty.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}
