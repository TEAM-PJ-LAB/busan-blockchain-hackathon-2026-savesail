// SaveSail executor (PRD §7.4): runs the §4 demo flow end to end and writes app/public/run.json.
//   tsx run/run.ts local [--quick]   LiteSVM with target/deploy/savesail.so
//   tsx run/run.ts devnet            RPC_URL, ROLE_SEED (and the program already deployed)
// Resumable: every step reads chain state first and skips what is already done.
import anchor from "@anchor-lang/core";
import type { Idl } from "@anchor-lang/core";
const { AnchorProvider, BN, Program, utils, Wallet } = anchor;
type BN = InstanceType<typeof anchor.BN>;
const bs58 = utils.bytes.bs58;
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Connection, Keypair, PublicKey, SystemProgram, type TransactionInstruction } from "@solana/web3.js";

import { spawn, execFileSync, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createHash, createHmac } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { DevnetChain, LocalChain, TxError, type Chain, type SentTx } from "./chain.js";
import { roleKeys, type Role } from "./keys.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const cfg = JSON.parse(readFileSync(join(ROOT, "scripts/config/demo.json"), "utf8"));
const idl = JSON.parse(readFileSync(join(ROOT, "idl/savesail.json"), "utf8")) as Idl;
const PROGRAM_ID = new PublicKey(idl.address);
const M = 1_000_000n; // 1 dUSD
const NO_OFF = 7;

const mode = process.argv[2] as "local" | "devnet";
const quick = process.argv.includes("--quick");
const sc = quick ? cfg.quick : cfg.scenario;
const outDir = join(ROOT, mode === "local" ? "target/run-local" : "target/run-devnet");
rmSync(join(outDir, "data"), { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
if (!process.env.RESUME) {
  rmSync(join(outDir, "txlog.jsonl"), { force: true });
  rmSync(join(outDir, "periods.jsonl"), { force: true });
}
const savedPeriods: Record<number, any> = {};
if (existsSync(join(outDir, "periods.jsonl"))) {
  for (const l of readFileSync(join(outDir, "periods.jsonl"), "utf8").trim().split("\n").filter(Boolean)) {
    const r = JSON.parse(l);
    savedPeriods[r.period] = r;
  }
}

// ------------------------------------------------------------------ setup
const chain: Chain = mode === "local"
  ? new LocalChain(PROGRAM_ID, join(ROOT, "target/deploy/savesail.so"), process.env.RESUME ? join(outDir, "svm.bin") : undefined)
  : new DevnetChain(process.env.RPC_URL ?? "https://api.devnet.solana.com");
const keys = roleKeys(mode === "local" ? "savesail-local-test-seed" : must("ROLE_SEED"));
const pk = (r: Role) => keys[r].publicKey;
const dummy = new AnchorProvider(new Connection("http://127.0.0.1:1"), new Wallet(keys.admin), {});
const program = new Program(idl, dummy);
const coder = program.coder;
const log: (SentTx & { step: string })[] = [];
const txlogPath = () => join(outDir, "txlog.jsonl");
const record: any = { cluster: mode, program: PROGRAM_ID.toBase58(), started: new Date().toISOString(), steps: [], periods: [] };

function must(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`missing env ${name}`);
  return v;
}
const pda = (...seeds: (Buffer | Uint8Array)[]) => PublicKey.findProgramAddressSync(seeds, PROGRAM_ID)[0];
const u64le = (n: bigint | number) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const u16le = (n: number) => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };
const hex32 = (h: string) => Array.from(Buffer.from(h, "hex"));

const mintKp = Keypair.fromSeed(createHmac("sha256", mode === "local" ? "local" : must("ROLE_SEED")).update(`savesail-dusd-mint-${process.env.DEAL_ID ?? sc.deal_id}`).digest());
const mint = mintKp.publicKey;
const dealId = BigInt(process.env.DEAL_ID ?? sc.deal_id); // devnet: a fresh deal for every run
const deal = pda(Buffer.from("deal"), pk("admin").toBuffer(), u64le(dealId));
const campaign = pda(Buffer.from("campaign"), deal.toBuffer(), Buffer.from([0]));
// Vaults are keypair accounts created once by init_vaults (their addresses are stored in Deal.vaults).
// Deterministic per deal so a resumed run names the same accounts.
const vaultKps = [0, 1, 2, 3, 4, 5].map((k) => Keypair.fromSeed(createHmac("sha256", mode === "local" ? "local" : must("ROLE_SEED"))
  .update(`savesail-vault-${dealId}-${k}`).digest()));
