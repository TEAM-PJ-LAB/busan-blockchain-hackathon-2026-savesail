// Chain access for the executor: devnet (RPC) or a local LiteSVM instance with the same program binary.
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  type Commitment,
} from "@solana/web3.js";
import { createRequire } from "node:module";

export interface SentTx {
  label: string;
  signature: string;
  slot: number;
  cu?: number;
}

export interface Chain {
  readonly kind: "local" | "devnet";
  send(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx>;
  account(pk: PublicKey): Promise<Buffer | null>;
  lamports(pk: PublicKey): Promise<number>;
  slot(): Promise<number>;
  waitSlots(n: number): Promise<void>;
  fund(from: Keypair, to: PublicKey, lamports: number): Promise<void>;
}

export class TxError extends Error {
  constructor(public label: string, public logs: string[], msg: string) {
    super(`${label}: ${msg}\n${logs.slice(-8).join("\n")}`);
  }
}

// ------------------------------------------------------------------ local (LiteSVM native binding)
export class LocalChain implements Chain {
  readonly kind = "local";
  private svm: any;
  constructor(programId: PublicKey, soPath: string) {
    const require = createRequire(import.meta.url);
    const { LiteSvm } = require("litesvm/dist/internal");
    this.svm = new LiteSvm();
    this.svm.setDefaultPrograms();
    this.svm.setTransactionHistory(0n);
    this.svm.addProgramFromFile(programId.toBytes(), soPath);
  }
  async send(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx> {
    this.svm.expireBlockhash();
    const tx = new Transaction({ feePayer: signers[0].publicKey, recentBlockhash: this.svm.latestBlockhash() });
    tx.add(...ixs);
    tx.sign(...signers);
    const r = this.svm.sendLegacyTransaction(tx.serialize());
    const meta = typeof r.meta === "function" ? r.meta() : r;
    const logs: string[] = meta.logs ? meta.logs() : [];
    if (typeof r.err === "function") throw new TxError(label, logs, JSON.stringify(r.err(), (_, v) => (typeof v === "bigint" ? v.toString() : v)));
    return { label, signature: Buffer.from(meta.signature()).toString("hex"), slot: Number(this.svm.getClock().slot), cu: Number(meta.computeUnitsConsumed()) };
  }
  async account(pk: PublicKey): Promise<Buffer | null> {
    const a = this.svm.getAccount(pk.toBytes());
    return a ? Buffer.from(a.data()) : null;
  }
  async lamports(pk: PublicKey): Promise<number> {
    return Number(this.svm.getBalance(pk.toBytes()) ?? 0n);
  }
  async slot(): Promise<number> {
    return Number(this.svm.getClock().slot);
  }
  async waitSlots(n: number): Promise<void> {
    const s = this.svm.getClock().slot;
    this.svm.warpToSlot(s + BigInt(n));
  }
  async fund(_from: Keypair, to: PublicKey, lamports: number): Promise<void> {
    this.svm.airdrop(to.toBytes(), BigInt(lamports));
  }
}

// ------------------------------------------------------------------ devnet
export class DevnetChain implements Chain {
  readonly kind = "devnet";
  readonly conn: Connection;
  constructor(url: string, private commitment: Commitment = "confirmed") {
    this.conn = new Connection(url, commitment);
  }
  async send(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx> {
    let last: unknown;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        const { blockhash, lastValidBlockHeight } = await this.conn.getLatestBlockhash(this.commitment);
        const tx = new Transaction({ feePayer: signers[0].publicKey, blockhash, lastValidBlockHeight });
        tx.add(...ixs);
        tx.sign(...signers);
        const sig = await this.conn.sendRawTransaction(tx.serialize(), { skipPreflight: false, maxRetries: 3 });
        const res = await this.conn.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, this.commitment);
        if (res.value.err) throw new TxError(label, [], JSON.stringify(res.value.err));
        return { label, signature: sig, slot: res.context.slot };
      } catch (e: any) {
        last = e;
        const logs: string[] = e?.logs ?? e?.transactionLogs ?? [];
        if (logs.some((l) => l.includes("custom program error") || l.includes("AnchorError"))) {
          throw new TxError(label, logs, e.message ?? String(e)); // program rejected: retrying cannot help
        }
        await sleep(1000 * 2 ** attempt); // exponential backoff (PRD 12.1)
      }
    }
    throw last;
  }
  async account(pk: PublicKey): Promise<Buffer | null> {
    const a = await this.conn.getAccountInfo(pk, this.commitment);
    return a ? Buffer.from(a.data) : null;
  }
  async lamports(pk: PublicKey): Promise<number> {
    return this.conn.getBalance(pk, this.commitment);
  }
  async slot(): Promise<number> {
    return this.conn.getSlot(this.commitment);
  }
  async waitSlots(n: number): Promise<void> {
    const target = (await this.slot()) + n;
    while ((await this.slot()) < target) await sleep(400);
  }
  async fund(from: Keypair, to: PublicKey, lamports: number): Promise<void> {
    const have = await this.lamports(to);
    if (have >= lamports) return;
    await this.send("fund", [SystemProgram.transfer({ fromPubkey: from.publicKey, toPubkey: to, lamports: lamports - have })], [from]);
  }
}

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
