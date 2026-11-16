// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The buffer an `execute` answers into: [`Readouts`], one
//! [`BatchReadouts`] per batch.
//!
//! The caller lends it, as an
//! [`AppendOnlyReadouts`](crate::AppendOnlyReadouts) view, and the
//! `execute` appends to it, as `std::io::Read::read_to_end` appends to the
//! caller's `Vec`: so a caller that sends many small `execute`s, such as a
//! driver sending a round at a time, reuses one buffer, and an `execute`
//! that fills it with
//! [`push_with_capacity`](crate::AppendOnlyReadouts::push_with_capacity)
//! allocates nothing once the buffer has held answers of its size.
//! Clearing it keeps the memory of every batch's readouts, which the next
//! answer reuses.

use std::fmt;
use std::ops::Deref;

use super::BatchReadouts;

/// The readouts of the batches of one or more `execute`s, read as a slice
/// of [`BatchReadouts`], in the order they were added.
///
/// Its owner lends it to an `execute` as an
/// [`AppendOnlyReadouts`](crate::AppendOnlyReadouts) view, made by
/// [`append_only`](Self::append_only), through which the `execute`
/// appends one per batch, in the batches' order, and cannot remove or
/// change those it found. The protocol checks every answer that crosses a
/// boundary of a chain against its batches, as
/// [`ShapeFault`](crate::ShapeFault) says, and, through the protocol, an
/// `execute` that fails leaves it as it was. Its owner
/// [`clear`](Self::clear)s it between `execute`s whose answers it no
/// longer needs; the memory stays, for the next.
#[derive(Default)]
pub struct Readouts {
    /// The batches' readouts: the first `len` in use, the rest kept, with
    /// their memory, for the next ones added.
    batches: Vec<BatchReadouts>,
    len: usize,
}

impl Readouts {
    /// No readouts, and no memory yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties it, keeping its memory.
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Adds the next batch's readouts, with no rows yet, of `readouts`
    /// values each and room for `shots` rows, and returns them to fill
    /// with [`push_row`](BatchReadouts::push_row). They reuse the memory
    /// of readouts cleared earlier, if any.
    ///
    /// # Panics
    ///
    /// When the room for `shots` rows is more than a `Vec` holds, here or
    /// at the first shot with conditions, as
    /// [`BatchReadouts::with_capacity`] says.
    #[inline]
    pub fn push_with_capacity(&mut self, shots: usize, readouts: u32) -> &mut BatchReadouts {
        if self.len == self.batches.len() {
            self.batches
                .push(BatchReadouts::with_capacity(shots, readouts));
        } else {
            self.batches[self.len].reset(shots, readouts);
        }
        self.len += 1;
        &mut self.batches[self.len - 1]
    }

    /// Adds the next batch's readouts, made elsewhere, such as by
    /// [`BatchReadouts::concat_readouts`]: they take the place of the
    /// memory kept for the next batch, which is freed.
    #[inline]
    pub fn push(&mut self, batch: BatchReadouts) {
        if self.len == self.batches.len() {
            self.batches.push(batch);
        } else {
            self.batches[self.len] = batch;
        }
        self.len += 1;
    }

    /// Keeps only the first `len` readouts, if it holds more, keeping
    /// the memory of the rest: what the protocol does to remove a failed
    /// `execute`'s answer.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.len = self.len.min(len);
    }
}

impl Deref for Readouts {
    type Target = [BatchReadouts];

    #[inline]
    fn deref(&self) -> &[BatchReadouts] {
        &self.batches[..self.len]
    }
}

impl<'a> IntoIterator for &'a Readouts {
    type Item = &'a BatchReadouts;
    type IntoIter = std::slice::Iter<'a, BatchReadouts>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The readouts in use, without the memory kept for reuse.
impl Clone for Readouts {
    fn clone(&self) -> Self {
        Self {
            batches: self.to_vec(),
            len: self.len,
        }
    }
}

/// The same readouts in the same order, whatever memory each keeps.
impl PartialEq for Readouts {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for Readouts {}

/// The readouts in use, as a list.
impl fmt::Debug for Readouts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
