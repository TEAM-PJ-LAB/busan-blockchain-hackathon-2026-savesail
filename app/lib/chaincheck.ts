// PRD 7.5 acceptance 1: every number the pages show is compared with the chain accounts it came from.
import { decodeCampaignStatus, decodeDeal, decodePeriod, decodeTokenAmount } from "./decode";
import type { Run } from "./run";

/** The part of run.json the comparison needs (keeps the transaction list out of the client bundle). */
export type ChainView = Pick<Run, "cluster" | "accounts" | "deal" | "schedule" | "periods" | "balances" | "token_accounts">;
export const chainView = (r: Run): ChainView => ({ cluster: r.cluster, accounts: r.accounts, deal: r.deal, schedule: r.schedule,
  periods: r.periods, balances: r.balances, token_accounts: r.token_accounts });

export type Fetch = (addresses: string[]) => Promise<(Uint8Array | null)[]>;
export type Result = { total: number; bad: string[] };

export async function compareChain(run: ChainView, fetch: Fetch): Promise<Result> {
  const tokens = Object.entries(run.token_accounts ?? {});
  const addrs = [run.accounts.deal, run.accounts.campaign, ...run.accounts.periods, ...tokens.map(([, a]) => a)];
  const raw = await fetch(addrs);
  const want: Record<string, string> = {};
  const got: Record<string, string> = {};
  const put = (k: string, w: unknown, g: unknown) => { want[k] = String(w); got[k] = String(g); };
  const missing = (i: number) => !raw[i];

  if (!missing(0)) {
    const d = decodeDeal(raw[0]!);
    const r = run.deal;
    put("deal.entitlement", r.entitlement, d.entitlement);
    put("deal.paid_cum", r.paid_cum, d.paidCum);
    put("deal.reserve", r.reserve, d.reserve);
    put("deal.opex_arrears", r.opex_arrears, d.opexArrears);
    put("deal.senior_arrears", r.senior_arrears, d.seniorArrears);
    put("deal.acc0", r.acc[0], d.acc[0]);
    put("deal.acc1", r.acc[1], d.acc[1]);
    put("deal.last_lb_bps", r.last_lb_bps, d.lastLb);
    put("deal.phi_bps", r.phi_bps, d.phi);
    put("deal.ref_rate_bps", r.ref_rate_bps, d.ref);
    put("deal.comp_min_bps", r.comp_min_bps, d.compMin);
    put("deal.miss_max_bps", r.miss_max_bps, d.missMax);
    put("deal.install_cost", r.install_cost, d.installCost);
    r.caps.forEach((x, t) => put(`deal.caps${t}`, x, d.caps[t]));
    r.raised.forEach((x, t) => put(`deal.raised${t}`, x, d.raised[t]));
    put("deal.opex_per_period", r.opex_per_period, d.opexPerPeriod);
    put("deal.reserve_target", r.reserve_target, d.reserveTarget);
    put("deal.n_periods", r.n_periods, d.nPeriods);
    put("deal.senior_per_period", r.senior_per_period, d.seniorFirst);
  } else want["deal"] = "account";
  if (!missing(1)) {
    const st = decodeCampaignStatus(raw[1]!, run.schedule.length);
    run.schedule.forEach((s, g) => put(`group${g}`, `${s.off}/${s.missing}`, `${st[g] & 7}/${(st[g] >> 3) & 7}`));
  } else want["campaign"] = "account";
  run.periods.forEach((p, i) => {
    const b = raw[2 + i];
    if (!b) { want[`period${i}`] = "account"; return; }
    const q = decodePeriod(b);
    const y = p.payload;
    put(`p${i}.due`, p.due, q.due);
    put(`p${i}.outcome`, p.outcome, q.outcome);
    put(`p${i}.lb`, y.lb_bps, q.lb);
    put(`p${i}.est`, y.est_bps, q.est);
    put(`p${i}.ub`, y.ub_bps, q.ub);
    put(`p${i}.flags`, y.flags, q.flags);
    put(`p${i}.compliance`, y.compliance_bps, q.compliance);
    put(`p${i}.kg_on`, y.kg_on, q.kgOn);
    p.dist.forEach((x, k) => put(`p${i}.dist${k}`, x, q.dist[k]));
  });
  const base = 2 + run.accounts.periods.length;
  tokens.forEach(([name], i) => {
    const b = raw[base + i];
    if (!b) { want[`token.${name}`] = "account"; return; }
    put(`token.${name}`, run.balances[name], decodeTokenAmount(b));
  });
  const keys = Object.keys(want);
  return { total: keys.length, bad: keys.filter((k) => want[k] !== got[k]) };
}

export function rpcFetch(url: string): Fetch {
  return async (addresses) => {
    const out: (Uint8Array | null)[] = [];
    for (let i = 0; i < addresses.length; i += 100) {
      const r = await fetch(url, { method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getMultipleAccounts", params: [addresses.slice(i, i + 100), { encoding: "base64", commitment: "confirmed" }] }) });
      const j = await r.json();
      if (j.error) throw new Error(j.error.message);
      for (const a of j.result.value as ({ data: [string, string] } | null)[])
        out.push(a ? Uint8Array.from(atob(a.data[0]), (c) => c.charCodeAt(0)) : null);
    }
    return out;
  };
}
