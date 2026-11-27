// P1 run (PRD §2.1 P1-1 ②, §12 Q6): a deal whose two tranche tokens are Token-2022 mints with
// DefaultAccountState=Frozen, the SaveSail transfer hook and `token_acl` metadata, gated by Token ACL and
// its ABL allow list. Listed wallets thaw their own accounts and receive (invest, transfer through the
// hook); a KYC-approved wallet off the list cannot thaw or receive. Writes app/public/p1-run.json.
//   tsx run/p1.ts local    LiteSVM: target/deploy/savesail.so + programs/savesail/tests/fixtures
//   tsx run/p1.ts devnet   RPC_URL, ROLE_SEED, DEAL_ID; program deployed, Token ACL and the gate on devnet
// Resumable: each step reads chain state first.
import anchor from "@anchor-lang/core";
import type { Idl } from "@anchor-lang/core";
const { AnchorProvider, BN, Program, utils, Wallet } = anchor;
const bs58 = utils.bytes.bs58;
import {
  AccountState,
  AuthorityType,
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeDefaultAccountStateInstruction,
  createInitializeMetadataPointerInstruction,
  createInitializeMint2Instruction,
  createInitializeTransferHookInstruction,
  createMintToInstruction,
  createSetAuthorityInstruction,
  createTransferCheckedInstruction,
  ExtensionType,
  getAssociatedTokenAddressSync,
  getDefaultAccountState,
  getExtensionData,
  getExtensionTypes,
  getMetadataPointerState,
  getMintLen,
  getTransferHook,
  MINT_SIZE,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  unpackAccount,
  unpackMint,
} from "@solana/spl-token";
import { createInitializeInstruction, createUpdateFieldInstruction, unpack as unpackMetadata } from "@solana/spl-token-metadata";
import { Connection, Keypair, PublicKey, SystemProgram, TransactionInstruction, type AccountMeta } from "@solana/web3.js";
import { createHash, createHmac } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DevnetChain, LocalChain, type Chain, type FailedTx, type SentTx } from "./chain.js";
import { roleKeys, type Role } from "./keys.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const idl = JSON.parse(readFileSync(join(ROOT, "idl/savesail.json"), "utf8")) as Idl;
const PROGRAM_ID = new PublicKey(idl.address);
const TACL = new PublicKey("TACLkU6CiCdkQN2MjoyDkVg2yAH9zkxiHDsiztQ52TP");
const GATE = new PublicKey("GATEzzqxhJnsWF6vHRsgtixxSB8PaQdcqGEVTEHWiULz");
const T22 = TOKEN_2022_PROGRAM_ID;
const M = 1_000_000n;
const DEC = 6;

const mode = process.argv[2] as "local" | "devnet";
if (mode !== "local" && mode !== "devnet") throw new Error("usage: tsx run/p1.ts local|devnet");
const seed = mode === "local" ? "savesail-local-test-seed" : must("ROLE_SEED");
const fixtures = join(ROOT, "programs/savesail/tests/fixtures");
const chain: Chain = mode === "local"
  ? new LocalChain(PROGRAM_ID, join(ROOT, "target/deploy/savesail.so"), undefined, [[TACL, join(fixtures, "token_acl.so")], [GATE, join(fixtures, "token_acl_gate.so")]])
  : new DevnetChain(process.env.RPC_URL ?? "https://api.devnet.solana.com");
const keys = roleKeys(seed);
const pk = (r: Role) => keys[r].publicKey;
const derive = (tag: string) => Keypair.fromSeed(createHmac("sha256", seed).update(tag).digest());
const program = new Program(idl, new AnchorProvider(new Connection("http://127.0.0.1:1"), new Wallet(keys.admin), {}));
const coder = program.coder;

const dealId = BigInt(process.env.DEAL_ID ?? "900001");
const outsider = derive(`savesail-p1-outsider-${dealId}`); // KYC-approved, never added to the allow list
const dusdKp = derive(`savesail-p1-dusd-${dealId}`);
const dusd = dusdKp.publicKey;
const trancheKps = [0, 1].map((t) => derive(`savesail-p1-tranche-${dealId}-${t}`));
const tranche = trancheKps.map((k) => k.publicKey);
const vaultKps = [0, 1, 2, 3, 4, 5].map((k) => derive(`savesail-p1-vault-${dealId}-${k}`));
const vault = (k: number) => vaultKps[k].publicKey;
const listSeed = new PublicKey(createHmac("sha256", seed).update(`savesail-p1-list-${dealId}`).digest());

