# carnot-keeper

Permissionless keeper bot for the Carnot protocol. Watches for ready batches on-chain, generates a SP1 ZK proof of settlement, and submits it back to the `carnot_engine` program.

## How it works

1. **Watcher** — subscribes to on-chain events for new/ready batches
2. **Prover** — invokes `keeper-prover` (built from `carnot-circuit`) to generate a SP1 proof
3. **Submitter** — sends the verified proof + settlement instruction on-chain
4. **Reward tracker** — records keeper fees earned in a local SQLite DB

## Prerequisites

- Node.js / `ts-node`
- A funded Solana keypair (base58) with (currently requires) authority to act as keeper
- `keeper-prover` binary (built from `carnot-circuit`)

## Setup

```sh
npm install
cp .env.example .env
# Fill in KEEPER_KEYPAIR, CARNOT_INTERNAL_API_KEY, SP1_PROVER_BINARY, RPC URLs
```

## Run

```sh
# Development
npm run dev

# Production (build first)
npm run build
npm start
```

## Docker

```sh
docker compose up -d
```

The container mounts a persistent volume at `/data` for the rewards DB.

## Key env vars

| Variable | Description |
|----------|-------------|
| `KEEPER_KEYPAIR` | Base58 private key — **never commit** |
| `CARNOT_INTERNAL_API_KEY` | Key for internal batch API |
| `SP1_PROVER_BINARY` | Path to `keeper-prover` binary |
| `SP1_PROVER` | `local` (default) or `network` (Succinct cloud) |
| `BATCH_POLL_INTERVAL_MS` | How often to poll for batches (default 30 s) |

## Reward tracking

```sh
npm run rewards
```

Reads `REWARD_DB_PATH` (default `~/.carnot-keeper/rewards.db`) and prints a summary of keeper fees earned per market.
