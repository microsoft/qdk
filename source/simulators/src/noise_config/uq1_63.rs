// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

/// This value is 1.0 in `UQ1.63` format (high order bit is 1, rest are 0).
pub(crate) const ONE: u64 = 1u64 << 63;

/// Maps an `f64` in the range`[0.0, 1.0]` to a `u64` in the `UQ1.63` format.
///
/// You can learn more at: <https://en.wikipedia.org/wiki/Q_(number_format)>.
pub(crate) fn from_prob(p: f64) -> u64 {
    // Only allow values from 0 to 1.0 for the incoming probability.
    assert!(
        (0.0..=1.0).contains(&p),
        "a probability should be a number between 0.0 and 1.0"
    );
    let bits: u64 = p.to_bits();

    // For a double-precision float:
    // - 1 bit sign (bit 63)
    // - 11 bits exponent (bits 62-52)
    // - 52 bits fraction (bits 51-0)
    //
    // The exponent is stored with a bias of 1023 (i.e., actual exponent + 1023).

    let exponent = (bits >> 52) & 0x7FF;
    let fraction = bits & ((1u64 << 52) - 1);

    if exponent == 0 {
        // zero or subnormal value
        return 0;
    }
    // Add back the implicit leading 1 to the significand
    let m = (1u64 << 52) | fraction;
    // For Q1.63, we need to adjust the exponent. Shift by 11 to account for the fixed-point format.
    let k = (exponent as i32) - 1012;
    if k >= 0 {
        m << k
    } else {
        let r = -k;
        if r >= 64 {
            return 0;
        }
        m >> r
    }
}
