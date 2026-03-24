// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::{
    fmt::{self, Display, Formatter},
    ops::{Add, Div, Mul, Sub},
};

use crate::semantic::ast::{LiteralKind, TimeUnit};

#[derive(Clone, Copy, Debug, Default)]
pub struct Duration {
    pub value: f64,
    pub unit: TimeUnit,
}

impl Duration {
    pub fn new(value: f64, unit: TimeUnit) -> Self {
        Self { value, unit }
    }

    /// Returns the smaller (more precise) time unit between two units.
    /// The order from smallest to largest is: Dt < Ns < Us < Ms < S
    fn smaller_unit(unit1: TimeUnit, unit2: TimeUnit) -> TimeUnit {
        use TimeUnit::*;
        match (unit1, unit2) {
            (Dt, _) | (_, Dt) => Dt, // Dt is always the smallest
            (Ns, _) | (_, Ns) => Ns, // Ns is smallest among non-Dt units
            (Us, _) | (_, Us) => Us, // Us is next smallest
            (Ms, _) | (_, Ms) => Ms, // Ms is next smallest
            (S, S) => S,             // Both are seconds
        }
    }

    /// Converts this duration to the specified time unit.
    fn convert_to_unit(self, target_unit: TimeUnit) -> Self {
        let converted_value = match (self.unit, target_unit) {
            (TimeUnit::Us, TimeUnit::Ns)
            | (TimeUnit::Ms, TimeUnit::Us)
            | (TimeUnit::S, TimeUnit::Ms) => self.value * 1_000.0,
            (TimeUnit::Ms, TimeUnit::Ns) | (TimeUnit::S, TimeUnit::Us) => self.value * 1_000_000.0,
            (TimeUnit::S, TimeUnit::Ns) => self.value * 1_000_000_000.0,
            (TimeUnit::Ns, TimeUnit::Us)
            | (TimeUnit::Us, TimeUnit::Ms)
            | (TimeUnit::Ms, TimeUnit::S) => self.value / 1_000.0,
            (TimeUnit::Ns, TimeUnit::Ms) | (TimeUnit::Us, TimeUnit::S) => self.value / 1_000_000.0,
            (TimeUnit::Ns, TimeUnit::S) => self.value / 1_000_000_000.0,
            (TimeUnit::Dt, _)
            | (_, TimeUnit::Dt)
            | (TimeUnit::Us, TimeUnit::Us)
            | (TimeUnit::Ns, TimeUnit::Ns)
            | (TimeUnit::Ms, TimeUnit::Ms)
            | (TimeUnit::S, TimeUnit::S) => self.value,
        };

        Self {
            value: converted_value,
            unit: target_unit,
        }
    }

    /// Normalizes two durations to the same time unit, returning a tuple of the normalized durations.
    /// Chooses the smaller (more precise) unit between the two for better precision.
    pub fn normalize_pair(self, other: Self) -> (Self, Self) {
        if self.unit == other.unit {
            // Same unit, no conversion needed
            (self, other)
        } else if self.unit == TimeUnit::Dt || other.unit == TimeUnit::Dt {
            // If either is Dt, convert both to Dt (preserving the special handling)
            (
                Self {
                    value: self.value,
                    unit: TimeUnit::Dt,
                },
                Self {
                    value: other.value,
                    unit: TimeUnit::Dt,
                },
            )
        } else {
            // Choose the smaller (more precise) unit
            let target_unit = Self::smaller_unit(self.unit, other.unit);
            let converted_self = self.convert_to_unit(target_unit);
            let converted_other = other.convert_to_unit(target_unit);
            (converted_self, converted_other)
        }
    }
}

impl Display for Duration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {}", self.value, self.unit)
    }
}

impl PartialEq for Duration {
    fn eq(&self, other: &Self) -> bool {
        // Convert both to a common unit for comparison
        let (normalized_self, normalized_other) = self.normalize_pair(*other);
        f64::EPSILON > (normalized_self.value - normalized_other.value).abs()
    }
}

impl Add for Duration {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        // Normalize to a common unit and add
        let (normalized_self, normalized_rhs) = self.normalize_pair(rhs);
        let value = normalized_self.value + normalized_rhs.value;
        Self {
            value,
            unit: normalized_self.unit,
        }
    }
}

impl Sub for Duration {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        // Normalize to a common unit and subtract
        let (normalized_self, normalized_rhs) = self.normalize_pair(rhs);
        let value = normalized_self.value - normalized_rhs.value;
        Self {
            value,
            unit: normalized_self.unit,
        }
    }
}

impl Mul<f64> for Duration {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        let value = self.value * rhs;
        Self {
            value,
            unit: self.unit,
        }
    }
}

impl Mul<i64> for Duration {
    type Output = Self;

    fn mul(self, rhs: i64) -> Self::Output {
        #[allow(clippy::cast_precision_loss)]
        let value = self.value * rhs as f64;
        Self {
            value,
            unit: self.unit,
        }
    }
}

impl Div<Duration> for Duration {
    type Output = f64;

    fn div(self, rhs: Self) -> Self::Output {
        // Normalize to a common unit and divide
        let (normalized_self, normalized_rhs) = self.normalize_pair(rhs);
        normalized_self.value / normalized_rhs.value
    }
}

impl Div<f64> for Duration {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        let value = self.value / rhs;
        Self {
            value,
            unit: self.unit,
        }
    }
}

impl Div<i64> for Duration {
    type Output = Self;

    fn div(self, rhs: i64) -> Self::Output {
        #[allow(clippy::cast_precision_loss)]
        let value = self.value / rhs as f64;
        Self {
            value,
            unit: self.unit,
        }
    }
}

impl From<Duration> for LiteralKind {
    fn from(value: Duration) -> Self {
        LiteralKind::Duration(value)
    }
}

impl From<f64> for LiteralKind {
    fn from(value: f64) -> Self {
        LiteralKind::Duration(Duration::new(value, TimeUnit::default()))
    }
}

impl From<i64> for LiteralKind {
    fn from(value: i64) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let value = value as f64;
        LiteralKind::Duration(Duration::new(value, TimeUnit::default()))
    }
}
