// PRD 7.5 acceptance 1: every number the pages show is compared with the chain accounts it came from.
import {
  base58, decodeCampaignStatus, decodeDeal, decodeMetadataField, decodeMintSupply, decodePeriod, decodePosition, decodeTokenAmount,
  decodeTokenState, t22Extension, T22_DEFAULT_ACCOUNT_STATE, T22_TOKEN_METADATA,
} from "./decode";
import type { P1Run, Run } from "./run";

/** The part of run.json the comparison needs (keeps the transaction list out of the client bundle). */
export type ChainView = Pick<Run, "cluster" | "accounts" | "deal" | "schedule" | "periods" | "balances" | "token_accounts" | "positions">;
export const chainView = (r: Run): ChainView => ({ cluster: r.cluster, accounts: r.accounts, deal: r.deal, schedule: r.schedule,
  periods: r.periods, balances: r.balances, token_accounts: r.token_accounts, positions: r.positions });

export type Fetch = (addresses: string[]) => Promise<(Uint8Array | null)[]>;
export type Result = { total: number; bad: string[] };

export async function compareChain(run: ChainView, fetch: Fetch): Promise<Result> {
  const tokens = Object.entries(run.token_accounts ?? {});
  const pos = run.positions ?? [];
  const addrs = [run.accounts.deal, run.accounts.campaign, ...run.accounts.periods, ...tokens.map(([, a]) => a), ...pos.map((p) => p.address)];
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
  const pbase = base + tokens.length;
  pos.forEach((p, i) => {
    const b = raw[pbase + i];
    const k = `pos.${p.investor}.${p.tranche}`;
    if (!b) { want[k] = "account"; return; }
    const q = decodePosition(b);
    put(`${k}.units`, p.units, q.units);
    put(`${k}.claimed`, p.claimed, q.claimed);
  });
  const keys = Object.keys(want);
  return { total: keys.length, bad: keys.filter((k) => want[k] !== got[k]) };
}

/** P1 values shown on the invest page: mints, Token ACL configs, allow list, holder accounts and positions. */
export type P1View = Pick<P1Run, "cluster" | "gate" | "mints" | "allow_list" | "holders">;
export const p1View = (p: P1Run): P1View => ({ cluster: p.cluster, gate: p.gate, mints: p.mints, allow_list: p.allow_list, holders: p.holders });

export async function compareP1(p: P1View, fetch: Fetch): Promise<Result> {
  const addrs = [...p.mints.flatMap((m) => [m.address, m.mint_config.address]), p.allow_list.address,
    ...p.holders.flatMap((h) => [h.account, h.position])];
  const raw = await fetch(addrs);
  const want: Record<string, string> = {};
  const got: Record<string, string> = {};
  const put = (k: string, w: unknown, g: unknown) => { want[k] = String(w); got[k] = String(g); };
  p.mints.forEach((m, i) => {
    const b = raw[2 * i], c = raw[2 * i + 1], k = `p1.${m.tranche}`;
    if (!b) want[`${k}.mint`] = "account";
    else {
      put(`${k}.supply`, m.supply, decodeMintSupply(b));
      const das = t22Extension(b, T22_DEFAULT_ACCOUNT_STATE);
      put(`${k}.default_state`, m.default_account_state, das ? ["Uninitialized", "Initialized", "Frozen"][das[0]] : "none");
      const md = t22Extension(b, T22_TOKEN_METADATA);
      put(`${k}.token_acl`, m.token_acl, md ? decodeMetadataField(md, "token_acl") : "none");
    }
    if (!c) want[`${k}.mint_config`] = "account";
    else {
      put(`${k}.gate`, m.mint_config.gating_program, base58(c.subarray(68, 100)));
      put(`${k}.thaw`, m.mint_config.permissionless_thaw, c[2] === 1);
    }
  });
  const l = raw[2 * p.mints.length];
  if (!l) want["p1.allow_list"] = "account";
  else {
    put("p1.allow_list.mode", p.allow_list.mode, l[65] === 0 ? "allow" : l[65] === 2 ? "block" : "unused");
    put("p1.allow_list.wallets", p.allow_list.wallets, new DataView(l.buffer, l.byteOffset + 66, 8).getBigUint64(0, true));
  }
  const hb = 2 * p.mints.length + 1;
  p.holders.forEach((h, i) => {
    const a = raw[hb + 2 * i], q = raw[hb + 2 * i + 1], k = `p1.${h.name}`;
    if (!a) want[`${k}.account`] = "account";
    else {
      put(`${k}.balance`, h.balance, decodeTokenAmount(a));
      put(`${k}.state`, h.state, decodeTokenState(a) === 2 ? "frozen" : "initialized");
    }
    if (!q) want[`${k}.position`] = "account";
    else put(`${k}.units`, h.units, decodePosition(q).units);
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
