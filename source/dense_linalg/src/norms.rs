// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The norm of a vector, or the Frobenius norm of a matrix, from its
//! elements: [`squared_norm`], [`norm`], and [`norm_divided_by`], which
//! stays in range at any scale. `docs/algorithms.md`'s Terms define the
//! norms.
//!
//! Each sums the squares from the first element to the last. Summed in
//! one fixed order, a norm has the same bits on every run and target.

use num_complex::Complex64;

/// `Σ |xᵢ|²`, the sum of the elements' squared moduli, from the first to
/// the last: a vector's squared norm, or a matrix's squared Frobenius norm
/// when `x` holds its elements. It is 0 when `x` is empty.
///
/// The squares overflow to infinity when an element's modulus is above
/// about `2⁵¹²`, and lose digits, or vanish, below about `2⁻⁵¹¹`;
/// [`norm_divided_by`] avoids both.
#[inline]
#[must_use]
pub fn squared_norm(x: &[Complex64]) -> f64 {
    x.iter().map(Complex64::norm_sqr).sum()
}

/// `‖x‖ = √(Σ |xᵢ|²)`, the square root of [`squared_norm`]: a vector's
/// norm, or a matrix's Frobenius norm when `x` holds its elements. Its
/// range is [`squared_norm`]'s.
#[inline]
#[must_use]
pub fn norm(x: &[Complex64]) -> f64 {
    squared_norm(x).sqrt()
}

/// `‖x‖ / divisor`, computed as `‖x / divisor‖`: each element is divided
/// by `divisor` before it is squared, and the squares are summed from the
/// first to the last. With `divisor` about the largest modulus of `x`, the
/// largest squares are near 1. So, at any scale, their sum neither
/// overflows nor underflows, where `norm(x) / divisor` would give infinity
/// or 0, or lose digits. A far smaller element's square may still
/// underflow, far below the sum's rounding.
///
/// The caller passes a positive `divisor`, for instance 1 for the zero
/// vector.
#[inline]
#[must_use]
pub fn norm_divided_by(x: &[Complex64], divisor: f64) -> f64 {
    x.iter()
        .map(|x| (x / divisor).norm_sqr())
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests;
