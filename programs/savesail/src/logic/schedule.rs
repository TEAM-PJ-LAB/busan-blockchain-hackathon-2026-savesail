//! Schedule rules (PRD §6.4). Mirrors offchain/reference/savesail_ref.py.

pub const BLOCKS_PER_GROUP: u64 = 5;
pub const NO_OFF: u8 = 7;
pub const MAX_SKIP: i32 = 8;
pub const RESOLVED: u8 = 0x80;
pub const OFF_DOMAIN: &[u8] = b"SAVESAIL_OFF_V1";

#[inline]
pub fn h(parts: &[&[u8]]) -> [u8; 32] {
    solana_sha256_hasher::hashv(parts).to_bytes()
}

/// `H^k(value) == last` with `1 <= k <= MAX_SKIP`.
pub fn verify_reveal(last: &[u8; 32], value: &[u8; 32], k: i32) -> bool {
    if !(1..=MAX_SKIP).contains(&k) {
        return false;
    }
    let mut x = *value;
    for _ in 0..k {
        x = h(&[&x]);
    }
    x == *last
}

/// Roles in fixed order: 0 payer, 1 investor_rep, 2 verifier.
pub fn off_index(campaign: &[u8; 32], group: u32, v: [&[u8; 32]; 3]) -> u8 {
    let d = h(&[OFF_DOMAIN, campaign, &group.to_le_bytes(), v[0], v[1], v[2]]);
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    (u64::from_le_bytes(b) % BLOCKS_PER_GROUP) as u8
}

pub fn data_acc_next(acc: &[u8; 32], group: u32, root: &[u8; 32]) -> [u8; 32] {
    h(&[acc, &group.to_le_bytes(), root])
}

/// status byte: bits 0-2 off_index, bits 3-5 missing roles, bit 7 resolved.
#[inline]
pub const fn pack_status(off: u8, missing_mask: u8) -> u8 {
    RESOLVED | (off & 0x07) | ((missing_mask & 0x07) << 3)
}
#[inline]
pub const fn is_resolved(s: u8) -> bool {
    s & RESOLVED != 0
}
#[inline]
pub const fn off_of(s: u8) -> u8 {
    s & 0x07
}
#[inline]
pub const fn missing_of(s: u8) -> u8 {
    (s >> 3) & 0x07
}

/// Merkle root used by off-chain committers (PRD §7.3). Host-side helper for tests.
#[cfg(not(target_os = "solana"))]
pub fn merkle_root(lines: &[&[u8]]) -> [u8; 32] {
    let mut lvl: Vec<[u8; 32]> = if lines.is_empty() {
        vec![h(&[&[0u8]])]
    } else {
        lines.iter().map(|l| h(&[&[0u8], l])).collect()
    };
    while lvl.len() > 1 {
        if lvl.len() % 2 == 1 {
            lvl.push(*lvl.last().unwrap());
        }
        lvl = lvl.chunks(2).map(|p| h(&[&[1u8], &p[0], &p[1]])).collect();
    }
    lvl[0]
}
