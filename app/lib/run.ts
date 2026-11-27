// Loads the executor's run record at build time (PRD 7.4/7.5). Every number on the site comes from here or the chain.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export type Tx = { step: string; sig: string; slot: number; cu?: number };
export type Period = {
  period: number; outcome: number; due: string; entitlement: string; dist: string[]; lb: number; est: number; ub: number;
  flags: number; excluded: number;
  payload: { kg_on: number; kg_miss_payer: number; kg_miss_inv: number; lb_bps: number; est_bps: number; ub_bps: number;
    compliance_bps: number; flags: number; report_hash: string; override_groups: number };
};
export type Run = {
  cluster: string; program: string; started: string; finished: string; seconds: number; tx_count: number; txs: Tx[];
  steps: { step: string; rejected: string }[]; periods: Period[]; schedule: { off: number; missing: number }[];
  accounts: { deal: string; campaign: string; mint: string; vaults: Record<string, string>; roles: Record<string, string>; periods: string[] };
  deal: { phi_bps: number; ref_rate_bps: number; entitlement: string; paid_cum: string; last_lb_bps: number | null; acc: string[];
    reserve: string; opex_arrears: string; senior_arrears: string; install_cost: string; caps: string[]; raised: string[];
    comp_min_bps: number; miss_max_bps: number; opex_per_period: string; reserve_target: string; n_periods: number;
    senior_per_period: string; n_groups: number; groups_per_period: number; bond_amount: string; slash_per_miss: string; retail_limit: string };
  balances: Record<string, string>;
  token_accounts: Record<string, string>;
  positions: { investor: string; tranche: number; address: string; units: string; claimed: string }[];
  scenario: { withhold: Record<string, number>; off_violations: number[]; groups_per_period: number };
};

export function loadRun(): Run {
  // devnet record (written by .github/workflows/devnet.yml) if present, else the committed local-chain record
  for (const p of [join(process.cwd(), "public/run.json"), join(process.cwd(), "fixtures/run-local.json")]) {
    if (existsSync(p)) return JSON.parse(readFileSync(p, "utf8"));
  }
  throw new Error("run.json not found: run the executor first (scripts/run)");
}

export const usd = (base: string | number | bigint, digits = 0) => {
  const v = Number(base) / 1e6;
  return v.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
};
export const pct = (x: number, d = 2) => (x * 100).toFixed(d) + "%";
export const OUTCOME = ["정상", "지급자 위반", "투자자 대표 상한 초과", "하한 없음(범위 밖·검증기관 거부)"];
export const explorer = (run: Run, sig: string) =>
  run.cluster === "devnet" ? `https://explorer.solana.com/tx/${sig}?cluster=devnet` : null;
export const findTx = (run: Run, prefix: string) => run.txs.find((t) => t.step.startsWith(prefix));

/** P1 record (scripts/run/p1.ts): Token-2022 tranche tokens gated by Token ACL. */
export type P1Run = {
  cluster: string; program: string; token_acl: string; gate: string; deal: string;
  mints: { tranche: string; address: string; default_account_state: string; token_acl: string | null; supply: string;
    mint_config: { address: string; gating_program: string; permissionless_thaw: boolean } }[];
  allow_list: { address: string; mode: string; wallets: number };
  holders: { name: string; listed: boolean; account: string; position: string; frozen_at_creation: boolean; state: string; balance: string; units: string }[];
  steps: { step: string; signature: string }[];
  rejected: { label: string; signature: string | null; err: string; expect: string }[];
  checks: Record<string, boolean>;
};
export function loadP1(): P1Run | null {
  const p = join(process.cwd(), "public/p1-run.json");
  return existsSync(p) ? JSON.parse(readFileSync(p, "utf8")) : null;
}
export const explorerOf = (cluster: string, kind: "tx" | "address", id: string) =>
  cluster === "devnet" ? `https://explorer.solana.com/${kind}/${id}?cluster=devnet` : null;
