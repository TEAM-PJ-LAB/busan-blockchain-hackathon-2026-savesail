// Minimal Borsh readers for the program accounts (programs/savesail/src/state.rs), no SDK in the bundle.
// scripts/run/check_web_chain.ts checks these offsets against the executor's chain state.

class R {
  o = 8; // Anchor discriminator
  constructor(public b: Uint8Array) {}
  skip(n: number) { this.o += n; return this; }
  u8() { return this.b[this.o++]; }
  u16() { const v = this.b[this.o] | (this.b[this.o + 1] << 8); this.o += 2; return v; }
  i32() { const v = new DataView(this.b.buffer, this.b.byteOffset + this.o, 4).getInt32(0, true); this.o += 4; return v; }
  u64() { const v = new DataView(this.b.buffer, this.b.byteOffset + this.o, 8).getBigUint64(0, true); this.o += 8; return v; }
  u128() { const lo = this.u64(); const hi = this.u64(); return (hi << 64n) | lo; }
}

export function decodeDeal(b: Uint8Array) {
  const r = new R(b);
  r.skip(32 + 8 + 32 + 32 + 32 + 96 + 32 + 32 + 32); // admin .. envelope_hash
  const phi = r.u16(); const ref = r.i32(); const compMin = r.u16(); const missMax = r.u16();
  const installCost = r.u64(); const caps = [r.u64(), r.u64()]; const raised = [r.u64(), r.u64()];
  const opexPerPeriod = r.u64(); const reserveTarget = r.u64(); const nPeriods = r.u16();
  const seniorFirst = r.u64(); r.skip(39 * 8); r.u8(); r.u8();
  r.u128(); r.u128();
  const lastLb = r.u8() ? r.i32() : null;
  const entitlement = r.u64(); const paidCum = r.u64(); const nextPeriod = r.u16();
  const opexArrears = r.u64(); const seniorArrears = r.u64(); const reserve = r.u64();
  const acc = [r.u128(), r.u128()];
  return { phi, ref, compMin, missMax, installCost, caps, raised, opexPerPeriod, reserveTarget, nPeriods, seniorFirst, lastLb, entitlement, paidCum, nextPeriod, opexArrears, seniorArrears, reserve, acc };
}

export function decodePeriod(b: Uint8Array) {
  const r = new R(b);
  r.skip(32); r.u16(); r.skip(96); r.u8();
  r.u16(); r.skip(64); const kgOn = r.u64(); r.u64(); r.u64(); r.u16(); r.u64();
  const lb = r.i32(); const est = r.i32(); const ub = r.i32(); const compliance = r.u16(); const flags = r.u8(); r.skip(32); r.skip(32);
  r.skip(32);
  const finalized = r.u8(); const outcome = r.u8(); const due = r.u64(); const settled = r.u8(); const distributed = r.u8();
  const dist = [r.u64(), r.u64(), r.u64(), r.u64(), r.u64()];
  return { kgOn, lb, est, ub, compliance, flags, finalized, outcome, due, settled, distributed, dist };
}

// Campaign.status[512] starts after deal..pending_val (8 + 300 bytes); one byte per group: bits 0-2 OFF block, 3-5 missing roles.
export const CAMPAIGN_STATUS_OFFSET = 8 + 32 + 2 + 2 + 8 + 8 + 8 + 1 + 96 + 12 + 24 + 2 + 8 + 1 + 96;
export function decodeCampaignStatus(b: Uint8Array, n: number) {
  return Array.from(b.subarray(CAMPAIGN_STATUS_OFFSET, CAMPAIGN_STATUS_OFFSET + n));
}

// SPL token account: mint 32, owner 32, amount u64 LE.
export function decodeTokenAmount(b: Uint8Array) {
  return new DataView(b.buffer, b.byteOffset + 64, 8).getBigUint64(0, true);
}

// Position: deal 32, owner 32, tranche u8, units u64, acc_snapshot u128, claimed u64.
export function decodePosition(b: Uint8Array) {
  const r = new R(b);
  r.skip(64); const tranche = r.u8(); const units = r.u64(); r.u128(); const claimed = r.u64();
  return { tranche, units, claimed };
}

// Token-2022 (P1): base mint 82 bytes / account 165 bytes, account-type byte at 165, then TLV (type u16, length u16).
export function t22Extension(b: Uint8Array, type: number): Uint8Array | null {
  for (let o = 166; o + 4 <= b.length; ) {
    const t = b[o] | (b[o + 1] << 8), l = b[o + 2] | (b[o + 3] << 8);
    if (t === type) return b.subarray(o + 4, o + 4 + l);
    if (t === 0) return null;
    o += 4 + l;
  }
  return null;
}
export const T22_DEFAULT_ACCOUNT_STATE = 6, T22_TOKEN_METADATA = 19;
export const decodeMintSupply = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset + 36, 8).getBigUint64(0, true);
/** token account state byte: 1 initialized, 2 frozen */
export const decodeTokenState = (b: Uint8Array) => b[108];
/** TokenMetadata additional field `key` (borsh: authority 32, mint 32, name, symbol, uri, vec<(key, value)>) */
export function decodeMetadataField(v: Uint8Array, key: string): string | null {
  const dv = new DataView(v.buffer, v.byteOffset, v.length), td = new TextDecoder();
  let o = 64;
  const str = () => { const n = dv.getUint32(o, true); o += 4; const s = td.decode(v.subarray(o, o + n)); o += n; return s; };
  str(); str(); str();
  const n = dv.getUint32(o, true); o += 4;
  for (let i = 0; i < n; i++) { const k = str(), val = str(); if (k === key) return val; }
  return null;
}
const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
export function base58(bytes: Uint8Array): string {
  let x = 0n;
  for (const c of bytes) x = (x << 8n) | BigInt(c);
  let s = "";
  while (x > 0n) { s = B58[Number(x % 58n)] + s; x /= 58n; }
  for (const c of bytes) { if (c !== 0) break; s = "1" + s; }
  return s;
}
