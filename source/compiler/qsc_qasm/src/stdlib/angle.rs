// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
pub(crate) mod tests;

use num_bigint::BigInt;

use crate::convert::safe_u64_to_f64;
use core::f64;
use std::convert::TryInto;
use std::f64::consts::TAU;
use std::fmt;
use std::ops::{
    Add, AddAssign, BitAnd, BitOr, BitXor, Div, DivAssign, Mul, MulAssign, Neg, Not, Shl, Shr, Sub,
    SubAssign,
};

/// A fixed-point angle type with a specified number of bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Angle {
    pub value: u64,
    pub size: u32,
}

impl Angle {
    #[must_use]
    pub fn new(value: u64, size: u32) -> Self {
        Angle { value, size }
    }

    #[must_use]
    pub fn from_u64_maybe_sized(value: u64, size: Option<u32>) -> Angle {
        Angle {
            value,
            size: size.unwrap_or(f64::MANTISSA_DIGITS),
        }
    }

    #[must_use]
    pub fn from_f64_maybe_sized(val: f64, size: Option<u32>) -> Angle {
        Self::from_f64_sized(val, size.unwrap_or(f64::MANTISSA_DIGITS))
    }

    /// Takes an `f64` representing angle and:
    ///  1. Wraps it around so that it is in the range [0, TAU).
    ///  2. Encodes it as a binary number between 0 and (1 << size) - 1.
    #[must_use]
    pub fn from_f64_sized(mut val: f64, size: u32) -> Angle {
        // First, we need to convert the angle to the `[0, TAU)` range.
        val %= TAU;

        // The modulus operator leaves negative numbers as negative.
        // So, in this case we need to add an extra `TAU`.
        if val < 0. {
            val += TAU;
        }

        // Handle the edge case where modulo returns tau due to floating-point precision
        // we've seen this when the user rotates by f64::EPSILON / 2.0 causing the value
        // to be still equal to tau after the modulo operation.
        if val >= TAU {
            val = 0.;
        }

        assert!(val >= 0., "Value must be >= 0.");
        assert!(val < TAU, "Value must be < tau.");
        assert!(size > 0, "Size must be > 0");

        // If the size is > f64::MANTISSA_DIGITS, the cast to f64
        // on the next lines will loose precision.
        if size > f64::MANTISSA_DIGITS {
            return Self::from_f64_sized_edge_case(val, size);
        }

        #[allow(clippy::cast_precision_loss)]
        let factor = TAU / (1u64 << size) as f64;
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let value = (val / factor).round() as u64;
        Angle::new(value, size)
    }

    /// This function handles the edge case when size > `f64::MANTISSA_DIGITS`.
    fn from_f64_sized_edge_case(val: f64, size: u32) -> Angle {
        let angle = Self::from_f64_sized(val, f64::MANTISSA_DIGITS);
        angle.cast(size, false)
    }

    #[cfg(test)]
    fn to_bitstring(self) -> String {
        format!("{:0width$b}", self.value, width = self.size as usize)
    }

    #[must_use]
    pub fn cast_to_maybe_sized(self, new_size: Option<u32>) -> Angle {
        match new_size {
            Some(size) => self.cast(size, false),
            None => self,
        }
    }

    fn cast(&self, new_size: u32, truncate: bool) -> Self {
        match new_size.cmp(&self.size) {
            std::cmp::Ordering::Less => {
                let value = if truncate {
                    let shift_amount = self.size - new_size;
                    self.value >> shift_amount
                } else {
                    // Rounding
                    let shift_amount = self.size - new_size;
                    let half = 1u64 << (shift_amount - 1);
                    let mask = (1u64 << shift_amount) - 1;
                    let lower_bits = self.value & mask;
                    let upper_bits = self.value >> shift_amount;

                    if lower_bits > half || (lower_bits == half && (upper_bits & 1) == 1) {
                        upper_bits + 1
                    } else {
                        upper_bits
                    }
                };
                Angle {
                    value,
                    size: new_size,
                }
            }
            std::cmp::Ordering::Equal => {
                // Same size, no change
                *self
            }
            std::cmp::Ordering::Greater => {
                // Padding with zeros
                let new_value = self.value << (new_size - self.size);
                Angle {
                    value: new_value,
                    size: new_size,
                }
            }
        }
    }
}

