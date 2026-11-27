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
import { rateLimitedFetch } from "./rpc.js";

export interface SentTx {
  label: string;
  signature: string;
  slot: number;
  cu?: number;
}

/** A transaction that was meant to fail: on devnet it is sent without preflight so the failure is on chain. */
export interface FailedTx {
  label: string;
  signature: string | null;
  err: string;
  logs: string[];
}

export interface Chain {
  readonly kind: "local" | "devnet";
  send(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx>;
  sendFail(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<FailedTx>;
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
  constructor(programId: PublicKey, soPath: string, resumeFrom?: string, extra: [PublicKey, string][] = []) {
    const require = createRequire(import.meta.url);
    const { LiteSvm } = require("litesvm/dist/internal");
    const { existsSync } = require("node:fs");
    if (resumeFrom && existsSync(resumeFrom)) {
      this.svm = LiteSvm.loadFromFile(resumeFrom);
    } else {
      this.svm = new LiteSvm();
      this.svm.setDefaultPrograms();
      this.svm.setTransactionHistory(0n);
      this.svm.addProgramFromFile(programId.toBytes(), soPath);
      for (const [id, path] of extra) this.svm.addProgramFromFile(id.toBytes(), path);
    }
  }
  save(path: string) {
    this.svm.saveToFile(path);
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
  async sendFail(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<FailedTx> {
    try {
      await this.send(label, ixs, signers);
    } catch (e) {
      if (e instanceof TxError) return { label, signature: null, err: e.message.split("\n")[0], logs: e.logs };
      throw e;
    }
    throw new Error(`${label}: expected a failure, the transaction succeeded`);
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
// HTTP only (no websocket subscriptions), every request through the rate limiter, the recent blockhash reused for
// 20 s, and a sent transaction never sent again while it can still land: after sendRawTransaction its signature is
// polled until it is confirmed or its blockhash expires, and only an expired, unseen transaction is rebuilt.
export class DevnetChain implements Chain {
  readonly kind = "devnet";
  readonly conn: Connection;
  private bh?: { blockhash: string; lastValidBlockHeight: number; at: number };
  constructor(url: string, private commitment: Commitment = "confirmed") {
    this.conn = new Connection(url, { commitment, fetch: rateLimitedFetch(), disableRetryOnRateLimit: true });
  }
  private async blockhash() {
    if (!this.bh || Date.now() - this.bh.at > 20_000) {
      const b = await this.conn.getLatestBlockhash(this.commitment);
      this.bh = { ...b, at: Date.now() };
    }
    return this.bh;
  }
  /** slot when confirmed, null when the blockhash expired without the transaction landing */
  private async landed(label: string, sig: string, lastValidBlockHeight: number): Promise<number | null> {
    for (let i = 0; ; i++) {
      await sleep(i === 0 ? 400 : 700);
      let st;
      try {
        st = (await this.conn.getSignatureStatuses([sig])).value[0];
      } catch {
        continue; // transient RPC failure: keep polling the same signature
      }
      if (st?.err) throw new TxError(label, [], JSON.stringify(st.err));
      if (st && (st.confirmationStatus === "confirmed" || st.confirmationStatus === "finalized")) return st.slot;
      if (i % 8 === 7) {
        const h = await this.conn.getBlockHeight(this.commitment).catch(() => 0);
        if (h > lastValidBlockHeight) {
          const again = (await this.conn.getSignatureStatuses([sig], { searchTransactionHistory: true })).value[0];
          if (again?.err) throw new TxError(label, [], JSON.stringify(again.err));
          return again && !again.err ? again.slot : null;
        }
      }
    }
  }
  async send(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx> {
    let last: unknown;
    for (let attempt = 0; attempt < 6; attempt++) {
      try {
        const { blockhash, lastValidBlockHeight } = await this.blockhash();
        const tx = new Transaction({ feePayer: signers[0].publicKey, blockhash, lastValidBlockHeight });
        tx.add(...ixs);
        tx.sign(...signers);
        const sig = await this.conn.sendRawTransaction(tx.serialize(), { skipPreflight: false, maxRetries: 5 });
        const slot = await this.landed(label, sig, lastValidBlockHeight);
        if (slot !== null) return { label, signature: sig, slot };
        this.bh = undefined; // expired unseen: rebuild with a fresh blockhash
      } catch (e: any) {
        if (e instanceof TxError) throw e;
        last = e;
        const logs: string[] = e?.logs ?? e?.transactionLogs ?? [];
        if (logs.some((l) => l.includes("custom program error") || l.includes("AnchorError"))) {
          throw new TxError(label, logs, e.message ?? String(e)); // program rejected: retrying cannot help
        }
        // a stale blockhash, or an identical transaction already sent under the cached one: take a fresh blockhash
        if (/blockhash not found|already been processed/i.test(String(e?.message))) this.bh = undefined;
        await sleep(1000 * 2 ** attempt); // exponential backoff (PRD 12.1)
      }
    }
    throw last;
  }
  async sendFail(label: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<FailedTx> {
    const { blockhash, lastValidBlockHeight } = await this.blockhash();
    const tx = new Transaction({ feePayer: signers[0].publicKey, blockhash, lastValidBlockHeight });
    tx.add(...ixs);
    tx.sign(...signers);
    const sig = await this.conn.sendRawTransaction(tx.serialize(), { skipPreflight: true, maxRetries: 5 });
    for (let i = 0; i < 120; i++) {
      await sleep(700);
      const st = (await this.conn.getSignatureStatuses([sig]).catch(() => ({ value: [null] }))).value[0];
      if (!st || (st.confirmationStatus !== "confirmed" && st.confirmationStatus !== "finalized")) continue;
      if (!st.err) throw new Error(`${label}: expected a failure, the transaction succeeded (${sig})`);
      const t = await this.conn.getTransaction(sig, { commitment: "confirmed", maxSupportedTransactionVersion: 0 }).catch(() => null);
      return { label, signature: sig, err: JSON.stringify(st.err), logs: t?.meta?.logMessages ?? [] };
    }
    throw new Error(`${label}: ${sig} did not land`);
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
