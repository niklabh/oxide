# Solana WASM Registry

On-chain registry that lets publishers attest to **WebAssembly module hashes** on Solana. Each entry binds a 32-byte hash (e.g. SHA-256 of a `.wasm` file) to a publisher's pubkey under a chosen name, plus a monotonic `version` counter and timestamps.

## What "signed" means here

Solana does not store free-form signatures, but every state change requires the publisher to **sign the transaction that writes the entry**. The program enforces:

- Only the keypair whose `Pubkey` is stored on the PDA can `update` or `revoke` (Anchor `Signer` + `has_one = publisher`).
- The PDA address itself is derived from the publisher's pubkey, so anyone can re-derive `(publisher, name) → address` and confirm the binding off-chain.

Net effect: storing a hash through this program is equivalent to publishing a detached signature over `(name, hash, version, timestamp)`, with the bonus of revocation and on-chain versioning.

## Account model

One PDA per `(publisher, name)`:

```
seeds = [ "wasm", publisher_pubkey, name_bytes ]
```

```rust
struct WasmEntry {
    publisher: Pubkey,    // who signed
    hash: [u8; 32],       // the WASM module hash (algorithm-agnostic)
    name: String,         // up to 64 bytes
    version: u32,         // 0 on register, +1 on every update
    created_at: i64,
    updated_at: i64,
    bump: u8,
}
```

## Instructions

| Instruction          | Signer    | Effect                                                                 |
| -------------------- | --------- | ---------------------------------------------------------------------- |
| `register(name,hash)`| publisher | Creates a new PDA for `(publisher, name)`, stores `hash`, `version = 0` |
| `update(hash)`       | publisher | Overwrites `hash`, increments `version`, updates `updated_at`           |
| `revoke()`           | publisher | Closes the account; rent is returned to the publisher                   |

## Prerequisites

- Rust 1.79+
- Solana CLI 2.0+ — `sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"`
- Anchor 0.32+ — `cargo install --git https://github.com/coral-xyz/anchor avm --locked && avm install 0.32.1 && avm use 0.32.1`
- Node 18+ and Yarn — `npm i -g yarn`

## Build & test (local)

```bash
cd solana-wasm-registry
yarn install
anchor build
anchor test
```

`anchor test` spins up `solana-test-validator`, deploys the program, and runs `tests/wasm-registry.ts` (register → update → reject stranger → revoke).

## First-time program key

The first `anchor build` generates a fresh keypair at `target/deploy/wasm_registry-keypair.json`. Sync it into the source so the `declare_id!` in `lib.rs` and the `[programs.*]` blocks in `Anchor.toml` all match:

```bash
anchor keys sync
anchor build   # rebuild with the real ID baked in
```

## Deploying to devnet

1. Make sure your local wallet has SOL:
   ```bash
   solana-keygen new --no-bip39-passphrase -o ~/.config/solana/id.json  # skip if you have one
   solana config set --url https://api.devnet.solana.com
   solana airdrop 2
   ```

2. Build and deploy:
   ```bash
   anchor build
   anchor deploy --provider.cluster devnet
   ```

   Anchor prints `Program Id: <pubkey>`. Verify it landed:
   ```bash
   solana program show <program-id> --url devnet
   ```

3. Upgrades later (same upgrade authority by default):
   ```bash
   anchor build
   anchor upgrade target/deploy/wasm_registry.so --program-id <program-id> --provider.cluster devnet
   ```

4. Make the program immutable when you're done iterating (optional, but recommended for production):
   ```bash
   solana program set-upgrade-authority <program-id> --final --url devnet
   ```

## Deploying to mainnet

Identical to devnet, but use `--provider.cluster mainnet` and a wallet funded with enough SOL (~2–5 SOL depending on binary size). Use a dedicated upgrade authority (ideally an offline / multisig key) and simulate every deploy first.

## Client usage (TypeScript)

```ts
import { createHash } from "crypto";
import fs from "fs";
import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";

const wasmBytes = fs.readFileSync("./my-app.wasm");
const hash = Array.from(createHash("sha256").update(wasmBytes).digest());

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);
const program = anchor.workspace.WasmRegistry;

const name = "my-app";
const [entryPda] = PublicKey.findProgramAddressSync(
  [Buffer.from("wasm"), provider.wallet.publicKey.toBuffer(), Buffer.from(name)],
  program.programId
);

// First publish
await program.methods
  .register(name, hash)
  .accounts({ entry: entryPda })
  .rpc();

// Later, publish a new build under the same name
const newHash = Array.from(
  createHash("sha256").update(fs.readFileSync("./my-app.v2.wasm")).digest()
);
await program.methods
  .update(newHash)
  .accounts({ entry: entryPda })
  .rpc();
```

## Verifying a hash later (no wallet required)

```ts
const entry = await program.account.wasmEntry.fetch(entryPda);
const actual = createHash("sha256")
  .update(fs.readFileSync("./my-app.wasm"))
  .digest();
const ok = Buffer.from(entry.hash).equals(actual);
console.log({ ok, version: entry.version, publisher: entry.publisher.toBase58() });
```

For full history, index the `EntryRegistered` / `EntryUpdated` / `EntryRevoked` events emitted by the program (each `update` only stores the latest hash on-chain).

## Risk notes

- The program does **not** inspect the WASM bytes — it stores whatever 32 bytes the publisher submits. The trust statement is "the holder of this pubkey attested to this hash at this time."
- `update` overwrites the previous hash. If you need an immutable, auditable history, index the events off-chain.
- Names are publisher-scoped, so two different publishers can use the same name without colliding — clients must always identify entries by `(publisher, name)`, never name alone.
- Set or burn the upgrade authority before treating any mainnet deployment as production.
- Account discriminator and `has_one = publisher` already prevent type-confusion and stranger-writes; do not relax those constraints in forks.
