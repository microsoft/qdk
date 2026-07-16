// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use core::f64;
use std::{
    fmt::{self, Display, Formatter},
    ops::{Add, Div, Mul, Neg, Sub},
};

use crate::semantic::ast::LiteralKind;

#[derive(Clone, Copy, Debug, Default)]
pub struct Complex {
    pub real: f64,
    pub imag: f64,
}

impl Complex {
    #[must_use]
    pub fn new(real: f64, imag: f64) -> Self {
        Self { real, imag }
    }

    #[must_use]
    pub fn real(real: f64) -> Self {
        Self { real, imag: 0. }
    }

    #[must_use]
    pub fn imag(imag: f64) -> Self {
        Self { real: 0., imag }
    }
}

impl Display for Complex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}, {:?}", self.real, self.imag)
    }
}

impl Neg for Complex {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            real: -self.real,
            imag: -self.imag,
        }
    }
}

impl PartialEq for Complex {
    fn eq(&self, other: &Self) -> bool {
        self.real == other.real && self.imag == other.imag
    }
}

impl Add for Complex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let real = self.real + rhs.real;
        let imag = self.imag + rhs.imag;
        Self { real, imag }
    }
}

impl Sub for Complex {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let real = self.real - rhs.real;
        let imag = self.imag - rhs.imag;
        Self { real, imag }
    }
}

impl Mul for Complex {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        let a = self;
        let b = rhs;
        let real = a.real * b.real - a.imag * b.imag;
        let imag = a.real * b.imag + a.imag * b.real;
        Self { real, imag }
    }
}

impl Div for Complex {
    type Output = Self;

    /// This mimics Q#'s implementation.
    /// ```qsharp
    /// function DividedByC(a : Complex, b : Complex) : Complex {
    ///     let sqNorm = b.Real * b.Real + b.Imag * b.Imag;
    ///     Complex(
    ///         (a.Real * b.Real + a.Imag * b.Imag) / sqNorm,
    ///         (a.Imag * b.Real - a.Real * b.Imag) / sqNorm
    ///     )
    /// }
    /// ```
    fn div(self, rhs: Self) -> Self::Output {
        let a = self;
        let b = rhs;
        let sq_norm = b.real * b.real + b.imag * b.imag;
        let real = (a.real * b.real + a.imag * b.imag) / sq_norm;
        let imag = (a.imag * b.real - a.real * b.imag) / sq_norm;
        Self { real, imag }
    }
}

impl Complex {
    /// This mimics Q#'s `PowC` implementation.
    #[must_use]
    pub fn pow(self, rhs: Self) -> Self {
        let (a, b) = (self.real, self.imag);
        let (c, d) = (rhs.real, rhs.imag);

        let base_sq_norm = a * a + b * b;
        let base_norm = base_sq_norm.sqrt();
        let base_arg = b.atan2(a);
        let magnitude = base_norm.powf(c) / f64::consts::E.powf(d * base_arg);
        let angle = d * base_norm.ln() + c * base_arg;

        Self {
            real: magnitude * angle.cos(),
            imag: magnitude * angle.sin(),
        }
    }
}

impl From<Complex> for LiteralKind {
    fn from(value: Complex) -> Self {
        LiteralKind::Complex(value)
    }
}
