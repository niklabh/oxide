import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { createHash } from "crypto";
import { expect } from "chai";
import { WasmRegistry } from "../target/types/wasm_registry";

describe("wasm-registry", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.WasmRegistry as Program<WasmRegistry>;
  const publisher = provider.wallet;

  const name = "my-app";
  const hashV1 = Array.from(
    createHash("sha256").update("dummy wasm bytes v1").digest()
  );
  const hashV2 = Array.from(
    createHash("sha256").update("dummy wasm bytes v2").digest()
  );

  const [entryPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("wasm"), publisher.publicKey.toBuffer(), Buffer.from(name)],
    program.programId
  );

  it("registers a hash", async () => {
    await program.methods
      .register(name, hashV1 as any)
      .accounts({ entry: entryPda, publisher: publisher.publicKey })
      .rpc();

    const entry = await program.account.wasmEntry.fetch(entryPda);
    expect(entry.publisher.toBase58()).to.equal(publisher.publicKey.toBase58());
    expect(entry.name).to.equal(name);
    expect(entry.version).to.equal(0);
    expect(Buffer.from(entry.hash).equals(Buffer.from(hashV1))).to.be.true;
  });

  it("updates the hash and bumps version", async () => {
    await program.methods
      .update(hashV2 as any)
      .accounts({ entry: entryPda, publisher: publisher.publicKey })
      .rpc();

    const entry = await program.account.wasmEntry.fetch(entryPda);
    expect(entry.version).to.equal(1);
    expect(Buffer.from(entry.hash).equals(Buffer.from(hashV2))).to.be.true;
  });

  it("rejects updates from a non-publisher", async () => {
    const stranger = anchor.web3.Keypair.generate();
    try {
      await program.methods
        .update(hashV1 as any)
        .accounts({ entry: entryPda, publisher: stranger.publicKey })
        .signers([stranger])
        .rpc();
      expect.fail("expected the update to be rejected");
    } catch (err) {
      expect(String(err)).to.match(/seeds|has_one|Signature|ConstraintHasOne/i);
    }
  });

  it("revokes the entry and returns rent", async () => {
    await program.methods
      .revoke()
      .accounts({ entry: entryPda, publisher: publisher.publicKey })
      .rpc();

    const info = await provider.connection.getAccountInfo(entryPda);
    expect(info).to.be.null;
  });
});
