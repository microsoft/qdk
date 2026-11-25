// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of `SettledPairs` (`svd::jacobi::settled_pairs`): the pairs a
//! sweep skips, those found orthogonal until one of their columns rotates.
//!
//! - A rotation unsettles every pair of its two columns, and no other.
//! - A pair found orthogonal after its rotation is settled.

use super::SettledPairs;

/// Four columns, every pair settled, then columns 1 and 2 rotate
/// together. Every pair holding column 1 or 2, as its first column or
/// its second, is unsettled, the rotated pair included; the pair
/// (0, 3), whose columns did not rotate, stays settled.
#[test]
fn unsettles_every_pair_of_a_rotated_column_and_no_other() {
    let pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
    let mut settled = SettledPairs::new(4);
    for (p, q) in pairs {
        settled.settle(p, q);
    }
    settled.unsettle_pairs_of(1, 2);
    for (p, q) in pairs {
        assert_eq!(settled.contains(p, q), (p, q) == (0, 3), "({p}, {q})");
    }
}

/// Columns 0 and 1 of three rotate, then are found orthogonal: the
/// pair is settled from then on, though nothing has happened since
/// its rotation.
#[test]
fn settles_a_pair_found_orthogonal_after_its_rotation() {
    let mut settled = SettledPairs::new(3);
    assert!(!settled.contains(0, 1), "no pair is settled at first");
    settled.unsettle_pairs_of(0, 1);
    settled.settle(0, 1);
    assert!(settled.contains(0, 1));
}
