// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_bigint::BigUint;
use num_complex::Complex;

use crate::displayable_output::DisplayableState;

#[test]
fn display_neg_zero() {
    let s = DisplayableState(vec![(BigUint::default(), Complex::new(-0.0, -0.0))], 1);
    // -0 should be displayed as 0.0000 without a minus sign
    assert_eq!("STATE:\n|0⟩: 0.0000+0.0000𝑖", s.to_plain());
}

#[test]
fn display_rounds_to_neg_zero() {
    let s = DisplayableState(
        vec![(BigUint::default(), Complex::new(-0.00001, -0.00001))],
        1,
    );
    // -0.00001 should be displayed as 0.0000 without a minus sign
    assert_eq!("STATE:\n|0⟩: 0.0000+0.0000𝑖", s.to_plain());
}

#[test]
fn display_preserves_order() {
    let s = DisplayableState(
        vec![
            (BigUint::from(0_u64), Complex::new(0.0, 0.0)),
            (BigUint::from(1_u64), Complex::new(0.0, 1.0)),
            (BigUint::from(2_u64), Complex::new(1.0, 0.0)),
            (BigUint::from(3_u64), Complex::new(1.0, 1.0)),
        ],
        2,
    );
    assert_eq!(
        "STATE:\n|00⟩: 0.0000+0.0000𝑖\n|01⟩: 0.0000+1.0000𝑖\n|10⟩: 1.0000+0.0000𝑖\n|11⟩: 1.0000+1.0000𝑖",
        s.to_plain()
    );
}