const find = (seeds: (Buffer | Uint8Array)[], prog: PublicKey) => PublicKey.findProgramAddressSync(seeds, prog)[0];
const pda = (...s: (Buffer | Uint8Array)[]) => find(s, PROGRAM_ID);
const u64le = (n: bigint) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; };
const deal = pda(Buffer.from("deal"), pk("admin").toBuffer(), u64le(dealId));
const campaign = pda(Buffer.from("campaign"), deal.toBuffer(), Buffer.from([0]));
const kycPda = (o: PublicKey) => pda(Buffer.from("kyc"), deal.toBuffer(), o.toBuffer());
const posPda = (t: number, o: PublicKey) => pda(Buffer.from("pos"), deal.toBuffer(), Buffer.from([t]), o.toBuffer());
const hookMetas = (m: PublicKey) => pda(Buffer.from("extra-account-metas"), m.toBuffer());
const mintConfig = (m: PublicKey) => find([Buffer.from("MINT_CONFIG"), m.toBuffer()], TACL);
const gateMetas = (m: PublicKey) => find([Buffer.from("thaw_extra_account_metas"), m.toBuffer()], GATE);
const list = find([Buffer.from("list_config"), pk("admin").toBuffer(), listSeed.toBuffer()], GATE);
const walletEntry = (w: PublicKey) => find([Buffer.from("wallet_entry"), list.toBuffer(), w.toBuffer()], GATE);
const flagPda = (ta: PublicKey) => find([Buffer.from("FLAG_ACCOUNT"), ta.toBuffer()], TACL);
const dusdAta = (o: PublicKey) => getAssociatedTokenAddressSync(dusd, o, true, TOKEN_PROGRAM_ID);
const tAta = (o: PublicKey, t: number) => getAssociatedTokenAddressSync(tranche[t], o, true, T22);
const rent = (len: number) => (len + 128) * 6960; // default rent, 2 years exempt

function must(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`missing env ${name}`);
  return v;
}
const meta = (pubkey: PublicKey, isWritable = false, isSigner = false): AccountMeta => ({ pubkey, isWritable, isSigner });
const raw = (programId: PublicKey, keys: AccountMeta[], data: number[] | Buffer) => new TransactionInstruction({ programId, keys, data: Buffer.from(data) });

const record: any = {
  cluster: mode, program: PROGRAM_ID.toBase58(), token_acl: TACL.toBase58(), gate: GATE.toBase58(), started: new Date().toISOString(),
  deal: deal.toBase58(), deal_id: dealId.toString(), steps: [] as (SentTx & { step: string })[], rejected: [] as (FailedTx & { expect: string })[],
};
async function send(step: string, ixs: TransactionInstruction[], signers: Keypair[]) {
  const t = await chain.send(step, ixs, signers);
  if (chain.kind === "local") t.signature = bs58.encode(Buffer.from(t.signature, "hex"));
  record.steps.push({ ...t, step });
  console.log(`ok   ${step} ${t.signature}`);
  return t;
}
/** must fail with a log line containing `expect` (on devnet the failed transaction is on chain) */
async function reject(step: string, ixs: TransactionInstruction[], signers: Keypair[], expect: string) {
  const f = await chain.sendFail(step, ixs, signers);
  const hit = [f.err, ...f.logs].some((l) => l.includes(expect));
  if (!hit) throw new Error(`${step}: expected "${expect}", got ${f.err}\n${f.logs.slice(-6).join("\n")}`);
  if (f.signature && chain.kind === "local") f.signature = null;
  record.rejected.push({ ...f, logs: f.logs.slice(-6), expect });
  console.log(`fail ${step} (${expect}) ${f.signature ?? ""}`);
}
const decode = async (name: string, a: PublicKey) => {
  const d = await chain.account(a);
  return d ? (coder.accounts as any).decode(name[0].toLowerCase() + name.slice(1), d) : null;
};
const info = (data: Buffer, owner: PublicKey) => ({ data, owner, lamports: 0, executable: false });

