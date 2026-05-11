import {
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  Signer,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";

export class TransactionBuilder {
  private instructions: TransactionInstruction[] = [];
  private signers: Signer[] = [];

  constructor(
    private readonly connection: Connection,
    private readonly payer: Signer
  ) {}

  addInstruction(ix: TransactionInstruction): this {
    this.instructions.push(ix);
    return this;
  }

  addInstructions(ixs: TransactionInstruction[]): this {
    this.instructions.push(...ixs);
    return this;
  }

  addSigner(signer: Signer): this {
    this.signers.push(signer);
    return this;
  }

  async buildVersionedTransaction(
    lookupTableAccounts: AddressLookupTableAccount[] = []
  ): Promise<VersionedTransaction> {
    const { blockhash } = await this.connection.getLatestBlockhash("confirmed");
    const message = new TransactionMessage({
      payerKey: this.payer.publicKey,
      recentBlockhash: blockhash,
      instructions: this.instructions,
    }).compileToV0Message(lookupTableAccounts);
    const tx = new VersionedTransaction(message);
    tx.sign([this.payer, ...this.signers]);
    return tx;
  }

  async execute(
    lookupTableAccounts: AddressLookupTableAccount[] = []
  ): Promise<string> {
    const tx = await this.buildVersionedTransaction(lookupTableAccounts);
    return this.connection.sendTransaction(tx, { skipPreflight: false });
  }

  async getLookupTableAccount(address: PublicKey): Promise<AddressLookupTableAccount | null> {
    const accountInfo = await this.connection.getAddressLookupTable(address);
    return accountInfo.value;
  }
}
