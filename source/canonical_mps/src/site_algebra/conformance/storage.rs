// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of a site's storage: [`SiteAlgebra::upload`],
//! [`SiteAlgebra::read`] and [`SiteAlgebra::duplicate`].

use super::inputs::{Pair, values};
use super::{read, upload};
use crate::site_algebra::{CentreSide, QrMoveCall, SiteAlgebra};

/// A read of the first `len` elements of an uploaded tensor returns
/// exactly those elements, for `len` from none to all. Catches a read
/// that returns other elements or another number of them, and an upload
/// that changes them.
pub(super) fn upload_and_read<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    let values = values(12, 700);
    let tensor = upload(algebra, &values)?;
    for len in [0, 1, 7, 12] {
        let read = read(algebra, &tensor, len)?;
        if read != values[..len] {
            return Err(format!(
                "a read of {len} of 12 uploaded elements returned others"
            ));
        }
    }
    Ok(())
}

/// A duplicate holds the first `len` elements of its source, and either
/// outlives the other unchanged: dropping the source leaves the copy, and
/// dropping the copy, or passing it to a move of the centre, leaves the
/// source. Catches a duplicate that copies other elements, and one that
/// shares the source's storage.
pub(super) fn duplicate<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    let values = values(12, 710);

    let source = upload(algebra, &values)?;
    let copy = duplicate_of(algebra, &source, 7)?;
    if read(algebra, &copy, 7)? != values[..7] {
        return Err("a duplicate of 7 of 12 elements holds others".to_owned());
    }
    drop(source);
    if read(algebra, &copy, 7)? != values[..7] {
        return Err("a duplicate changed when its source was dropped".to_owned());
    }

    let source = upload(algebra, &values)?;
    drop(duplicate_of(algebra, &source, 12)?);
    if read(algebra, &source, 12)? != values {
        return Err("a tensor changed when its duplicate was dropped".to_owned());
    }

    // The copy is the left site `[2, 2, 3]` of a pair whose right site is
    // `[3, 2, 1]`; the move's outcome is not checked here.
    let copy = duplicate_of(algebra, &source, 12)?;
    let pair = Pair::graded((2, 3, 1), 720);
    let right = upload(algebra, &pair.right)?;
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
        .map_err(|error| format!("a move of a duplicate failed: {error}"))?;
    drop((outcome, copy));
    if read(algebra, &source, 12)? != values {
        return Err("a tensor changed when its duplicate was moved".to_owned());
    }
    Ok(())
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
