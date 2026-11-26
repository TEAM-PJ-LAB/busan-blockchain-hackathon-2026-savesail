//! Waterfall and close-out (PRD §6.5). Mirrors offchain/reference/savesail_ref.py.

use super::settle::MathError;

pub const ACC: u128 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub opex_arrears: u64,
    pub senior_arrears: u64,
    pub reserve: u64,
    pub acc: [u128; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dist {
    pub opex: u64,
    pub senior: u64,
    pub reserve_draw: u64,
    pub reserve_in: u64,
    pub junior: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Close {
    pub refund: u64,
    pub opex_arrears_paid: u64,
    pub senior_arrears_paid: u64,
    pub reserve_to_junior: u64,
}

fn acc_add(acc: u128, amount: u64, units: u64) -> Result<u128, MathError> {
    if units == 0 {
        return Err(MathError);
    }
    acc.checked_add((amount as u128) * ACC / units as u128).ok_or(MathError)
}

/// `available` = collection vault balance. Updates `b` and returns the split.
pub fn distribute(
    b: &mut Book,
    available: u64,
    opex_per_period: u64,
    senior_scheduled: u64,
    reserve_target: u64,
    units: [u64; 2],
) -> Result<Dist, MathError> {
    let mut nb = *b;
    let mut av = available;
    let o_due = opex_per_period.checked_add(nb.opex_arrears).ok_or(MathError)?;
    let opex = av.min(o_due);
    av -= opex;
    nb.opex_arrears = o_due - opex;

    let s_due = senior_scheduled.checked_add(nb.senior_arrears).ok_or(MathError)?;
    let from_av = av.min(s_due);
    av -= from_av;
    let reserve_draw = nb.reserve.min(s_due - from_av);
    nb.reserve -= reserve_draw;
    let senior = from_av + reserve_draw;
    nb.senior_arrears = s_due - senior;

    let reserve_in = av.min(reserve_target.saturating_sub(nb.reserve));
    av -= reserve_in;
    nb.reserve += reserve_in;
    let junior = av;

    nb.acc[0] = acc_add(nb.acc[0], senior, units[0])?;
    nb.acc[1] = acc_add(nb.acc[1], junior, units[1])?;
    *b = nb;
    Ok(Dist { opex, senior, reserve_draw, reserve_in, junior })
}

pub fn claimable(units: u64, acc: u128, snapshot: u128) -> Result<u64, MathError> {
    let d = acc.checked_sub(snapshot).ok_or(MathError)?;
    u64::try_from((units as u128).checked_mul(d).ok_or(MathError)? / ACC).map_err(|_| MathError)
}

/// Order: payer refund of overpayment -> opex arrears -> senior arrears -> junior.
pub fn close(b: &mut Book, paid_cum: u64, entitlement: u64, units: [u64; 2]) -> Result<Close, MathError> {
    let mut nb = *b;
    let refund = paid_cum.saturating_sub(entitlement).min(nb.reserve);
    nb.reserve -= refund;
    let o = nb.opex_arrears.min(nb.reserve);
    nb.reserve -= o;
    nb.opex_arrears -= o;
    let s = nb.senior_arrears.min(nb.reserve);
    nb.reserve -= s;
    nb.senior_arrears -= s;
    nb.acc[0] = acc_add(nb.acc[0], s, units[0])?;
    let j = nb.reserve;
    nb.reserve = 0;
    nb.acc[1] = acc_add(nb.acc[1], j, units[1])?;
    *b = nb;
    Ok(Close { refund, opex_arrears_paid: o, senior_arrears_paid: s, reserve_to_junior: j })
}
