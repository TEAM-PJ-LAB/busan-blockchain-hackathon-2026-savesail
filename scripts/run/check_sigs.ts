// G1: every transaction recorded in run.json exists on devnet (finalized or confirmed).
import { Connection } from "@solana/web3.js";
import { readFileSync } from "node:fs";

const run = JSON.parse(readFileSync(process.argv[2], "utf8"));
const conn = new Connection(process.env.RPC_URL ?? "https://api.devnet.solana.com", "confirmed");
const sigs: string[] = run.txs.map((t: { sig: string }) => t.sig);
let ok = 0;
for (let i = 0; i < sigs.length; i += 200) {
  const r = await conn.getSignatureStatuses(sigs.slice(i, i + 200), { searchTransactionHistory: true });
  ok += r.value.filter((s) => s && !s.err && (s.confirmationStatus === "confirmed" || s.confirmationStatus === "finalized")).length;
}
console.log(`${ok}/${sigs.length} signatures confirmed on devnet`);
if (ok !== sigs.length) process.exit(1);