async function main() {
  const t0 = Date.now();
  if (chain.kind === "devnet") {
    const c = (chain as DevnetChain).conn;
    for (const [name, id] of [["token_acl", TACL], ["gate", GATE], ["savesail", PROGRAM_ID]] as const) {
      const a = await c.getAccountInfo(id);
      if (!a?.executable) throw new Error(`${name} ${id.toBase58()} is not deployed on devnet`);
    }
    record.programs_on_devnet = true;
  }
  // 1. lamports, test dollar
  for (const w of [pk("investorA"), pk("investorB"), outsider.publicKey]) await chain.fund(keys.admin, w, 60_000_000);
  if (chain.kind === "local") await chain.fund(keys.admin, pk("admin"), 4_000_000_000);
  if (!(await chain.account(dusd))) {
    await send("create_dusd", [
      SystemProgram.createAccount({ fromPubkey: pk("admin"), newAccountPubkey: dusd, lamports: rent(MINT_SIZE), space: MINT_SIZE, programId: TOKEN_PROGRAM_ID }),
      createInitializeMint2Instruction(dusd, DEC, pk("admin"), null),
    ], [keys.admin, dusdKp]);
    const ixs: TransactionInstruction[] = [];
    for (const w of [pk("investorA"), pk("investorB"), outsider.publicKey]) {
      ixs.push(createAssociatedTokenAccountIdempotentInstruction(pk("admin"), dusdAta(w), w, dusd));
      ixs.push(createMintToInstruction(dusd, dusdAta(w), pk("admin"), 100_000n * M));
    }
    await send("dusd_accounts", ixs, [keys.admin]);
  }

  // 2. deal in fundraising with three KYC records (PRD §4 step 2)
  if (!(await decode("Deal", deal))) {
    const params = {
      dealId: new BN(dealId.toString()), payer: pk("payer"), investorRep: pk("investor_rep"),
      verifiers: [pk("verifier0"), pk("verifier1"), pk("verifier2")], manufacturer: pk("manufacturer"), opexToken: dusdAta(pk("investorA")),
      envelopeHash: Array.from(createHash("sha256").update("savesail-p1-run").digest()), phiBps: 7850, refRateBps: 600, compMinBps: 9500,
      missMaxBps: 50, installCost: new BN("400000000000"), caps: [new BN("100000000000"), new BN("300000000000")],
      opexPerPeriod: new BN("1000000000"), reserveTarget: new BN("2000000000"), seniorSchedule: [new BN("1000000000"), new BN("1000000000")],
      nGroups: 4, groupsPerPeriod: 2, revealTimeoutSlots: new BN(25), bondAmount: new BN("1000000"), slashPerMiss: new BN("300000"),
    };
    const ix = await program.methods.initializeDeal(params).accountsStrict({ admin: pk("admin"), deal, campaign, paymentMint: dusd, systemProgram: SystemProgram.programId }).instruction();
    await send("initialize_deal", [ix], [keys.admin]);
  }
  if (!(await decode("Deal", deal)).vaultsReady) {
    const ix = await program.methods.initVaults().accountsStrict({ admin: pk("admin"), deal, paymentMint: dusd, raiseVault: vault(0), collectionVault: vault(1),
      seniorVault: vault(2), juniorVault: vault(3), reserveVault: vault(4), bondVault: vault(5), tokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId }).instruction();
    await send("init_vaults", [ix], [keys.admin, ...vaultKps]);
  }
  for (const [name, w] of [["A", pk("investorA")], ["B", pk("investorB")], ["outsider", outsider.publicKey]] as const) {
    if (await chain.account(kycPda(w))) continue;
    const ix = await program.methods.setKyc(w, 0, new BN("69930000000")).accountsStrict({ admin: pk("admin"), deal, kyc: kycPda(w), systemProgram: SystemProgram.programId }).instruction();
    await send(`set_kyc_${name}`, [ix], [keys.admin]);
  }

  // 3. ABL allow list (gate CreateList mode Allow, AddWallet A and B)
  const admin = pk("admin");
  if (!(await chain.account(list))) {
    await send("abl_create_list", [raw(GATE, [meta(admin, false, true), meta(admin, true, true), meta(list, true), meta(SystemProgram.programId)],
      [1, 0, ...listSeed.toBytes()])], [keys.admin]);
  }
  for (const [name, w] of [["A", pk("investorA")], ["B", pk("investorB")]] as const) {
    if (await chain.account(walletEntry(w))) continue;
    await send(`abl_add_wallet_${name}`, [raw(GATE, [meta(admin, false, true), meta(admin, true, true), meta(list, true), meta(w), meta(walletEntry(w), true),
      meta(SystemProgram.programId)], [2])], [keys.admin]);
  }

  // 4. tranche mints, Token ACL MintConfig (freeze authority moves to it), permissionless thaw, gate thaw metas
  for (const t of [0, 1]) {
    const m = tranche[t];
    if (!(await chain.account(m))) {
      const len = getMintLen([ExtensionType.MetadataPointer, ExtensionType.TransferHook, ExtensionType.DefaultAccountState]);
      const ixs = [
        SystemProgram.createAccount({ fromPubkey: admin, newAccountPubkey: m, lamports: rent(len + 512), space: len, programId: T22 }),
        createInitializeMetadataPointerInstruction(m, admin, m, T22),
        createInitializeTransferHookInstruction(m, PublicKey.default, PROGRAM_ID, T22),
        createInitializeDefaultAccountStateInstruction(m, AccountState.Frozen, T22),
        createInitializeMint2Instruction(m, DEC, admin, admin, T22),
        createInitializeInstruction({ programId: T22, metadata: m, updateAuthority: admin, mint: m, mintAuthority: admin,
          name: t === 0 ? "SaveSail Senior" : "SaveSail Junior", symbol: t === 0 ? "SSS" : "SSJ", uri: "" }),
        createUpdateFieldInstruction({ programId: T22, metadata: m, updateAuthority: admin, field: "token_acl", value: GATE.toBase58() }),
        createSetAuthorityInstruction(m, admin, AuthorityType.MintTokens, deal, [], T22),
      ];
      await send(`create_tranche_mint_${t}`, ixs, [keys.admin, trancheKps[t]]);
    }
    if (!(await chain.account(mintConfig(m)))) {
      await send(`token_acl_config_${t}`, [
        raw(TACL, [meta(admin, true, true), meta(admin, false, true), meta(m, true), meta(mintConfig(m), true), meta(SystemProgram.programId), meta(T22)],
          [0, ...GATE.toBytes()]),
        raw(TACL, [meta(admin, false, true), meta(mintConfig(m), true)], [8, 0, 1]),
        raw(GATE, [meta(admin, false, true), meta(admin, true, true), meta(mintConfig(m)), meta(m), meta(gateMetas(m), true), meta(SystemProgram.programId), meta(list)], [4]),
      ], [keys.admin]);
    }
  }
  if (!(await decode("Deal", deal)).trancheMints[0].equals(tranche[0])) {
    const ix = await program.methods.registerTrancheMints().accountsStrict({ admin, deal, paymentMint: dusd, seniorMint: tranche[0], juniorMint: tranche[1],
      seniorConfig: mintConfig(tranche[0]), juniorConfig: mintConfig(tranche[1]), seniorMetas: hookMetas(tranche[0]), juniorMetas: hookMetas(tranche[1]),
      systemProgram: SystemProgram.programId }).instruction();
    await send("register_tranche_mints", [ix], [keys.admin]);
  }

  // 5. holders: associated account (frozen by default) -> self-thaw through Token ACL -> receive
  const thawIx = (signer: PublicKey, owner: PublicKey, t: number) => {
    const m = tranche[t], ta = tAta(owner, t);
    return raw(TACL, [meta(signer, true, true), meta(m), meta(ta, true), meta(flagPda(ta), true), meta(owner), meta(mintConfig(m)), meta(T22),
      meta(SystemProgram.programId), meta(GATE), meta(gateMetas(m)), meta(list), meta(walletEntry(owner))], [6]);
  };
  const investIx = (w: PublicKey, t: number, amount: bigint) => program.methods.investTokenized(t, new BN(amount.toString())).accountsStrict({
    investor: w, deal, kyc: kycPda(w), position: posPda(t, w), paymentMint: dusd, investorToken: dusdAta(w), raiseVault: vault(0),
    tokenProgram: TOKEN_PROGRAM_ID, trancheMint: tranche[t], investorTrancheToken: tAta(w, t), trancheTokenProgram: T22,
    systemProgram: SystemProgram.programId }).instruction();
  const openIx = (payer: PublicKey, w: PublicKey, t: number) => program.methods.openPosition(t, w).accountsStrict({ payer, deal, position: posPda(t, w),
    systemProgram: SystemProgram.programId }).instruction();
  // transfer_checked plus the hook's extra accounts in the order spl-transfer-hook-interface resolves them
  const xferIx = (from: PublicKey, to: PublicKey, t: number, amount: bigint) => {
    const i = createTransferCheckedInstruction(tAta(from, t), tranche[t], tAta(to, t), from, amount, DEC, [], T22);
    i.keys.push(meta(deal), meta(posPda(t, from), true), meta(posPda(t, to), true), meta(PROGRAM_ID), meta(hookMetas(tranche[t])));
    return i;
  };
  const tokenState = async (w: PublicKey, t: number) => {
    const d = await chain.account(tAta(w, t));
    return d ? unpackAccount(tAta(w, t), info(d, T22), T22) : null;
  };
  const A = keys.investorA, B = keys.investorB, U = outsider;
  for (const [name, w] of [["A", A], ["B", B], ["outsider", U]] as const) {
    if (!(await chain.account(tAta(w.publicKey, 0)))) {
      await send(`create_ata_${name}`, [createAssociatedTokenAccountIdempotentInstruction(w.publicKey, tAta(w.publicKey, 0), w.publicKey, tranche[0], T22)], [w]);
    }
  }
  const frozenAtStart = { A: (await tokenState(A.publicKey, 0))!.isFrozen, B: (await tokenState(B.publicKey, 0))!.isFrozen, outsider: (await tokenState(U.publicKey, 0))!.isFrozen };
  if ((await tokenState(A.publicKey, 0))!.isFrozen) await send("thaw_A_self", [thawIx(A.publicKey, A.publicKey, 0)], [A]);
  if (!(await chain.account(posPda(0, A.publicKey)))) await send("invest_tokenized_A", [await investIx(A.publicKey, 0, 10_000n * M)], [A]);
  if ((await tokenState(B.publicKey, 0))!.isFrozen) await send("thaw_B_self", [thawIx(B.publicKey, B.publicKey, 0)], [B]);
  if (!(await chain.account(posPda(0, B.publicKey)))) {
    await send("open_position_B", [await openIx(B.publicKey, B.publicKey, 0)], [B]);
    await send("transfer_A_to_B", [xferIx(A.publicKey, B.publicKey, 0, 2_500n * M)], [A]);
  }
  // the outsider: thaw by itself and by a listed wallet, invest, receive by transfer: all fail
  if (!(await chain.account(posPda(0, U.publicKey)))) {
    await reject("thaw_outsider_self", [thawIx(U.publicKey, U.publicKey, 0)], [U], `Program ${GATE.toBase58()} failed`);
    await reject("thaw_outsider_by_A", [thawIx(A.publicKey, U.publicKey, 0)], [A], `Program ${GATE.toBase58()} failed`);
    await reject("invest_tokenized_outsider", [await investIx(U.publicKey, 0, 1_000n * M)], [U], "Account is frozen");
    await send("open_position_outsider", [await openIx(U.publicKey, U.publicKey, 0)], [U]);
    await reject("transfer_A_to_outsider", [xferIx(A.publicKey, U.publicKey, 0, 1n * M)], [A], "Account is frozen");
  }

  // 6. read back everything the criteria name
  const mints = [];
  for (const t of [0, 1]) {
    const m = tranche[t];
    const mi = unpackMint(m, info((await chain.account(m))!, T22), T22);
    const md = unpackMetadata(getExtensionData(ExtensionType.TokenMetadata, mi.tlvData)!);
    const cfg = (await chain.account(mintConfig(m)))!;
    mints.push({
      tranche: t === 0 ? "senior" : "junior", address: m.toBase58(), extensions: getExtensionTypes(mi.tlvData).map((e) => ExtensionType[e]),
      default_account_state: AccountState[getDefaultAccountState(mi)!.state], transfer_hook_program: getTransferHook(mi)!.programId.toBase58(),
      transfer_hook_authority: getTransferHook(mi)!.authority.toBase58(), metadata_address: getMetadataPointerState(mi)!.metadataAddress?.toBase58(),
      name: md.name, token_acl: Object.fromEntries(md.additionalMetadata)["token_acl"] ?? null,
      mint_authority: mi.mintAuthority?.toBase58(), freeze_authority: mi.freezeAuthority?.toBase58(), supply: mi.supply.toString(),
      mint_config: { address: mintConfig(m).toBase58(), gating_program: new PublicKey(cfg.subarray(68, 100)).toBase58(), permissionless_thaw: cfg[2] === 1,
        permissionless_freeze: cfg[3] === 1, authority: new PublicKey(cfg.subarray(36, 68)).toBase58() },
      gate_thaw_metas: gateMetas(m).toBase58(), hook_metas: hookMetas(m).toBase58(),
    });
  }
  const l = (await chain.account(list))!;
  record.mints = mints;
  record.allow_list = { address: list.toBase58(), mode: l[65] === 0 ? "allow" : l[65] === 2 ? "block" : "unused", wallets: Number(l.readBigUInt64LE(66)),
    entries: { A: walletEntry(A.publicKey).toBase58(), B: walletEntry(B.publicKey).toBase58() } };
  const d = await decode("Deal", deal);
  record.holders = [];
  for (const [name, w, listed] of [["investorA", A.publicKey, true], ["investorB", B.publicKey, true], ["outsider", U.publicKey, false]] as const) {
    const s = await tokenState(w, 0);
    const p = await decode("Position", posPda(0, w));
    record.holders.push({ name, address: w.toBase58(), listed, account: tAta(w, 0).toBase58(), position: posPda(0, w).toBase58(), frozen_at_creation: (frozenAtStart as any)[name === "investorA" ? "A" : name === "investorB" ? "B" : "outsider"],
      state: s?.isFrozen ? "frozen" : "initialized", balance: s?.amount.toString() ?? "0", units: p?.units.toString() ?? "0" });
  }
  const senior = mints[0];
  const checks = {
    one_mint_per_tranche: d.trancheMints[0].equals(tranche[0]) && d.trancheMints[1].equals(tranche[1]) && !tranche[0].equals(tranche[1]),
    default_frozen: mints.every((x) => x.default_account_state === "Frozen"),
    token_acl_metadata: mints.every((x) => x.token_acl === GATE.toBase58()),
    hook_is_savesail_immutable: mints.every((x) => x.transfer_hook_program === PROGRAM_ID.toBase58() && x.transfer_hook_authority === PublicKey.default.toBase58()),
    mint_config_gate: mints.every((x) => x.mint_config.gating_program === GATE.toBase58() && x.mint_config.permissionless_thaw && x.freeze_authority === x.mint_config.address),
    allow_list: record.allow_list.mode === "allow" && record.allow_list.wallets === 2,
    created_frozen: Object.values(frozenAtStart).every(Boolean),
    listed_thaw_and_receive: record.holders.slice(0, 2).every((h: any) => h.state === "initialized" && BigInt(h.balance) > 0n),
    unlisted_thaw_and_receive_fail: record.holders[2].state === "frozen" && record.holders[2].balance === "0"
      && ["thaw_outsider_self", "thaw_outsider_by_A", "invest_tokenized_outsider", "transfer_A_to_outsider"].every((s) => record.rejected.some((r: any) => r.label === s)),
    units_equal_balance: record.holders.every((h: any) => h.units === h.balance),
    supply_equals_raised: senior.supply === d.raised[0].toString() && mints[1].supply === d.raised[1].toString(),
  };
  record.checks = checks;
  record.duration_s = Math.round((Date.now() - t0) / 1000);
  record.finished = new Date().toISOString();
  const out = join(ROOT, mode === "local" ? "target/p1-local.json" : "app/public/p1-run.json");
  if (chain instanceof LocalChain) chain.save(join(ROOT, "target/p1-local-svm.bin")); // for check_p1_chain.ts
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(record, null, 1));
  const bad = Object.entries(checks).filter(([, v]) => !v).map(([k]) => k);
  console.log(`P1 ${mode}: ${Object.keys(checks).length - bad.length}/${Object.keys(checks).length} checks, ${record.steps.length} transactions, `
    + `${record.rejected.length} expected rejections, ${record.duration_s}s -> ${out}`);
  if (bad.length) {
    console.error(`FAIL ${bad.join(", ")}`);
    process.exit(1);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
