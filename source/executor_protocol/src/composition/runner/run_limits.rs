// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The limits of each run ([`RunLimits`]): the most shots and whether a
//! seed may be fixed, as every part's offer meets them.

use crate::Offer;

/// The limits of each run: the most shots and whether a seed may be fixed,
/// met by every part's offer, the executor's and each adapter's, so that an
/// adapter may lower them, such as a job queue, but never raise them above
/// what a part below offers: an adapter that restates its offer, such as
/// `Offer::unlimited().max_qubits(n)`, keeps the shots and
/// seedability of the parts below.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunLimits {
    /// The fewest of the most shots any part offers; `None`: no part has a
    /// limit.
    pub(crate) max_shots: Option<u32>,
    /// Whether every part is seedable.
    pub(crate) seedable: bool,
}

impl RunLimits {
    /// The limits that `offers`, every part's, all meet.
    pub(crate) fn of<'a>(offers: impl IntoIterator<Item = &'a Offer>) -> Self {
        offers.into_iter().fold(
            Self {
                max_shots: None,
                seedable: true,
            },
            |limits, offer| Self {
                max_shots: match (limits.max_shots, offer.max_shots) {
                    (Some(most), Some(its)) => Some(most.min(its)),
                    (most, its) => most.or(its),
                },
                seedable: limits.seedable && offer.seedable,
            },
        )
    }
}
