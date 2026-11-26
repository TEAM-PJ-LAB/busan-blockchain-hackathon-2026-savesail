//! Pure rule modules shared by instructions and host tests. No account or CPI code here.

pub mod schedule;
pub mod settle;
pub mod waterfall;

#[cfg(test)]
mod tests;
