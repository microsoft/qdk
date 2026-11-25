// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of a site's storage: [`SiteAlgebra::upload`],
//! [`SiteAlgebra::read`] and [`SiteAlgebra::duplicate`].

use super::inputs::{Pair, derived, values};
use super::{read, upload};
use crate::site_algebra::{CentreSide, QrMoveCall, SiteAlgebra};
use num_complex::Complex64;

/// A read of the first `len` elements of an uploaded tensor returns exactly
/// those elements, for `len` from none to all. Catches a read that returns
/// other elements or another number of them, and an upload that changes them.
/// For instance, a read that negates its first element fails it with: `12
/// values of seed 700: a read of 1 of 12 uploaded elements returned others`.
pub(super) fn read_returns_what_was_uploaded<A: SiteAlgebra>(
    algebra: &A,
    kit_seed: u64,
) -> Result<(), String> {
    let seed = derived(kit_seed, 700);
    let failed = failure_on_values_of(seed);
    let values = values(12, seed);
    let tensor = upload(algebra, &values).map_err(&failed)?;
    for len in [0, 1, 7, 12] {
        let read = read(algebra, &tensor, len).map_err(&failed)?;
        if read != values[..len] {
            return Err(failed(format!(
                "a read of {len} of 12 uploaded elements returned others"
            )));
        }
    }
    Ok(())
}

/// A duplicate holds the first `len` elements of its source. Either outlives
/// the other unchanged. Dropping the source leaves the copy. Dropping the
/// copy, or passing it to a move of the centre, leaves the source.
/// Catches a duplicate that copies other elements, and one that shares the
/// source's storage. For instance, a duplicate that shares its source's storage
/// fails it with: `12 values of seed 710: a duplicate changed when its source
/// was dropped`.
pub(super) fn duplicate_is_a_copy_in_its_own_storage<A: SiteAlgebra>(
    algebra: &A,
    kit_seed: u64,
) -> Result<(), String> {
    let seed = derived(kit_seed, 710);
    let values = values(12, seed);
    copy_outlives_its_source(algebra, &values, seed)?;
    let source = upload(algebra, &values).map_err(failure_on_values_of(seed))?;
    dropped_copy_leaves_its_source(algebra, &source, &values, seed)?;
    moved_copy_leaves_its_source(algebra, &source, &values, seed, kit_seed)
}

/// The first step of [`duplicate_is_a_copy_in_its_own_storage`]: a
/// duplicate of 7 of the 12 `values` holds them, and still does once its
/// source is dropped.
fn copy_outlives_its_source<A: SiteAlgebra>(
    algebra: &A,
    values: &[Complex64],
    seed: u64,
) -> Result<(), String> {
    let failed = failure_on_values_of(seed);
    let source = upload(algebra, values).map_err(&failed)?;
    let copy = duplicate_of(algebra, &source, 7).map_err(&failed)?;
    if read(algebra, &copy, 7).map_err(&failed)? != values[..7] {
        return Err(failed(
            "a duplicate of 7 of 12 elements holds others".to_owned(),
        ));
    }
    drop(source);
    if read(algebra, &copy, 7).map_err(&failed)? != values[..7] {
        return Err(failed(
            "a duplicate changed when its source was dropped".to_owned(),
        ));
    }
    Ok(())
}

/// The second step of [`duplicate_is_a_copy_in_its_own_storage`]:
/// `source`, holding `values`, still does once a duplicate of it is
/// dropped.
fn dropped_copy_leaves_its_source<A: SiteAlgebra>(
    algebra: &A,
    source: &A::Tensor,
    values: &[Complex64],
    seed: u64,
) -> Result<(), String> {
    let failed = failure_on_values_of(seed);
    drop(duplicate_of(algebra, source, 12).map_err(&failed)?);
    if read(algebra, source, 12).map_err(&failed)? != values {
        return Err(failed(
            "a tensor changed when its duplicate was dropped".to_owned(),
        ));
    }
    Ok(())
}

/// The last step of [`duplicate_is_a_copy_in_its_own_storage`]: `source`,
/// holding `values`, still does once a duplicate of it is passed to a move
/// of the centre, and the move's outcome dropped.
fn moved_copy_leaves_its_source<A: SiteAlgebra>(
    algebra: &A,
    source: &A::Tensor,
    values: &[Complex64],
    seed: u64,
    kit_seed: u64,
) -> Result<(), String> {
    // The copy is the left site `[2, 2, 3]` of a pair whose right site is
    // `[3, 2, 1]`; the move's outcome is not checked here.
    let copy = duplicate_of(algebra, source, 12).map_err(failure_on_values_of(seed))?;
    let pair = Pair::graded((2, 3, 1), derived(kit_seed, 720));
    let failed = |message: String| format!("12 values of seed {seed}, {pair}: {message}");
    let right = upload(algebra, &pair.right).map_err(failed)?;
    let call = QrMoveCall {
        left: &copy,
        right: &right,
        left_extent: 2,
        bond: 3,
        right_extent: 1,
        gate: None,
        centre: CentreSide::Right,
    };
    let outcome = algebra
        .qr_move(&call)
        .map_err(|error| failed(format!("a move of a duplicate failed: {error}")))?;
    drop((outcome, copy));
    if read(algebra, source, 12).map_err(failed)? != values {
        return Err(failed(
            "a tensor changed when its duplicate was moved".to_owned(),
        ));
    }
    Ok(())
}

/// What a failure on the 12 values of `seed` reports: `message`, prefixed
/// with those values' seed.
fn failure_on_values_of(seed: u64) -> impl Fn(String) -> String {
    move |message| format!("12 values of seed {seed}: {message}")
}

/// A duplicate of the first `len` elements of `tensor`, or why the
/// algebra failed to make it.
fn duplicate_of<A: SiteAlgebra>(
    algebra: &A,
    tensor: &A::Tensor,
    len: usize,
) -> Result<A::Tensor, String> {
    algebra
        .duplicate(tensor, len)
        .map_err(|error| format!("a duplicate failed: {error}"))
}
