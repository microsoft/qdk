// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex;

/// `NearlyZero` trait allows for approximate evaluation of a value to the additive identity.
pub trait NearlyZero {
    fn is_nearly_zero(&self) -> bool;
}

impl NearlyZero for f64 {
    fn is_nearly_zero(&self) -> bool {
        self.max(0.0) - 0.0_f64.min(*self) <= 1e-10
    }
}

impl<T> NearlyZero for Complex<T>
where
    T: NearlyZero,
{
    fn is_nearly_zero(&self) -> bool {
        self.re.is_nearly_zero() && self.im.is_nearly_zero()
    }
}
