// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an `execute` answers into: [`AppendOnlyReadouts`], a view of its
//! caller's [`Readouts`] that reads them all and only appends.

use std::fmt;
use std::ops::Deref;

use super::{BatchReadouts, Readouts};

/// The caller's [`Readouts`], lent to an `execute`: it reads them as a
/// slice of [`BatchReadouts`], and appends its answer after them, one per
/// batch, but cannot remove or change any.
///
/// So the readouts a caller holds when it lends them, such as a driver's
/// earlier rounds, stay as they were, and an `execute` that fails leaves
/// them as they were: the protocol removes what the failed `execute`
/// appended. A part that sends an `execute` below lends its own view on:
/// `below.execute(batches, seed, readouts)`. A part that needs to remove
/// what it appended, such as an adapter that retries, collects in a
/// buffer of its own and appends the answer it keeps.
///
/// An `execute` may assign or swap in a view of another buffer, one that
/// outlives the call (a leaked or static one): that changes only where
/// its own view points, never the caller's buffer, and what it appends
/// there is not its answer. The protocol
/// checks every answer that crosses a boundary of a chain against its
/// batches, as [`ShapeFault`](crate::ShapeFault) says, so an answer
/// appended elsewhere fails the `execute`, as readouts missing.
///
/// [`Readouts::append_only`] makes one.
pub struct AppendOnlyReadouts<'a> {
    readouts: &'a mut Readouts,
}

impl Readouts {
    /// A view of these readouts that only appends, to lend to an
    /// `execute`.
    #[inline]
    pub fn append_only(&mut self) -> AppendOnlyReadouts<'_> {
        AppendOnlyReadouts { readouts: self }
    }
}

impl AppendOnlyReadouts<'_> {
    /// Adds the next batch's readouts, with no rows yet, of `readouts`
    /// values each and room for `shots` rows, and returns them to fill,
    /// as [`Readouts::push_with_capacity`] does.
    ///
    /// # Panics
    ///
    /// When the room for `shots` rows is more than a `Vec` holds, here or
    /// at the first shot with conditions, as
    /// [`BatchReadouts::with_capacity`] says.
    #[inline]
    pub fn push_with_capacity(&mut self, shots: usize, readouts: u32) -> &mut BatchReadouts {
        self.readouts.push_with_capacity(shots, readouts)
    }

    /// Adds the next batch's readouts, made elsewhere, as
    /// [`Readouts::push`] does.
    #[inline]
    pub fn push(&mut self, batch: BatchReadouts) {
        self.readouts.push(batch);
    }

    /// The buffer it views, which the protocol's boundaries lend on, in
    /// fresh views, and trim when an `execute` fails.
    #[inline]
    pub(crate) fn buffer(&mut self) -> &mut Readouts {
        self.readouts
    }
}

impl Deref for AppendOnlyReadouts<'_> {
    type Target = [BatchReadouts];

    #[inline]
    fn deref(&self) -> &[BatchReadouts] {
        self.readouts
    }
}

/// The readouts in use, as a list.
impl fmt::Debug for AppendOnlyReadouts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.readouts, f)
    }
}
