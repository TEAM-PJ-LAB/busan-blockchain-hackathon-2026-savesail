// PRD 7.5 ①: the web page's chain comparison (app/lib/chaincheck.ts) against the real accounts.
// local: reads the executor's saved LiteSVM state; devnet: RPC. Also checks that a changed value is caught.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { PublicKey } from "@solana/web3.js";
import { compareChain, rpcFetch, type Fetch } from "../../app/lib/chaincheck";

const [runPath, statePath] = process.argv.slice(2);
const run = JSON.parse(readFileSync(runPath, "utf8"));
let fetch: Fetch;
if (run.cluster === "local") {
  const { LiteSvm } = createRequire(import.meta.url)("litesvm/dist/internal");
  const svm = LiteSvm.loadFromFile(statePath);
  fetch = async (a) => a.map((x) => { const acc = svm.getAccount(new PublicKey(x).toBytes()); return acc ? Uint8Array.from(acc.data()) : null; });
} else fetch = rpcFetch(process.env.RPC_URL ?? "https://api.devnet.solana.com");

const r = await compareChain(run, fetch);
console.log(`chain check: ${r.total - r.bad.length}/${r.total} values match${r.bad.length ? ": " + r.bad.join(", ") : ""}`);
let fail = r.bad.length > 0 || r.total < 20 + run.schedule.length;
// sensitivity: one changed value per kind must be reported
for (const [label, edit] of [
  ["deal", (x: any) => (x.deal.entitlement = String(BigInt(x.deal.entitlement) + 1n))],
  ["period", (x: any) => (x.periods[x.periods.length - 1].dist[1] = String(BigInt(x.periods[x.periods.length - 1].dist[1]) + 1n))],
  ["group", (x: any) => (x.schedule[0].off = (x.schedule[0].off + 1) % 5)],
  ["token", (x: any) => (x.balances.investorA = String(BigInt(x.balances.investorA) + 1n))],
] as const) {
  const m = JSON.parse(JSON.stringify(run));
  edit(m);
  const q = await compareChain(m, fetch);
  const caught = q.bad.length === 1;
  console.log(`  changed ${label} value -> ${caught ? "caught" : "MISSED"} (${q.bad.join(", ")})`);
  if (!caught) fail = true;
}
if (fail) process.exit(1);
