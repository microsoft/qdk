// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// `i64` is 64 bits wide, but `f64`'s mantissa is only 53 bits wide
pub(crate) fn safe_i64_to_f64(value: i64) -> Option<f64> {
    const MAX_EXACT_INT: i64 = 2i64.pow(f64::MANTISSA_DIGITS);
    const MAX_EXACT_NEG_INT: i64 = -(2i64.pow(f64::MANTISSA_DIGITS));
    if (MAX_EXACT_NEG_INT..=MAX_EXACT_INT).contains(&value) {
        #[allow(clippy::cast_precision_loss)]
        Some(value as f64)
    } else {
        None
    }
}

pub(crate) fn safe_u64_to_f64(value: u64) -> Option<f64> {
    const MAX_EXACT_UINT: u64 = 2u64.pow(f64::MANTISSA_DIGITS);
    if value <= MAX_EXACT_UINT {
        #[allow(clippy::cast_precision_loss)]
        Some(value as f64)
    } else {
        None
    }
}
