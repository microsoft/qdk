// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The conformance kit: checks a [`SiteAlgebra`] against its contract.
//!
//! The engine trusts every backend to keep the promises of
//! [`SiteAlgebra`]'s methods, so a backend's tests run the kit on it
//! instead of writing those checks again. [`check`] returns every case the
//! algebra fails, each naming the rule it broke, the kit's seed, and the
//! pair and setting at fault:
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
//! values, which are unique where its factors are not. [`check`] draws the
//! inputs from [`DEFAULT_SEED`]; [`check_with_seed`] draws them from another
//! seed, so that a backend is not fitted to one set of inputs, and every
//! failure names the seed that repeats it. A truncated split
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
//! of each method checks they are unchanged; another reads its new sites
//! one element past the end its bond gives them, and checks the read is
//! refused.
//!
//! An error on any call is a failure of its case, but for the read past a
//! new site's end: the kit makes only the calls the contract allows, and
//! that read, which the contract says is refused. The latitude the
//! contract leaves is not checked: a bond larger than the cutoff needs,
//! and the side of a tie a split takes. The kit's own code never panics on
//! what an algebra returns, so a failing algebra reports every case it
//! fails; a panic inside the algebra's own call (a read of a tensor shorter
//! than the bond the algebra reported, for instance) is the algebra's
//! failure, which the test harness reports, and it stops the kit, except
//! in the read past a new site's end, where a panic is the refusal the
//! case expects, and the kit catches it.
//! The kit needs the `conformance` feature, so it costs nothing in a build
//! that does not test a backend.

mod formulas;
mod gate_split;
pub(crate) mod inputs;
mod qr_move;
mod storage;

use super::SiteAlgebra;
use num_complex::Complex64;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};

/// How far a product, a retained fraction or an isometry may be from its
/// formula, in the Frobenius norm of the normalised pair or the largest entry:
/// a backend in double precision rounds far below it on the kit's largest pair
/// (64 × 64), and a fault in the layout errs by about one.
pub const TOLERANCE: f64 = 1e-10;

/// The kit's seed of [`check`].
pub const DEFAULT_SEED: u64 = 0;

/// Runs every case against `algebra` on the inputs of [`DEFAULT_SEED`] and
/// returns the failures, empty when the algebra conforms.
pub fn check<A: SiteAlgebra>(algebra: &A) -> Vec<Failure> {
    check_with_seed(algebra, DEFAULT_SEED)
}

/// Runs every case against `algebra` on inputs drawn from `seed` and
/// returns the failures, empty when the algebra conforms. Every seed draws
/// the same shapes and settings, with other values.
pub fn check_with_seed<A: SiteAlgebra>(algebra: &A, seed: u64) -> Vec<Failure> {
    cases()
        .into_iter()
        .filter_map(|(case, run)| {
            let message = run(algebra, seed).err()?;
            Some(Failure {
                case,
                seed,
                message,
            })
        })
        .collect()
}

/// The kit's cases, in the order [`check`] runs them.
fn cases<A: SiteAlgebra>() -> [Case<A>; 17] {
    [
        (
            "a read returns what was uploaded",
            storage::read_returns_what_was_uploaded,
        ),
        (
            "a duplicate is a copy in its own storage",
            storage::duplicate_is_a_copy_in_its_own_storage,
        ),
        (
            "an untruncated split gives back θ/‖θ‖",
            gate_split::untruncated_split_gives_back_theta,
        ),
        (
            "a truncated split keeps θ's largest values, normalised",
            gate_split::truncated_split_keeps_the_largest_values,
        ),
        (
            "a split's bond lies within its cap and θ's sides",
            gate_split::bond_lies_within_the_cap_and_theta_sides,
        ),
        (
            "a split leaves the site away from the centre an isometry",
            gate_split::site_away_from_the_centre_is_an_isometry,
        ),
        (
            "a split drops at most its cutoff's share",
            gate_split::drops_at_most_the_cutoffs_share,
        ),
        (
            "a split retains the fraction of its kept squares",
            gate_split::retains_the_fraction_of_its_kept_squares,
        ),
        (
            "a split that drops nothing retains exactly 1",
            gate_split::dropping_nothing_retains_exactly_one,
        ),
        (
            "a split at a tie retains the fraction of its bond",
            gate_split::at_a_tie_retains_the_fraction_of_its_bond,
        ),
        (
            "a split leaves its inputs unchanged",
            gate_split::leaves_its_inputs_unchanged,
        ),
        (
            "a split's new sites hold exactly their elements",
            gate_split::new_sites_hold_exactly_their_elements,
        ),
        (
            "a move gives back the gated pair's product",
            qr_move::gives_back_the_gated_product,
        ),
        (
            "a move's bond is the smaller of the pair's bond and the side it leaves",
            qr_move::bond_is_the_smaller_of_the_bond_and_the_side_it_leaves,
        ),
        (
            "a move leaves the site it leaves an isometry",
            qr_move::site_it_leaves_is_an_isometry,
        ),
        (
            "a move leaves its inputs unchanged",
            qr_move::leaves_its_inputs_unchanged,
        ),
        (
            "a move's new sites hold exactly their elements",
            qr_move::new_sites_hold_exactly_their_elements,
        ),
    ]
}

/// One case an algebra failed. A split that ignores its cap, for instance,
/// fails a case with:
///
/// ```text
/// a split's bond lies within its cap and θ's sides (kit seed 0): the pair
/// [1, 1, 1] of seed 300, cap 1, cutoff 0, centre Left: bond 2 is not in 1..=1
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The case that failed, named for the rule of the contract it checks,
    /// for instance `a move leaves the site it leaves an isometry`, so a
    /// backend's author knows which method to fix.
    pub case: &'static str,
    /// The kit's seed the inputs were drawn from: [`check_with_seed`] with
    /// it repeats the failure.
    pub seed: u64,
    /// What differed from the contract, and on which pair and setting.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (kit seed {}): {}",
            self.case, self.seed, self.message
        )
    }
}

/// A case of the kit: the name its [`Failure`] gives it, and the function
/// that runs it on the algebra, on inputs drawn from the kit's seed.
type Case<A> = (&'static str, fn(&A, u64) -> Result<(), String>);

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

/// Whether `algebra` refuses a read of one element more than the `held`
/// elements of `tensor`, as [`SiteAlgebra::read`] promises: with an error or
/// a panic, which this catches, rather than with elements.
fn refuses_a_read_past<A: SiteAlgebra>(algebra: &A, tensor: &A::Tensor, held: usize) -> bool {
    let answered = panic::catch_unwind(AssertUnwindSafe(|| algebra.read(tensor, held + 1).is_ok()));
    !matches!(answered, Ok(true))
}
