// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The one rule of a required value against a provided one, which every
//! kind of statement follows, with keys and values of the test's own.

use crate::{StatementMismatch, Statements};

/// Keys of no kind the crate has, so that the rule is checked as a rule of
/// any key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    A,
    B,
    C,
}

/// The statements of `pairs`, stated in the order given.
fn stating<V: Ord + Copy>(pairs: &[(Key, V)]) -> Statements<Key, V> {
    (pairs.iter()).fold(Statements::new(), |stated, &(key, value)| {
        stated.with(key, value)
    })
}

/// The mismatch of `key`, `required` above `provided`.
fn above<V>(key: Key, required: V, provided: V) -> StatementMismatch<Key, V> {
    StatementMismatch {
        key,
        required,
        provided,
    }
}

/// A required value fits a provided one unless the provider states a
/// smaller one, so that a part states only what it needs or limits: a key
/// a part does not state, it neither needs nor limits, on either side.
/// Each value required above the one provided is listed, with both values,
/// every one at once and in the keys' order, so that the faults read alike
/// whatever order the parts stated them in. For yes-or-no values, `true`
/// required over `false` provided is the one mismatch. A fit refusing an
/// equal value fails "an equal value"; one reading a key the provider does
/// not state as a limit fails "a key the provider does not state"; one
/// listing in another order fails "every larger value".
#[test]
fn a_required_value_fits_unless_a_stated_provided_value_is_smaller() {
    use Key::{A, B, C};
    for (row, required, provided, expected) in [
        (
            "a key the requirer does not state",
            &[][..],
            &[(A, 0)][..],
            vec![],
        ),
        (
            "a key the provider does not state",
            &[(A, u32::MAX)],
            &[],
            vec![],
        ),
        ("an equal value", &[(A, 4)], &[(A, 4)], vec![]),
        ("a smaller value", &[(A, 3)], &[(A, 4)], vec![]),
        ("a larger value", &[(A, 5)], &[(A, 4)], vec![above(A, 5, 4)]),
        (
            "every larger value, a fitting one between them",
            &[(C, 9), (B, 1), (A, 5)],
            &[(B, 2), (C, 8), (A, 4)],
            vec![above(A, 5, 4), above(C, 9, 8)],
        ),
    ] {
        assert_eq!(
            stating(required).mismatches(&stating(provided)),
            expected,
            "{row}"
        );
    }
    for (row, required, provided, expected) in [
        (
            "`true` over `false`",
            true,
            false,
            vec![above(A, true, false)],
        ),
        ("`false` over `true`", false, true, vec![]),
    ] {
        assert_eq!(
            stating(&[(A, required)]).mismatches(&stating(&[(A, provided)])),
            expected,
            "{row}"
        );
    }
}