impl Default for Angle {
    fn default() -> Self {
        Self {
            value: 0,
            size: f64::MANTISSA_DIGITS,
        }
    }
}

// Bit shift
impl Shl<i64> for Angle {
    type Output = Self;

    fn shl(self, rhs: i64) -> Self::Output {
        let mask = (1 << self.size) - 1;
        Self {
            value: (self.value << rhs) & mask,
            size: self.size,
        }
    }
}

impl Shr<i64> for Angle {
    type Output = Self;

    fn shr(self, rhs: i64) -> Self::Output {
        Self {
            value: self.value >> rhs,
            size: self.size,
        }
    }
}

// Bitwise

impl Not for Angle {
    type Output = Self;

    fn not(self) -> Self::Output {
        let mask = (1 << self.size) - 1;
        Self {
            value: !self.value & mask,
            size: self.size,
        }
    }
}

impl BitAnd for Angle {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        assert_eq!(self.size, rhs.size);
        Self {
            value: self.value & rhs.value,
            size: self.size,
        }
    }
}

impl BitOr for Angle {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        assert_eq!(self.size, rhs.size);
        Self {
            value: self.value | rhs.value,
            size: self.size,
        }
    }
}

impl BitXor for Angle {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        assert_eq!(self.size, rhs.size);
        Self {
            value: self.value ^ rhs.value,
            size: self.size,
        }
    }
}

// Arithmetic

impl Add for Angle {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        assert_eq!(self.size, other.size, "Sizes must be the same");
        Angle {
            value: (self.value + other.value) % (1u64 << self.size),
            size: self.size,
        }
    }
}

impl Sub for Angle {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        assert_eq!(self.size, other.size, "Sizes must be the same");
        Angle {
            value: (self.value + (1u64 << self.size) - other.value) % (1u64 << self.size),
            size: self.size,
        }
    }
}

impl Mul<u64> for Angle {
    type Output = Self;

    fn mul(self, factor: u64) -> Self {
        Angle {
            value: (self.value * factor) % (1u64 << self.size),
            size: self.size,
        }
    }
}

impl Mul<u128> for Angle {
    type Output = Self;

    fn mul(self, factor: u128) -> Self {
        let r = BigInt::from(self.value) * BigInt::from(factor);
        let r = r % BigInt::from(1u128 << self.size);
        Angle {
            value: r.try_into().expect("Value is too large"),
            size: self.size,
        }
    }
}

impl Div<u64> for Angle {
    type Output = Self;

    fn div(self, divisor: u64) -> Self {
        Angle {
            value: self.value / divisor,
            size: self.size,
        }
    }
}

impl Div for Angle {
    type Output = u64;

    fn div(self, other: Self) -> u64 {
        assert_eq!(self.size, other.size, "Sizes must be the same");
        self.value / other.value
    }
}

impl Neg for Angle {
    type Output = Self;

    fn neg(self) -> Self {
        Angle {
            value: ((1u64 << self.size) - self.value) % (1u64 << self.size),
            size: self.size,
        }
    }
}

impl AddAssign for Angle {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl SubAssign for Angle {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl MulAssign<u64> for Angle {
    fn mul_assign(&mut self, factor: u64) {
        *self = *self * factor;
    }
}

impl DivAssign<u64> for Angle {
    fn div_assign(&mut self, divisor: u64) {
        *self = *self / divisor;
    }
}

impl TryInto<f64> for Angle {
    type Error = &'static str;

    /// Angle to float cast is not allowed in QASM3.
    /// This function is only meant to be used in unit tests.
    fn try_into(self) -> Result<f64, Self::Error> {
        if self.size > 64 {
            return Err("Size exceeds 64 bits");
        }

        // Edge case handling.
        if self.size > f64::MANTISSA_DIGITS {
            let angle = self.cast(f64::MANTISSA_DIGITS, false);
            return angle.try_into();
        }

        let Some(denom) = safe_u64_to_f64(1u64 << self.size) else {
            return Err("Denominator is too large");
        };
        let Some(value) = safe_u64_to_f64(self.value) else {
            return Err("Value is too large");
        };
        let factor = TAU / denom;
        Ok(value * factor)
    }
}

impl From<Angle> for bool {
    fn from(val: Angle) -> Self {
        val.value != 0
    }
}

impl fmt::Display for Angle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", (*self).try_into().unwrap_or(f64::NAN))
    }
}
