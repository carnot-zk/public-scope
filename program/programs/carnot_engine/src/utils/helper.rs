use anchor_lang::prelude::*;

use crate::errors::CarnotError;

pub const BPS_DENOMINATOR: u128 = 10_000;

pub trait SafeMath: Sized {
    fn safe_add(self, rhs: Self) -> Result<Self>;
    fn safe_sub(self, rhs: Self) -> Result<Self>;
    fn safe_mul(self, rhs: Self) -> Result<Self>;
    fn safe_div(self, rhs: Self) -> Result<Self>;
}

macro_rules! impl_safe_math {
    ($($t:ty),* $(,)?) => {
        $(
            impl SafeMath for $t {
                #[track_caller]
                fn safe_add(self, rhs: Self) -> Result<Self> {
                    self.checked_add(rhs).ok_or(error!(CarnotError::ArithmeticOverflow))
                }

                #[track_caller]
                fn safe_sub(self, rhs: Self) -> Result<Self> {
                    self.checked_sub(rhs).ok_or(error!(CarnotError::ArithmeticOverflow))
                }

                #[track_caller]
                fn safe_mul(self, rhs: Self) -> Result<Self> {
                    self.checked_mul(rhs).ok_or(error!(CarnotError::ArithmeticOverflow))
                }

                #[track_caller]
                fn safe_div(self, rhs: Self) -> Result<Self> {
                    self.checked_div(rhs).ok_or(error!(CarnotError::ArithmeticOverflow))
                }
            }
        )*
    };
}

impl_safe_math!(u128, u64, u32, i64);

#[track_caller]
pub fn safe_to_u64(value: u128) -> Result<u64> {
    value
        .try_into()
        .map_err(|_| error!(CarnotError::ArithmeticOverflow))
}

#[track_caller]
pub fn safe_to_u32(value: u128) -> Result<u32> {
    value
        .try_into()
        .map_err(|_| error!(CarnotError::ArithmeticOverflow))
}

#[track_caller]
pub fn safe_abs_i64_to_u64(value: i64) -> Result<u64> {
    value
        .checked_abs()
        .ok_or(error!(CarnotError::ArithmeticOverflow))?
        .try_into()
        .map_err(|_| error!(CarnotError::ArithmeticOverflow))
}

#[track_caller]
pub fn safe_mul_div_u64(value: u64, numerator: u64, denominator: u64) -> Result<u64> {
    let quotient = u128::from(value)
        .safe_mul(u128::from(numerator))?
        .safe_div(u128::from(denominator))?;
    safe_to_u64(quotient)
}

#[track_caller]
pub fn safe_ratio_bps_u128(numerator: u64, denominator: u64) -> Result<u128> {
    u128::from(numerator)
        .safe_mul(BPS_DENOMINATOR)?
        .safe_div(u128::from(denominator))
}

#[track_caller]
pub fn safe_elapsed_secs(now: i64, since: i64) -> Result<i64> {
    now.safe_sub(since)
}

/// Shared guard for any instruction that requires a strictly positive token amount.
#[track_caller]
pub fn require_nonzero(value: u64) -> Result<()> {
    require!(value > 0, CarnotError::InvalidAmount);
    Ok(())
}
