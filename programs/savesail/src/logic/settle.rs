//! Settlement with cumulative true-up (PRD §6.5). Mirrors offchain/reference/savesail_ref.py.

pub const BPS: u128 = 10_000;
pub const RATE_MAX_BPS: i32 = 5_000;

/// `flags` bits (PRD §6.2).
pub const F_COVARIATE: u8 = 1 << 0;
pub const F_METER: u8 = 1 << 1;
pub const F_FEW_GROUPS: u8 = 1 << 2;
pub const F_DATA_QUALITY: u8 = 1 << 3;
pub const F_PAYER_MASK: u8 = F_COVARIATE | F_METER | F_DATA_QUALITY;
pub const F_ALLOWED: u8 = 0x0F;

/// Period outcome (PRD §6.5 branches, `PeriodState.outcome`).
pub const OUT_NORMAL: u8 = 0;
pub const OUT_PAYER_VIOLATION: u8 = 1;
pub const OUT_INVESTOR_OVER_CAP: u8 = 2;
pub const OUT_NO_BOUND: u8 = 3;

#[derive(Debug, PartialEq, Eq)]
pub struct MathError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accrual {
    pub w_norm: u128,
    pub sv_fixed: u128,
    pub last_lb_bps: Option<i32>,
    pub entitlement: u64,
    pub paid_cum: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct PeriodInput {
    pub kg_on: u64,
    pub kg_miss_payer: u64,
    pub price_micro_per_t: u64,
    pub lb_bps: i32,
    pub ub_bps: i32,
    pub outcome: u8,
}

/// Group counts of one period (PRD §6.5). `override_groups` = safety overrides below the weather threshold.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub groups: u32,
    pub payer_miss: u32,
    pub inv_miss: u32,
    pub ver_miss: u32,
    pub override_groups: u32,
}

/// kg * micro-USD/t / 1000 = micro-USD (= dUSD base units).
pub fn value_micro(kg: u64, price_micro_per_t: u64) -> Result<u128, MathError> {
    (kg as u128).checked_mul(price_micro_per_t as u128).map(|x| x / 1000).ok_or(MathError)
}

/// ON-fuel value -> saved value: w * r / (1 - r), r clamped to >= 0.
pub fn g_num(w: u128, rate_bps: i32) -> Result<u128, MathError> {
    let r = rate_bps.max(0) as u128;
    if r >= BPS {
        return Err(MathError);
    }
    w.checked_mul(r).map(|x| x / (BPS - r)).ok_or(MathError)
}

pub fn rates_valid(lb: i32, est: i32, ub: i32) -> bool {
    -(BPS as i32) < lb && lb <= est && est <= ub && ub < RATE_MAX_BPS
}

#[inline]
fn over_cap(n: u32, groups: u32, miss_max_bps: u16) -> bool {
    (n as u64) * (BPS as u64) > (miss_max_bps as u64) * (groups as u64)
}

/// Precedence: payer violation > investor over-cap > no bound (C15 or verifier over-cap, C18) > normal.
pub fn classify(compliance_bps: u16, comp_min_bps: u16, flags: u8, c: &Counts, miss_max_bps: u16) -> u8 {
    let payer_viol = compliance_bps < comp_min_bps
        || over_cap(c.payer_miss + c.override_groups, c.groups, miss_max_bps)
        || flags & F_PAYER_MASK != 0;
    if payer_viol {
        OUT_PAYER_VIOLATION
    } else if over_cap(c.inv_miss, c.groups, miss_max_bps) {
        OUT_INVESTOR_OVER_CAP
    } else if flags & F_FEW_GROUPS != 0 || over_cap(c.ver_miss, c.groups, miss_max_bps) {
        OUT_NO_BOUND
    } else {
        OUT_NORMAL
    }
}

/// Applies one finalized period and returns `due`. `kg_miss_inv` is settled at 0 and omitted.
pub fn finalize(a: &mut Accrual, p: &PeriodInput, phi_bps: u16, ref_rate_bps: i32) -> Result<u64, MathError> {
    let w = value_micro(p.kg_on, p.price_micro_per_t)?;
    let mut next = *a;
    match p.outcome {
        OUT_PAYER_VIOLATION => {
            let fixed = g_num(w, p.ub_bps.max(ref_rate_bps))?;
            next.sv_fixed = next.sv_fixed.checked_add(fixed).ok_or(MathError)?;
        }
        OUT_INVESTOR_OVER_CAP => {}
        OUT_NO_BOUND => next.w_norm = next.w_norm.checked_add(w).ok_or(MathError)?,
        OUT_NORMAL => {
            next.w_norm = next.w_norm.checked_add(w).ok_or(MathError)?;
            next.last_lb_bps = Some(p.lb_bps);
        }
        _ => return Err(MathError),
    }
    let miss = g_num(value_micro(p.kg_miss_payer, p.price_micro_per_t)?, p.ub_bps)?;
    next.sv_fixed = next.sv_fixed.checked_add(miss).ok_or(MathError)?;
    let e = g_num(next.w_norm, next.last_lb_bps.unwrap_or(0))?
        .checked_add(next.sv_fixed)
        .and_then(|x| x.checked_mul(phi_bps as u128))
        .map(|x| x / BPS)
        .ok_or(MathError)?;
    next.entitlement = u64::try_from(e).map_err(|_| MathError)?;
    *a = next;
    Ok(a.entitlement.saturating_sub(a.paid_cum))
}