const vault = (k: number) => vaultKps[k].publicKey;
const kycPda = (inv: PublicKey) => pda(Buffer.from("kyc"), deal.toBuffer(), inv.toBuffer());
const posPda = (t: number, o: PublicKey) => pda(Buffer.from("pos"), deal.toBuffer(), Buffer.from([t]), o.toBuffer());
const periodPda = (p: number) => pda(Buffer.from("period"), deal.toBuffer(), u16le(p));
const ata = (o: PublicKey) => getAssociatedTokenAddressSync(mint, o, true);

const STOP_AFTER = Number(process.env.STOP_AFTER ?? 0); // test hook: simulate a crash after N transactions
let queue: Promise<unknown> = Promise.resolve();
function send(step: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx> {
  if (chain.kind === "devnet") return sendNow(step, ixs, signers);
  // local: one transaction at a time so a simulated crash never leaves an executed but unlogged transaction
  const p = queue.then(() => sendNow(step, ixs, signers));
  queue = p.catch(() => undefined);
  return p;
}
async function sendNow(step: string, ixs: TransactionInstruction[], signers: Keypair[]): Promise<SentTx> {
  const t = await chain.send(step, ixs, signers);
  if (chain.kind === "local") t.signature = bs58.encode(Buffer.from(t.signature, "hex"));
  log.push({ ...t, step });
  appendFileSync(txlogPath(), JSON.stringify({ ...t, step }) + "\n");
  if (STOP_AFTER && log.length >= STOP_AFTER) {
    if (chain instanceof LocalChain) chain.save(join(outDir, "svm.bin"));
    console.log(`simulated crash after ${step}`);
    process.exit(3);
  }
  return t;
}
async function expectFail(step: string, ixs: TransactionInstruction[], signers: Keypair[], code: string) {
  try {
    await chain.send(step, ixs, signers);
  } catch (e) {
    const msg = e instanceof TxError ? e.message : String(e);
    if (!msg.includes(code)) throw new Error(`${step}: expected ${code}, got ${msg}`);
    record.steps.push({ step, rejected: code });
    return;
  }
  throw new Error(`${step}: expected rejection ${code}`);
}
async function decode(name: string, addr: PublicKey): Promise<any | null> {
  const d = await chain.account(addr);
  return d ? (coder.accounts as any).decode(name[0].toLowerCase() + name.slice(1), d) : null;
}

// ------------------------------------------------------------------ python helpers (synth, verifier)
let synth: ChildProcessWithoutNullStreams;
let synthLines: AsyncIterator<string>;
function startSynth() {
  const scfg = { seed: sc.synth_seed, ...sc.synth };
  writeFileSync(join(outDir, "synth_config.json"), JSON.stringify(scfg));
  synth = spawn("python3", [join(ROOT, "offchain/synth/synth.py"), "serve", join(outDir, "synth_config.json"), join(outDir, "data")]);
  synth.stderr.on("data", (d) => process.stderr.write(d));
  synthLines = createInterface({ input: synth.stdout })[Symbol.asyncIterator]();
}
async function synthGroup(g: number, off: number, viol: boolean) {
  synth.stdin.write(JSON.stringify({ group: g, off_index: off, viol }) + "\n");
  const r = await synthLines.next();
  return JSON.parse(r.value as string) as { root_ops: string; root_ctrl: string };
}
function envelope() {
  const env = JSON.parse(readFileSync(join(ROOT, "offchain/verifier/envelope.template.json"), "utf8"));
  env.meter_public_key = JSON.parse(readFileSync(join(outDir, "data/meter_key.json"), "utf8")).public_key;
  env.mcr_kw = JSON.parse(readFileSync(join(outDir, "data/engine.json"), "utf8")).mcr_kw;
  env.engine_curve_sha256 = createHash("sha256").update(readFileSync(join(outDir, "data/engine_curve.csv"))).digest("hex");
  env.train_h = sc.synth.train_h;
  env.min_training_h = sc.synth.train_h;
  const p = join(outDir, "envelope.json");
  writeFileSync(p, JSON.stringify(env, null, 1));
  const h = execFileSync("python3", [join(ROOT, "offchain/verifier/verifier.py"), "hash", p]).toString().trim();
  return { path: p, hash: h };
}
function verify(chainJson: object, envPath: string, p: number) {
  const cp = join(outDir, `chain_${p}.json`);
  writeFileSync(cp, JSON.stringify(chainJson));
  execFileSync("python3", [join(ROOT, "offchain/verifier/verifier.py"), join(outDir, "data"), cp, envPath, join(outDir, "attest")]);
  return JSON.parse(readFileSync(join(outDir, `attest/payload_${p}.json`), "utf8"));
}

// ------------------------------------------------------------------ flow
async function main() {
  const t0 = Date.now();
  const gpp = sc.groups_per_period as number;
  const nGroups = sc.n_groups as number;
  const periods = nGroups / gpp;
  startSynth();
  // wait for the synth to write the fixed inputs (training data, meter key) before building the envelope
  const keyFile = join(outDir, "data/meter_key.json");
  for (let i = 0; i < 600 && !existsSync(keyFile); i++) await new Promise((r) => setTimeout(r, 100));
  const env = envelope();

  // 1. lamports and the test dollar
  for (const r of Object.keys(keys) as Role[]) if (r !== "admin") await chain.fund(keys.admin, pk(r), 50_000_000);
  if (chain.kind === "local") await chain.fund(keys.admin, pk("admin"), 4_000_000_000);
  if (!(await chain.account(mint))) {
    const rent = 1_461_600;
    await send("create_mint", [
      SystemProgram.createAccount({ fromPubkey: pk("admin"), newAccountPubkey: mint, lamports: rent, space: MINT_SIZE, programId: TOKEN_PROGRAM_ID }),
      createInitializeMint2Instruction(mint, 6, pk("admin"), null),
    ], [keys.admin, mintKp]);
  }
  const balances: Record<string, bigint> = { payer: 3_000_000n * M, investor_rep: 10_000n * M, verifier0: 10_000n * M,
    investorA: 200_000n * M, investorB: 200_000n * M, investorC: 4_000_000n * M };
  for (const r of ["payer", "investor_rep", "verifier0", "verifier1", "verifier2", "manufacturer", "opex", "investorA", "investorB", "investorC"] as Role[]) {
    if (await chain.account(ata(pk(r)))) continue;
    const ixs = [createAssociatedTokenAccountIdempotentInstruction(pk("admin"), ata(pk(r)), pk(r), mint)];
    if (balances[r]) ixs.push(createMintToInstruction(mint, ata(pk(r)), pk("admin"), balances[r]));
    await send(`token_account_${r}`, ixs, [keys.admin]);
  }

  // 2. deal, vaults, KYC, fundraising (PRD §4 step 2)
  const sched = Array.from({ length: sc.senior_periods }, () => new BN(sc.senior_per_period));
  const params = {
    dealId: new BN(dealId.toString()), payer: pk("payer"), investorRep: pk("investor_rep"),
    verifiers: [pk("verifier0"), pk("verifier1"), pk("verifier2")], manufacturer: pk("manufacturer"), opexToken: ata(pk("opex")),
    envelopeHash: hex32(env.hash), phiBps: sc.phi_bps, refRateBps: 600, compMinBps: 9500, missMaxBps: 50,
    installCost: new BN(sc.install_cost), caps: [new BN(sc.caps[0]), new BN(sc.caps[1])], opexPerPeriod: new BN(sc.opex_per_period),
    reserveTarget: new BN(sc.reserve_target), seniorSchedule: sched, nGroups, groupsPerPeriod: gpp,
    revealTimeoutSlots: new BN(sc.reveal_timeout_slots), bondAmount: new BN(sc.bond), slashPerMiss: new BN(sc.slash),
  };
  let d = await decode("Deal", deal);
  if (!d) {
    const ix = await program.methods.initializeDeal(params).accountsStrict({ admin: pk("admin"), deal, campaign, paymentMint: mint, systemProgram: SystemProgram.programId }).instruction();
    await send("initialize_deal", [ix], [keys.admin]);
    d = await decode("Deal", deal);
  }
  if (!d.vaultsReady) {
    const ix = await program.methods.initVaults().accountsStrict({ admin: pk("admin"), deal, paymentMint: mint, raiseVault: vault(0), collectionVault: vault(1),
      seniorVault: vault(2), juniorVault: vault(3), reserveVault: vault(4), bondVault: vault(5), tokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId }).instruction();
    await send("init_vaults", [ix], [keys.admin, ...vaultKps]);
    d = await decode("Deal", deal);
  }
  if (d.vaults.some((v: PublicKey, k: number) => !v.equals(vault(k)))) throw new Error("Deal.vaults differ from the derived vault keys");
  const kyc = async (r: Role, tier: number, limit: string) => {
    if (await decode("Kyc", kycPda(pk(r)))) return;
    const ix = await program.methods.setKyc(pk(r), tier, new BN(limit)).accountsStrict({ admin: pk("admin"), deal, kyc: kycPda(pk(r)), systemProgram: SystemProgram.programId }).instruction();
    await send(`set_kyc_${r}`, [ix], [keys.admin]);
  };
  await kyc("investorA", 0, sc.retail_limit);
  await kyc("investorB", 0, sc.retail_limit);
  await kyc("investorC", 1, sc.pro_limit);
  const investIx = (r: Role, tranche: number, amount: string) => program.methods.invest(tranche, new BN(amount)).accountsStrict({
    investor: pk(r), deal, kyc: kycPda(pk(r)), position: posPda(tranche, pk(r)), paymentMint: mint, investorToken: ata(pk(r)),
    raiseVault: vault(0), tokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId }).instruction();
  d = await decode("Deal", deal);
  if (d.status === 0) {
    const has = async (r: Role, t: number) => !!(await decode("Position", posPda(t, pk(r))));
    if (!(await has("investorA", 0))) {
      await expectFail("invest_A_junior", [await investIx("investorA", 1, "1000000")], [keys.investorA], "TierNotAllowed");
      await send("invest_A_senior", [await investIx("investorA", 0, sc.retail_limit)], [keys.investorA]);
    }
    await expectFail("invest_A_over_limit", [await investIx("investorA", 0, "1000000")], [keys.investorA], "LimitExceeded");
    if (!(await has("investorB", 0))) await send("invest_B_senior", [await investIx("investorB", 0, sc.retail_limit)], [keys.investorB]);
    const rest = BigInt(sc.caps[0]) - 2n * BigInt(sc.retail_limit);
    if (!(await has("investorC", 0))) await send("invest_C_senior", [await investIx("investorC", 0, rest.toString())], [keys.investorC]);
    if (!(await has("investorC", 1))) await send("invest_C_junior", [await investIx("investorC", 1, sc.caps[1])], [keys.investorC]);
    const ix = await program.methods.closeFundraising().accountsStrict({ admin: pk("admin"), deal, paymentMint: mint, raiseVault: vault(0),
      manufacturerToken: ata(pk("manufacturer")), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
    await send("close_fundraising", [ix], [keys.admin]);
  }

  // 3. schedule anchors (PRD §6.4): hash chains c_0 = s_r, c_i = H(c_{i-1}); anchor c_N, group g reveals c_{N-1-g}
  const N = nGroups;
  const chainOf = (role: string) => {
    const seed = createHmac("sha256", mode === "local" ? "local" : must("ROLE_SEED")).update(`savesail-chain-${role}-${dealId}`).digest();
    const c = [seed];
    for (let i = 0; i < N; i++) c.push(createHash("sha256").update(c[c.length - 1]).digest());
    return c;
  };
  const chains = [chainOf("payer"), chainOf("investor_rep"), chainOf("verifier")];
  const roleSigner: Role[] = ["payer", "investor_rep", "verifier0"];
  let cam = await decode("Campaign", campaign);
  for (let r = 0; r < 3; r++) {
    if (cam.anchorsSet & (1 << r)) continue;
    const ix = await program.methods.registerAnchor(r, Array.from(chains[r][N])).accountsStrict({ party: pk(roleSigner[r]), deal, campaign,
      paymentMint: mint, partyToken: ata(pk(roleSigner[r])), bondVault: vault(5), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
    await send(`register_anchor_${r}`, [ix], [keys[roleSigner[r]]]);
  }

  // 4. groups: reveal (verifier first, C18) -> OFF position from the chain -> synth -> data commits
  const revealIx = (r: number, g: number) => program.methods.reveal(r, g, Array.from(chains[r][N - 1 - g])).accountsStrict({
    party: pk(roleSigner[r]), deal, campaign }).instruction();
  const withhold: Record<string, number> = sc.withhold ?? {};
  const viol = new Set<number>(sc.off_violations ?? []);
  const commitIx = (kind: number, g: number, root: string) => program.methods.commitGroupData(kind, g, hex32(root)).accountsStrict({
    committer: pk(kind === 0 ? "payer" : "manufacturer"), deal, campaign }).instruction();
  const distributed = async (p: number) => (await decode("PeriodState", periodPda(p)))?.distributed;
  cam = await decode("Campaign", campaign);
  for (let g = 0; g < nGroups; g++) {
    const p = Math.floor(g / gpp);
    cam = await decode("Campaign", campaign);
    if (g >= cam.currentGroup) {
      const w = withhold[String(g)];
      const pend = cam.currentGroup === g ? cam.pending : 0;
      if (w !== 2 && !(pend & 4)) await send(`reveal_${g}_verifier`, [await revealIx(2, g)], [keys.verifier0]);
      const rest: Promise<SentTx>[] = [];
      for (const r of [0, 1]) if (w !== r && w !== 2 && !(pend & (1 << r))) rest.push(revealIx(r, g).then((ix) => send(`reveal_${g}_${r}`, [ix], [keys[roleSigner[r]]])));
      await Promise.all(rest);
      if (w !== undefined) {
        await chain.waitSlots(sc.reveal_timeout_slots + 1);
        const ix = await program.methods.resolveMissing(g).accountsStrict({ caller: pk("admin"), deal, campaign, paymentMint: mint, bondVault: vault(5),
          reserveVault: vault(4), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
        await send(`resolve_missing_${g}`, [ix], [keys.admin]);
      }
    }
    cam = await decode("Campaign", campaign);
    if (g >= cam.nextDataGroup[0] || g >= cam.nextDataGroup[1]) {
      const st = cam.status[g];
      const roots = await synthGroup(g, st & 7, viol.has(g));
      const c2 = await decode("Campaign", campaign);
      await Promise.all([
        g >= c2.nextDataGroup[0] ? commitIx(0, g, roots.root_ops).then((ix) => send(`commit_ops_${g}`, [ix], [keys.payer])) : null,
        g >= c2.nextDataGroup[1] ? commitIx(1, g, roots.root_ctrl).then((ix) => send(`commit_ctrl_${g}`, [ix], [keys.manufacturer])) : null,
      ]);
    } else {
      const st = cam.status[g];
      await synthGroup(g, st & 7, viol.has(g)); // regenerate local data on resume
    }
    if ((g + 1) % gpp === 0) await settlePeriod(p, gpp, env, !!(await distributed(p)));
  }

  // 6. claims and bond release
  const claimIx = (r: Role, t: number) => program.methods.claim(t).accountsStrict({ owner: pk(r), deal, position: posPda(t, pk(r)), paymentMint: mint,
    trancheVault: vault(2 + t), ownerToken: ata(pk(r)), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
  for (const [r, t] of [["investorA", 0], ["investorB", 0], ["investorC", 0], ["investorC", 1]] as [Role, number][]) {
    try { await send(`claim_${r}_${t}`, [await claimIx(r, t)], [keys[r]]); } catch (e) { if (!String(e).includes("NothingToClaim")) throw e; }
  }
  cam = await decode("Campaign", campaign);
  if (!cam.bondsReleased) {
    const ix = await program.methods.releaseBonds().accountsStrict({ caller: pk("admin"), deal, campaign, paymentMint: mint, bondVault: vault(5),
      payerToken: ata(pk("payer")), investorRepToken: ata(pk("investor_rep")), verifierToken: ata(pk("verifier0")), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
    await send("release_bonds", [ix], [keys.admin]);
  }
  synth.stdin.end();

  // 7. run.json (demo-mode input for the web; every number is read back from the chain)
  d = await decode("Deal", deal);
  const tokenBal = async (a: PublicKey) => { const b = await chain.account(a); return b ? b.readBigUInt64LE(64).toString() : "0"; };
  record.finished = new Date().toISOString();
  record.scenario = { withhold: sc.withhold ?? {}, off_violations: sc.off_violations ?? [], groups_per_period: gpp };
  record.seconds = Math.round((Date.now() - t0) / 1000);
  record.accounts = { deal: deal.toBase58(), campaign: campaign.toBase58(), mint: mint.toBase58(),
    vaults: Object.fromEntries([0, 1, 2, 3, 4, 5].map((k) => [k, vault(k).toBase58()])),
    periods: Array.from({ length: periods }, (_, q) => periodPda(q).toBase58()),
    roles: Object.fromEntries((Object.keys(keys) as Role[]).map((r) => [r, pk(r).toBase58()])) };
  record.deal = { phi_bps: d.phiBps, ref_rate_bps: d.refRateBps, entitlement: d.entitlement.toString(), paid_cum: d.paidCum.toString(),
    last_lb_bps: d.lastLbBps, acc: d.acc.map((x: BN) => x.toString()), reserve: d.reserve.toString(), opex_arrears: d.opexArrears.toString(),
    senior_arrears: d.seniorArrears.toString(), install_cost: d.installCost.toString(), caps: d.caps.map((x: BN) => x.toString()),
    raised: d.raised.map((x: BN) => x.toString()), comp_min_bps: d.compMinBps, miss_max_bps: d.missMaxBps, opex_per_period: d.opexPerPeriod.toString(),
    reserve_target: d.reserveTarget.toString(), n_periods: d.nPeriods, senior_per_period: d.seniorSchedule[0].toString(),
    n_groups: cam.nGroups, groups_per_period: cam.groupsPerPeriod, bond_amount: cam.bondAmount.toString(), slash_per_miss: cam.slashPerMiss.toString(),
    retail_limit: String(sc.retail_limit) };
  const tokenAccounts: [string, PublicKey][] = [["senior_vault", vault(2)], ["junior_vault", vault(3)], ["reserve_vault", vault(4)],
    ["collection_vault", vault(1)], ["bond_vault", vault(5)], ["raise_vault", vault(0)],
    ...(["investorA", "investorB", "investorC", "payer", "opex", "manufacturer"] as Role[]).map((r): [string, PublicKey] => [r, ata(pk(r))])];
  record.token_accounts = Object.fromEntries(tokenAccounts.map(([k, a]) => [k, a.toBase58()]));
  record.balances = Object.fromEntries(await Promise.all(tokenAccounts.map(async ([k, a]) => [k, await tokenBal(a)])));
  cam = await decode("Campaign", campaign);
  record.schedule = Array.from(cam.status.slice(0, nGroups) as number[]).map((s) => ({ off: s & 7, missing: (s >> 3) & 7 }));
  const all = readFileSync(txlogPath(), "utf8").trim().split("\n").map((l) => JSON.parse(l));
  record.txs = all.map((t) => ({ step: t.step, sig: t.signature, slot: t.slot, cu: t.cu }));
  record.tx_count = all.length;
  const outPath = join(ROOT, mode === "local" ? "target/run-local/run.json" : "app/public/run.json");
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, JSON.stringify(record, null, 1));
  if (chain instanceof LocalChain) chain.save(join(outDir, "svm.bin")); // final state for check_web_chain.ts
  console.log(`done: ${log.length} transactions in ${record.seconds}s -> ${outPath}`);
}

async function settlePeriod(p: number, gpp: number, env: { path: string; hash: string }, done: boolean) {
  const cam = await decode("Campaign", campaign);
  const prev: number[] = [];
  for (let q = 0; q < p; q++) prev.push((await decode("PeriodState", periodPda(q))).outcome);
  const chainJson = { period: p, groups_per_period: gpp, status: Array.from(cam.status as number[]),
    snap_ops: Buffer.from(cam.periodSnap[p][0]).toString("hex"), snap_ctrl: Buffer.from(cam.periodSnap[p][1]).toString("hex"),
    prev_outcomes: prev, price_micro_per_t: 750_000_000 };
  const pl = verify(chainJson, env.path, p); // re-run on resume too: the payload is deterministic (PRD 7.2-10 2)
  const payload = { period: p, dataRootOps: hex32(pl.data_root_ops), dataRootCtrl: hex32(pl.data_root_ctrl), kgOn: new BN(pl.kg_on),
    kgMissPayer: new BN(pl.kg_miss_payer), kgMissInv: new BN(pl.kg_miss_inv), overrideGroups: pl.override_groups,
    priceMicroPerT: new BN(pl.price_micro_per_t), lbBps: pl.lb_bps, estBps: pl.est_bps, ubBps: pl.ub_bps, complianceBps: pl.compliance_bps,
    flags: pl.flags, envelopeHash: hex32(pl.envelope_hash), reportHash: hex32(pl.report_hash) };
  let ps = await decode("PeriodState", periodPda(p));
  const ph = createHash("sha256").update((program.coder.types as any).encode("attestationPayload", payload)).digest();
  for (const [i, v] of (["verifier0", "verifier1"] as Role[]).entries()) {
    if (ps?.finalized) break;
    if (ps && ps.voted & (1 << i) && Buffer.from(ps.votes[i]).equals(ph)) continue; // vote already recorded
    const ix = await program.methods.approveAttestation(payload).accountsStrict({ verifier: pk(v), deal, campaign, periodState: periodPda(p),
      systemProgram: SystemProgram.programId }).instruction();
    await send(`approve_${p}_${v}`, [ix], [keys[v]]);
    ps = await decode("PeriodState", periodPda(p));
  }
  if (!done && !ps.settled) {
    const ix = await program.methods.settle(p).accountsStrict({ payer: pk("payer"), deal, periodState: periodPda(p), paymentMint: mint,
      payerToken: ata(pk("payer")), collectionVault: vault(1), tokenProgram: TOKEN_PROGRAM_ID }).instruction();
    await send(`settle_${p}`, [ix], [keys.payer]);
  }
  if (!(await decode("PeriodState", periodPda(p))).distributed) {
    const ix = await program.methods.distribute(p).accountsStrict({ caller: pk("admin"), deal, periodState: periodPda(p), paymentMint: mint,
      collectionVault: vault(1), seniorVault: vault(2), juniorVault: vault(3), reserveVault: vault(4), opexToken: ata(pk("opex")),
      tokenProgram: TOKEN_PROGRAM_ID }).instruction();
    await send(`distribute_${p}`, [ix], [keys.admin]);
  }
  if (savedPeriods[p]) {
    record.periods.push(savedPeriods[p]);
    return;
  }
  ps = await decode("PeriodState", periodPda(p));
  const dl = await decode("Deal", deal); // still this period's entitlement: the next period is not finalized yet
  const report = JSON.parse(readFileSync(join(outDir, `attest/report_${p}.json`), "utf8"));
  const rec = { period: p, payload: pl, outcome: ps.outcome, due: ps.due.toString(), entitlement: dl.entitlement.toString(),
    dist: ps.dist.map((x: BN) => x.toString()), lb: report.lb, est: report.est, ub: report.ub, flags: report.flags,
    excluded: Object.keys(report.groups.excluded).length };
  appendFileSync(join(outDir, "periods.jsonl"), JSON.stringify(rec) + "\n");
  record.periods.push(rec);
  console.log(`period ${p}: outcome ${ps.outcome} due ${ps.due.toString()} lb/est/ub ${pl.lb_bps}/${pl.est_bps}/${pl.ub_bps} flags ${pl.flags}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
