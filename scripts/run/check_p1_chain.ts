// PRD 7.5 ① for the P1 values on the invest page: app/lib/chaincheck.ts compareP1 against the real accounts.
// local: the LiteSVM state saved by p1.ts local; devnet: RPC. Also checks that one changed value per kind is caught.
//   tsx run/check_p1_chain.ts ../target/p1-local.json ../target/p1-local-svm.bin
//   tsx run/check_p1_chain.ts ../app/public/p1-run.json
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { PublicKey } from "@solana/web3.js";
import { compareP1, rpcFetch, type Fetch } from "../../app/lib/chaincheck";

const [runPath, statePath] = process.argv.slice(2);
const run = JSON.parse(readFileSync(runPath, "utf8"));
let fetch: Fetch;
if (run.cluster === "local") {
  const { LiteSvm } = createRequire(import.meta.url)("litesvm/dist/internal");
  const svm = LiteSvm.loadFromFile(statePath);
  fetch = async (a: string[]) => a.map((x: string) => { const acc = svm.getAccount(new PublicKey(x).toBytes()); return acc ? Uint8Array.from(acc.data()) : null; });
} else fetch = rpcFetch(process.env.RPC_URL ?? "https://api.devnet.solana.com");

const r = await compareP1(run, fetch);
console.log(`P1 chain check: ${r.total - r.bad.length}/${r.total} values match${r.bad.length ? ": " + r.bad.join(", ") : ""}`);
let fail = r.bad.length > 0 || r.total !== 5 * run.mints.length + 2 + 3 * run.holders.length;
for (const [label, edit] of [
  ["supply", (x: any) => (x.mints[0].supply = String(BigInt(x.mints[0].supply) + 1n))],
  ["default state", (x: any) => (x.mints[1].default_account_state = "Initialized")],
  ["token_acl", (x: any) => (x.mints[0].token_acl = x.mints[0].address)],
  ["allow list", (x: any) => (x.allow_list.wallets += 1)],
  ["holder state", (x: any) => (x.holders[2].state = "initialized")],
  ["units", (x: any) => (x.holders[0].units = String(BigInt(x.holders[0].units) + 1n))],
] as const) {
  const m = JSON.parse(JSON.stringify(run));
  edit(m);
  const q = await compareP1(m, fetch);
  const caught = q.bad.length === 1;
  console.log(`  changed ${label} -> ${caught ? "caught" : "MISSED"} (${q.bad.join(", ")})`);
  if (!caught) fail = true;
}
if (fail) process.exit(1);
