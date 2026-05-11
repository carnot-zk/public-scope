# carnot-programs

Anchor smart contracts for the Carnot protocol, plus a TypeScript SDK and CLI tooling.

## Programs

| Program | Devnet | Mainnet |
|---------|--------|---------|
| `carnot_engine` | `carCrmy6qN8tRgvUp9v6JrfUuxroGrKdndUdwMMNumS` | `cartX31ocscytAK988e5h1xAMAeNXt6zqdgeyr3pZ3U` |

## Prerequisites

- [Rust](https://rustup.rs/) + [Anchor CLI](https://www.anchor-lang.com/docs/installation) 0.32.0
- Solana CLI 2.3.0
- [Bun](https://bun.sh/) ≥ 1.3.5
- pnpm (enforced by preinstall hook)

## Setup

```sh
pnpm install
```

Copy `.env.example` to `.env` and fill in your RPC URLs and wallet paths.

## Build

```sh
# Build all programs
pnpm build

# Build a single program
pnpm build -- -p carnot_engine

# Verifiable build
pnpm build -- --verifiable
```

## Test

```sh
pnpm test
```

## Deploy

```sh
# Devnet
pnpm deploy -- --program carnot_engine --env devnet

# Mainnet
pnpm deploy -- --program carnot_engine --env mainnet
```

Devnet builds include the `staging` feature flag automatically.

## TypeScript SDK (`ts-sdk/`)

Exports PDA helpers, account types, and a `TransactionBuilder` for constructing on-chain instructions.

```ts
import { TransactionBuilder, findMarketPda } from "./ts-sdk";
```
