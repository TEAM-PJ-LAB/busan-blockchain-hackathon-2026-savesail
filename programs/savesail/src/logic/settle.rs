//! Settlement with cumulative true-up (PRD §6.5). Mirrors offchain/reference/savesail_ref.py.

pub const BPS: u128 = 10_000;
pub const RATE_MAX_BPS: i32 = 5_000;

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
    pub violation: bool,
}

/// kg * micro-USD/t / 1000 = micro-USD (= dUSD base units).
pub fn value_micro(kg: u64, price_micro_per_t: u64) -> Result<u128, MathError> {
    (kg as u128)
        .checked_mul(price_micro_per_t as u128)
        .map(|x| x / 1000)
        .ok_or(MathError)
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

pub fn is_violation(
    compliance_bps: u16,
    comp_min_bps: u16,
    payer_miss_groups: u32,
    groups: u32,
    miss_max_bps: u16,
) -> bool {
    compliance_bps < comp_min_bps
        || (payer_miss_groups as u64) * (BPS as u64) > (miss_max_bps as u64) * (groups as u64)
}

/// Applies one finalized period and returns `due`. `kg_miss_inv` is settled at 0 and omitted.
pub fn finalize(
    a: &mut Accrual,
    p: &PeriodInput,
    phi_bps: u16,
    ref_rate_bps: i32,
) -> Result<u64, MathError> {
    let w = value_micro(p.kg_on, p.price_micro_per_t)?;
    let mut next = *a;
    if p.violation {
        let fixed = g_num(w, next.last_lb_bps.unwrap_or(ref_rate_bps))?;
        next.sv_fixed = next.sv_fixed.checked_add(fixed).ok_or(MathError)?;
    } else {
        next.w_norm = next.w_norm.checked_add(w).ok_or(MathError)?;
        next.last_lb_bps = Some(p.lb_bps);
    }
    let miss = g_num(value_micro(p.kg_miss_payer, p.price_micro_per_t)?, p.ub_bps)?;
    next.sv_fixed = next.sv_fixed.checked_add(miss).ok_or(MathError)?;
    let lb = next.last_lb_bps.unwrap_or(ref_rate_bps);
    let e = g_num(next.w_norm, lb)?
        .checked_add(next.sv_fixed)
        .and_then(|x| x.checked_mul(phi_bps as u128))
        .map(|x| x / BPS)
        .ok_or(MathError)?;
    next.entitlement = u64::try_from(e).map_err(|_| MathError)?;
    *a = next;
    Ok(a.entitlement.saturating_sub(a.paid_cum))
}
