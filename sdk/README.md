# @carnot-zk/sdk

Shared types, Anchor IDLs, PDA helpers, and constants for the Carnot platform.

## Installation

```bash
npm install @carnot-zk/sdk
# or
yarn add @carnot-zk/sdk
# or
pnpm add @carnot-zk/sdk
```

**Peer dependency:** `@solana/web3.js ^1.91.0`

## Entry Points

The package exposes four sub-path imports so you only bundle what you need:

| Import | Contents |
|--------|----------|
| `@carnot-zk/sdk` | Everything (re-exports all below) |
| `@carnot-zk/sdk/constants` | Program IDs, USDT mint, Pyth feed IDs, risk parameters |
| `@carnot-zk/sdk/pda` | PDA derivation helpers |
| `@carnot-zk/sdk/idl` | Carnot Engine Anchor IDL |

## Usage

### Constants

```ts
import {
  PROGRAM_IDS,
  USDT_MINT,
  getUsdtMint,
  normalizeSolanaNetwork,
} from '@carnot-zk/sdk/constants';

const network = 'mainnet';
const programId = PROGRAM_IDS[network].carnot;
const usdtMint = getUsdtMint(network);
```

### Pyth Price Feed IDs

```ts
import {
  getPythBtcUsdFeedId,
  getPythEthUsdFeedId,
  getPythSolUsdFeedId,
  getPythJupUsdFeedId,
  PYTH_MAX_PRICE_AGE_SECS,
} from '@carnot-zk/sdk/constants';

const btcFeedId = getPythBtcUsdFeedId('mainnet');
```

### Risk Parameters

```ts
import {
  PLATFORM_EDGE_BPS,
  KEEPER_FEE_BPS,
  PROTOCOL_FEE_BPS,
  MAX_SINGLE_BET_EXPOSURE_BPS,
} from '@carnot-zk/sdk/constants';
```

### PDA Derivation

```ts
import { PublicKey } from '@solana/web3.js';
import {
  findCarnotGlobalStatePda,
  findCarnotVaultStatePda,
  findCarnotMarketStatePda,
  findCarnotTraderAccountPda,
  findCarnotLpPositionPda,
  findCarnotBatchReceiptPda,
  findCarnotClaimReceiptPda,
  findCarnotNullifierPda,
} from '@carnot-zk/sdk/pda';

const programId = new PublicKey('cartX31ocscytAK988e5h1xAMAeNXt6zqdgeyr3pZ3U'); // devnet

const [globalState] = findCarnotGlobalStatePda(programId);
const [vaultState] = findCarnotVaultStatePda(programId);
const [traderAccount] = findCarnotTraderAccountPda(traderPubkey, programId);
const [lpPosition] = findCarnotLpPositionPda(lpPubkey, programId);
```

### Encoding Utilities

```ts
import { hexToBytes, hexToBuffer, bytesToHex, stripHexPrefix } from '@carnot-zk/sdk';

const bytes = hexToBytes('0xdeadbeef');
const hex   = bytesToHex(bytes); // '0xdeadbeef'
```

### Types

```ts
import type {
  // Trade
  TradeRecord,
  OhlcTick,
  Direction,
  KeeperTradeRecordWire,
  KeeperOhlcTickWire,
  // ZK settlement
  CarnotSubmission,
  CarnotPublicInputs,
  CarnotVerifyAndSettleArgs,
  // Network
  SolanaNetwork,
  ProgramIds,
} from '@carnot-zk/sdk';
```

## Program IDs

| Network | Program ID |
|---------|-----------|
| mainnet | `carCrmy6qN8tRgvUp9v6JrfUuxroGrKdndUdwMMNumS` |
| devnet  | `cartX31ocscytAK988e5h1xAMAeNXt6zqdgeyr3pZ3U` |

## Building

```bash
pnpm build       # builds CJS + ESM + .d.ts into dist/
pnpm typecheck   # tsc --noEmit
```

## License

MIT
