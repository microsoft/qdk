// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The buffer an `execute` answers into: [`Readouts`], one
//! [`BatchReadouts`] per batch.
//!
//! The caller lends it, and the `execute` appends to it, as
//! `std::io::Read::read_to_end` appends to the caller's `Vec`: so a caller
//! that sends many small `execute`s, such as a driver sending a round at a
//! time, reuses one buffer, and an `execute` that fills it with
//! [`Readouts::push_with_capacity`] allocates nothing once the buffer has held
//! answers of its size. Clearing it keeps the memory of every batch's
//! readouts, which the next answer reuses.

use std::fmt;
use std::ops::Deref;

use super::BatchReadouts;

/// The readouts of the batches of one or more `execute`s, read as a slice
/// of [`BatchReadouts`], in the order they were added.
///
/// An `execute` appends one per batch, in the batches' order. Through the
/// protocol, it cannot change those it found unseen: it reads them as a
/// slice, its [`clear`](Self::clear) keeps them, and replacing the whole
/// value, when it found readouts, fails. The protocol checks every answer
/// that crosses a boundary of a chain against its batches, as
/// [`ShapeFault`](crate::ShapeFault) says, and an `execute` that fails
/// leaves it as it was, unless it replaced it. Its owner `clear`s it
/// between `execute`s whose answers it no longer needs; the memory stays,
/// for the next.
#[derive(Default)]
pub struct Readouts {
    /// The batches' readouts: the first `len` in use, the rest kept, with
    /// their memory, for the next ones added.
    batches: Vec<BatchReadouts>,
    len: usize,
    /// While it is lent to an `execute` through the protocol, the count of
    /// readouts that `execute` found, which `clear` keeps; 0 otherwise.
    found_len: usize,
}

/// A lending of [`Readouts`] to an `execute`, from [`Readouts::lend`] to
/// [`Readouts::end_loan`], which takes it: not `Copy`, so that each loan
/// is ended once.
pub(crate) struct Loan {
    /// The readouts the `execute` found: its answer starts there.
    start: usize,
    /// What `found_len` was before, restored when the loan ends: the start
    /// of the loan this one is nested in, if any.
    outer_found_len: usize,
}

impl Readouts {
    /// No readouts, and no memory yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties it, keeping its memory. Lent to an `execute` through the
    /// protocol, it removes only the readouts that `execute` added, never
    /// those it found.
    pub fn clear(&mut self) {
        self.len = self.found_len;
    }

    /// Adds the next batch's readouts, with no rows yet, of `readouts`
    /// values each and room for `shots` rows, and returns them to fill
    /// with [`push_row`](BatchReadouts::push_row). They reuse the memory
    /// of readouts cleared earlier, if any.
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
    pub fn push(&mut self, batch: BatchReadouts) {
        if self.len == self.batches.len() {
            self.batches.push(batch);
        } else {
            self.batches[self.len] = batch;
        }
        self.len += 1;
    }

    /// Lends it to an `execute`, which appends after the readouts it holds
    /// now and, until the loan ends, cannot `clear` them.
    pub(crate) fn lend(&mut self) -> Loan {
        let loan = Loan {
            start: self.len,
            outer_found_len: self.found_len,
        };
        self.found_len = self.len;
        loan
    }

    /// The readouts appended during `loan`, or `None` when this is not the
    /// value lent: the `execute` assigned or swapped in another.
    pub(crate) fn appended(&self, loan: &Loan) -> Option<&[BatchReadouts]> {
        // `lend` sets `found_len`, and `end_loan` restores it only on the
        // value it lent, so another value holds 0, or the start of a loan of
        // its own.
        (self.found_len == loan.start).then(|| self.get(loan.start..).unwrap_or_default())
    }

    /// Ends `loan`, keeping the answer appended during it, or removing it.
    /// It restores the outer loan's start only on the value lent: one that
    /// replaced it keeps its own, so that the boundary above still sees the
    /// replacement, even if a part between answers anyway.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking the loan is what ends it once"
    )]
    pub(crate) fn end_loan(&mut self, loan: Loan, keep_answer: bool) {
        if !keep_answer {
            self.len = self.len.min(loan.start);
        }
        if self.found_len == loan.start {
            self.found_len = loan.outer_found_len;
        }
    }
}

impl Deref for Readouts {
    type Target = [BatchReadouts];

    fn deref(&self) -> &[BatchReadouts] {
        &self.batches[..self.len]
    }
}

impl<'a> IntoIterator for &'a Readouts {
    type Item = &'a BatchReadouts;
    type IntoIter = std::slice::Iter<'a, BatchReadouts>;

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
            found_len: 0,
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
