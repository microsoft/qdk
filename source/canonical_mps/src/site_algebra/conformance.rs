// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conformance kit: checks a [`SiteAlgebra`] against its contract.
//!
//! The engine trusts every backend to keep the promises of
//! [`SiteAlgebra`]'s methods, so a backend's tests run the kit on it
//! instead of writing those checks again. [`check`] returns every case the
//! algebra fails, each naming its case and the pair and setting at fault:
//!
//! ```
//! use canonical_mps::site_algebra::{SiteAlgebra, conformance};
//!
//! fn assert_conforms<A: SiteAlgebra>(algebra: &A) {
//!     let failures = conformance::check(algebra);
//!     assert!(failures.is_empty(), "{failures:#?}");
//! }
//! ```
//!
//! Each case calls the algebra on seeded inputs and compares what it
//! returns with the contract's formulas, never with another algebra: the
//! gated pair `θ` computed by the formula's own loops, and `θ`'s singular
//! values, which are unique where its factors are not. A truncated split
//! is checked by the Eckart–Young theorem: no matrix of rank `k` is nearer
//! `θ` than its SVD truncated to `k` values, so a split whose product is
//! as near as the dropped values allow is that truncation. The cases, by
//! the method they check:
//!
//! - `storage.rs`: [`upload`](SiteAlgebra::upload),
//!   [`read`](SiteAlgebra::read) and
//!   [`duplicate`](SiteAlgebra::duplicate);
//! - `gate_split.rs`: [`gate_split`](SiteAlgebra::gate_split), on pairs, from
//!   one whose extents are all one (`θ` of 2 × 2) to a gated pair of 32 × 16,
//!   at caps of 1, 2, 3 and none and cutoffs of 0, 0.05 and just below 1, on
//!   either side, and on a gated pair of 64 × 64 without truncation;
//! - `qr_move.rs`: [`qr_move`](SiteAlgebra::qr_move), on the same pairs
//!   and on bonds larger than the side of the site the centre leaves,
//!   with and without a gate, either way.
//!
//! Each split and each move reads its input tensors back too, and a case
//! of each method checks they are unchanged.
//!
//! An error on any call is a failure of its case: the kit makes only the
//! calls the contract allows. The latitude the contract leaves is not
//! checked: a bond larger than the cutoff needs, and the side of a tie a
//! split takes. No fault of the algebra makes the kit panic, so a failing
//! algebra reports every case it fails, except that a tensor shorter than
//! the bond the algebra reported may make the algebra's own read panic.
//! The kit needs the `conformance` feature, so it costs nothing in a build
//! that does not test a backend.

mod formulas;
mod gate_split;
mod inputs;
mod qr_move;
mod storage;

use super::SiteAlgebra;
use num_complex::Complex64;
use std::fmt;

/// How far a product, a retained fraction or an isometry may be from its
/// formula, in the Frobenius norm of the normalised pair or the largest entry:
/// a backend in double precision rounds far below it on the kit's largest pair
/// (64 × 64), and a fault in the layout errs by about one.
pub const TOLERANCE: f64 = 1e-10;

/// Runs every case against `algebra` and returns the failures, empty when
/// the algebra conforms.
pub fn check<A: SiteAlgebra>(algebra: &A) -> Vec<Failure> {
    cases()
        .into_iter()
        .filter_map(|(case, run)| run(algebra).err().map(|message| Failure { case, message }))
        .collect()
}

/// The kit's cases, in the order [`check`] runs them.
fn cases<A: SiteAlgebra>() -> [Case<A>; 15] {
    [
        ("upload and read", storage::upload_and_read),
        ("duplicate", storage::duplicate),
        ("split without truncation", gate_split::without_truncation),
        ("split truncation", gate_split::truncation),
        ("split bond", gate_split::bond),
        ("split isometry", gate_split::isometry),
        ("split cutoff", gate_split::cutoff),
        (
            "split retains the fraction of its kept squares",
            gate_split::retained_fraction,
        ),
        (
            "split that drops nothing retains exactly 1",
            gate_split::drops_nothing,
        ),
        (
            "split at a tie retains the fraction of its bond",
            gate_split::at_a_tie,
        ),
        ("split inputs unchanged", gate_split::inputs_unchanged),
        ("move product", qr_move::product),
        ("move bond", qr_move::bond),
        ("move isometry", qr_move::isometry),
        ("move inputs unchanged", qr_move::inputs_unchanged),
    ]
}

/// One case an algebra failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The case that failed.
    pub case: &'static str,
    /// What differed from the contract, and on which pair and setting.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.case, self.message)
    }
}

/// A case of the kit: the name its [`Failure`] gives it, and the function
/// that runs it on the algebra.
type Case<A> = (&'static str, fn(&A) -> Result<(), String>);

/// A new tensor holding `values`, or why the algebra failed to make it.
fn upload<A: SiteAlgebra>(algebra: &A, values: &[Complex64]) -> Result<A::Tensor, String> {
    algebra
        .upload(values)
        .map_err(|error| format!("an upload failed: {error}"))
}

/// The first `len` elements of `tensor`, or why the algebra failed to read
/// them, or read another number of them.
fn read<A: SiteAlgebra>(
    algebra: &A,
    tensor: &A::Tensor,
    len: usize,
) -> Result<Vec<Complex64>, String> {
    let values = algebra
        .read(tensor, len)
        .map_err(|error| format!("a read failed: {error}"))?;
    if values.len() == len {
        Ok(values.into_owned())
    } else {
        Err(format!(
            "a read of {len} elements returned {}",
            values.len()
        ))
    }
}
