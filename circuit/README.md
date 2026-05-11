# carnot-circuit

SP1 ZK circuit for Carnot settlement proofs. Verifies trade outcomes, oracle integrity, and payout accounting inside the SP1 zkVM, producing a single `public_outputs_hash` committed on-chain.

## Workspace crates

| Crate | Role |
|-------|------|
| `lib` | Shared logic: trade commitment, payout Merkle root, Pyth helpers |
| `program` | SP1 guest program — the circuit itself |
| `aggregator-program` | SP1 aggregator guest (batch-level proof aggregation) |
| `script` | Host binaries: `carnot` (one-shot prove), `keeper-prover` (long-running prover server), `vkey` (print verification key) |

## Prerequisites

- Rust (see `rust-toolchain` for the pinned nightly)
- [SP1](https://docs.succinct.xyz/getting-started/install.html) v6.1.0

## Build

```sh
# Build all crates (including the keeper-prover binary)
cargo build --release
```

The `script/build.rs` compiles the guest program via `sp1-build` automatically.

## One-shot prove (dev / testing)

```sh
cargo run --release --bin carnot -- --help
```

Reads inputs from `script/fixtures/` and writes a proof to `script/out/`.

## keeper-prover (for carnot-keeper)

```sh
cargo build --release --bin keeper-prover
# Binary lands at: target/release/keeper-prover
```

Point `SP1_PROVER_BINARY` in `keeper/.env` at this path.

## Verification key

```sh
cargo run --release --bin vkey
```

Prints the SP1 verification key that must match the value stored in the `carnot_engine` on-chain program.

## Docker (prover image)

```sh
docker build -t carnot-prover .
```

## Circuit public outputs

The guest commits a single SHA256 hash over 20 fields (batch_id, window, trade counts, Pyth checkpoints hash, pool balances, fees, payout/trade commitments, nullifier). The on-chain program verifies this hash matches its expected state transition.
