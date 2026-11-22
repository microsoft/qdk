// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How amounts fit: a required amount fits unless the provider states a
//! smaller one.

use crate::tests::fake_parts::capabilities;
use crate::{BoundaryCompatible, Features, ResourceMismatch, Resources};

/// Each amount `required` states above what `provided` states, as
/// [`Capabilities`](crate::Capabilities)' fit lists them.
fn fit(required: Resources, provided: Resources) -> Vec<ResourceMismatch> {
    let fit = capabilities(required, Features::unconstrained())
        .fits(&capabilities(provided, Features::unconstrained()));
    fit.map_or_else(|mismatch| mismatch.resources, |()| Vec::new())
}

/// A required amount fits a provided one unless the provider states a
/// smaller one, so that a part states only what it needs or limits: an
/// amount a part does not state, it neither needs nor limits, on either
/// side. Each amount required above the one provided is listed, with both
/// amounts, every one at once and in the fields' order. A fit refusing an
/// equal amount fails "an equal amount"; one reading an amount the
/// provider does not state as a limit fails "an amount the provider does
/// not state"; one listing only the first, or in another order, fails
/// "every larger amount".
#[test]
fn a_required_amount_fits_unless_a_stated_provided_amount_is_smaller() {
    let none = Resources::unconstrained();
    let shots = |n| none.with_shots(n);
    for (row, required, provided, expected) in [
        (
            "an amount the requirer does not state",
            none,
            shots(0),
            vec![],
        ),
        (
            "an amount the provider does not state",
            shots(u32::MAX),
            none,
            vec![],
        ),
        ("an equal amount", shots(4), shots(4), vec![]),
        ("a smaller amount", shots(3), shots(4), vec![]),
        (
            "a larger amount",
            shots(5),
            shots(4),
            vec![ResourceMismatch::Shots {
                required: 5,
                provided: 4,
            }],
        ),
        (
            "every larger amount",
            none.with_instructions_per_batch(9).with_shots(5),
            none.with_shots(4).with_instructions_per_batch(8),
            vec![
                ResourceMismatch::Shots {
                    required: 5,
                    provided: 4,
                },
                ResourceMismatch::InstructionsPerBatch {
                    required: 9,
                    provided: 8,
                },
            ],
        ),
    ] {
        assert_eq!(fit(required, provided), expected, "{row}");
    }
}
