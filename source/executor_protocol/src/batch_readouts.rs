// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What comes back up for a batch: its [`BatchReadouts`], a row per shot,
//! in its shot set's order, of one [`ReadoutValue`] per readout slot, with
//! the shot's [`ShotConditions`]; and a slot's position in a row, a
//! [`ReadoutIndex`].
//!
//! A part that sends a batch must match each value it gets back to the
//! readout it asked for, on each shot. Rows by position make that an index:
//! the slots are each instruction's readouts in instruction order (its
//! outcomes, then its flags), so slot k is the same readout on every shot
//! of the batch; and a lost value is [`ReadoutValue::Missing`], never left
//! out. So every row has exactly R values, R being the width its maker
//! gave [`with_capacity`](BatchReadouts::with_capacity), and
//! [`push_row`](BatchReadouts::push_row) refuses any other width
//! ([`ShapeFault::RowWidth`]). That the readouts match their batch is not
//! checked here, since it needs the batch: the protocol checks it on every
//! answer that crosses a boundary of a chain, as [`ShapeFault`] says.
//!
//! The rows are dense: every value of the batch in one buffer, a byte or
//! two each, so a shot's row is a slice; and the shots' conditions are kept
//! only once a shot has any. So a batch's readouts, made with its number of
//! shots, take one allocation while every shot is clear, and at most two,
//! whatever that number. An `execute` appends them to the caller's
//! [`Readouts`], lent as an [`AppendOnlyReadouts`] view, which keeps their
//! memory for the next answer.

mod append_only_readouts;
mod readout_value;
mod readouts;
mod shot_conditions;

use std::error::Error;
use std::fmt;

pub use append_only_readouts::AppendOnlyReadouts;
pub use readout_value::{ReadoutFailure, ReadoutValue};
pub use readouts::Readouts;
pub use shot_conditions::ShotConditions;

/// One batch's readouts: a row per shot, in its shot set's order, of one
/// [`ReadoutValue`] per readout slot, and each shot's conditions. An
/// `execute` appends one per batch to the caller's [`Readouts`], in the
/// batches' order.
///
/// Every row has exactly R values, R being the width given to
/// [`with_capacity`](Self::with_capacity) (`push_row` refuses others);
/// slot k is the same readout on every shot; and a lost value is
/// [`ReadoutValue::Missing`]. The protocol checks them against their batch
/// as [`ShapeFault`] says. A shot is read by its position in the batch's
/// shot set, as `row(i)`.
#[derive(Clone)]
pub struct BatchReadouts {
    /// R, the number of values in each row.
    readouts: u32,
    /// How many rows there are.
    shots: usize,
    /// The rows it was made with room for: the room the conditions take
    /// when a shot first has any.
    room: usize,
    /// The rows, one after another: shot i's row is
    /// `values[i * R..(i + 1) * R]`.
    values: Vec<ReadoutValue>,
    /// Each shot's conditions, by its position; empty while every shot's
    /// conditions are `NONE`, so that readouts of clear shots take no
    /// allocation for them.
    conditions: Vec<ShotConditions>,
}

/// The same width, rows and conditions, whatever memory each keeps.
impl PartialEq for BatchReadouts {
    fn eq(&self, other: &Self) -> bool {
        self.readouts == other.readouts
            && self.shots == other.shots
            && self.values == other.values
            && (0..self.shots).all(|i| self.conditions(i) == other.conditions(i))
    }
}

impl Eq for BatchReadouts {}

// The small methods a part calls per batch or per shot, to build an answer
// or to read one, are `#[inline]`: parts live in other crates, where,
// without it, these methods are not inlined, and their calls cost more
// than their work.
impl BatchReadouts {
    /// No rows yet, of `readouts` values each, with room for `shots` rows.
    ///
    /// # Panics
    ///
    /// When the room for `shots` rows exceeds `isize::MAX` bytes, as
    /// [`Vec::with_capacity`] does; or, at the first shot with conditions
    /// ([`push_row`](Self::push_row)), when the room for `shots` shots'
    /// conditions does.
    #[must_use]
    #[inline]
    pub fn with_capacity(shots: usize, readouts: u32) -> Self {
        Self {
            readouts,
            shots: 0,
            room: shots,
            values: Vec::with_capacity(shots.saturating_mul(readouts as usize)),
            conditions: Vec::new(),
        }
    }

    /// Empties it for rows of `readouts` values, with room for `shots`
    /// rows, keeping its memory: as [`with_capacity`](Self::with_capacity),
    /// with no allocation where its memory has the room.
    #[inline]
    pub(crate) fn reset(&mut self, shots: usize, readouts: u32) {
        self.readouts = readouts;
        self.shots = 0;
        self.room = shots;
        self.values.clear();
        self.values.reserve(shots.saturating_mul(readouts as usize));
        self.conditions.clear();
    }

    /// Records the next shot's `conditions`, its values already added: the
    /// conditions are kept from the first shot that has any, with room for
    /// every row.
    #[inline]
    fn record_shot(&mut self, conditions: ShotConditions) {
        if self.conditions.is_empty() {
            if conditions.is_empty() {
                self.shots += 1;
                return;
            }
            self.conditions.reserve(self.room.max(self.shots + 1));
            self.conditions.resize(self.shots, ShotConditions::NONE);
        }
        self.conditions.push(conditions);
        self.shots += 1;
    }

    /// Adds the next shot's row, `values`, with its `conditions`.
    ///
    /// # Errors
    ///
    /// [`ShapeFault::RowWidth`] when `values` does not hold exactly
    /// [`readouts`](Self::readouts) values; nothing is added then.
    ///
    /// # Panics
    ///
    /// At the first shot with conditions, when the room for the conditions
    /// of the shots given to [`with_capacity`](Self::with_capacity) exceeds
    /// `isize::MAX` bytes.
    #[inline]
    pub fn push_row(
        &mut self,
        values: &[ReadoutValue],
        conditions: ShotConditions,
    ) -> Result<(), ShapeFault> {
        if values.len() != self.readouts as usize {
            return Err(ShapeFault::RowWidth {
                expected: self.readouts,
                got: values.len(),
            });
        }
        self.values.extend_from_slice(values);
        self.record_shot(conditions);
        Ok(())
    }

    /// R, the number of values in each row, as given to
    /// [`with_capacity`](Self::with_capacity).
    #[must_use]
    #[inline]
    pub fn readouts(&self) -> u32 {
        self.readouts
    }

    /// How many rows it holds: one per shot, once complete.
    #[must_use]
    #[inline]
    pub fn shots(&self) -> usize {
        self.shots
    }

    /// The row of the shot at `position` in the batch's shot set: its R
    /// values, by slot.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn row(&self, position: usize) -> &[ReadoutValue] {
        self.check_position(position);
        let width = self.readouts as usize;
        &self.values[position * width..(position + 1) * width]
    }

    /// The value in readout slot `readout` of the row of the shot at
    /// `position` in the batch's shot set.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots), or `readout` not
    /// below R, where it would read another shot's value.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn value(&self, position: usize, readout: ReadoutIndex) -> ReadoutValue {
        assert!(
            readout.get() < self.readouts,
            "slot {} is beyond the rows' {} readout slots",
            readout.get(),
            self.readouts
        );
        self.row(position)[readout.get() as usize]
    }

    /// The conditions of the shot at `position` in the batch's shot set.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn conditions(&self, position: usize) -> ShotConditions {
        self.check_position(position);
        self.conditions
            .get(position)
            .copied()
            .unwrap_or(ShotConditions::NONE)
    }

    /// Joins the readouts of one batch's instructions sent below in parts,
    /// as consecutive `execute`s on the same shots, such as a stepping
    /// debugger sends them: each shot's row is its rows in the parts, in
    /// order, and its conditions their union.
    ///
    /// # Panics
    ///
    /// - When `parts` is empty: a batch has at least one part.
    /// - When its parts hold different numbers of rows: the parts must
    ///   answer the same shots.
    /// - When their slots add up beyond `u32::MAX`: the joined width, R, is
    ///   a `u32`.
    #[must_use]
    #[track_caller]
    pub fn concat_readouts(parts: &[BatchReadouts]) -> Self {
        let Some(first) = parts.first() else {
            panic!("no readouts to join: a batch's readouts need at least one part");
        };
        let shots = first.shots();
        if let Some(other) = parts.iter().find(|part| part.shots() != shots) {
            panic!(
                "readouts of {} and of {} shots cannot be joined: the parts must answer the same shots",
                shots,
                other.shots()
            );
        }
        let readouts = parts
            .iter()
            .try_fold(0u32, |sum, part| sum.checked_add(part.readouts))
            .expect("the parts' readout slots add up to at most u32::MAX");
        let mut joined = Self::with_capacity(shots, readouts);
        for position in 0..shots {
            let mut conditions = ShotConditions::NONE;
            for part in parts {
                joined.values.extend_from_slice(part.row(position));
                conditions |= part.conditions(position);
            }
            joined.record_shot(conditions);
        }
        joined
    }

    /// Panics, naming the rows, when `position` is not a row's.
    #[track_caller]
    #[inline]
    fn check_position(&self, position: usize) {
        assert!(
            position < self.shots(),
            "shot position {position} is beyond the readouts' {} rows",
            self.shots()
        );
    }
}

/// The rows, each its values and its shot's conditions, so a failing test
/// shows them by shot: `BatchReadouts { readouts: 1, rows: [([One],
/// ShotConditions(NONE))] }`.
#[expect(
    clippy::missing_fields_in_debug,
    reason = "the rows show `values` and `conditions` by shot"
)]
impl fmt::Debug for BatchReadouts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows =
            (0..self.shots()).map(|position| (self.row(position), self.conditions(position)));
        f.debug_struct("BatchReadouts")
            .field("readouts", &self.readouts)
            .field("rows", &DebugRows(rows))
            .finish()
    }
}

/// Lists the rows for `BatchReadouts`' `Debug`.
struct DebugRows<I>(I);

impl<'a, I> fmt::Debug for DebugRows<I>
where
    I: Iterator<Item = (&'a [ReadoutValue], ShotConditions)> + Clone,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.clone()).finish()
    }
}

/// A readout slot's position in a batch's rows, k: slot k is the same
/// readout on every shot of the batch. It must be below R, the rows' width,
/// which [`BatchReadouts::value`] checks; R is the batch's readout slots
/// on every answer to a [`checked`](crate::Batch::checked) batch that
/// crossed a boundary of a chain, since the protocol checks it there.
///
/// A `u32`, at no cost over one, so that a slot is not mistaken for a
/// shot's position or a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ReadoutIndex(u32);

impl ReadoutIndex {
    /// Slot `k`.
    #[must_use]
    pub const fn new(k: u32) -> Self {
        Self(k)
    }

    /// The slot's position, k.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Why readouts do not have their expected shape: a row pushed with
/// another width than the one given to [`BatchReadouts::with_capacity`]
/// (`RowWidth`, from [`BatchReadouts::push_row`]); or readouts that do
/// not match the batches sent, found by the protocol's check of the
/// answer.
///
/// The protocol checks every answer that crosses a boundary of a chain,
/// and makes a failure of it
/// ([`ExecuteError::Failed`](crate::ExecuteError::Failed), carrying the
/// fault as [`Failure::Part`](crate::Failure::Part)) for readouts not one
/// per batch sent (`Batches`, named by the first batch's first shot), for
/// a batch's rows not one per shot of it (`Rows`, named by that batch's
/// first shot), or, for a batch its sender marked
/// [`checked`](crate::Batch::checked), for rows whose width is not the
/// batch's readout slots, the sum of its instructions' outcomes and flags
/// (`Slots`, named by that batch's first shot): that sum costs a pass over
/// the instructions, which an unchecked batch is spared. So the part above
/// finds a row for every shot, and, for a checked batch, every slot it
/// asked for at its index, after the readouts it had, which the
/// [`AppendOnlyReadouts`] it lent keep unchanged.
// The one statement of the protocol's check of an answer's shape: the
// other docs link here rather than restate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShapeFault {
    /// A row of `got` values, where the readouts' rows have `expected`.
    RowWidth {
        /// R, the width of the readouts' rows.
        expected: u32,
        /// The number of values the row held.
        got: usize,
    },
    /// An `execute` appended `got` batches' readouts to the caller's
    /// [`Readouts`], where it was sent `expected` batches.
    Batches {
        /// The batches sent.
        expected: usize,
        /// The readouts appended.
        got: usize,
    },
    /// The readouts of `batches[batch]` hold `got` rows, where the batch
    /// has `expected` shots.
    Rows {
        /// The batch's position in the `execute`.
        batch: usize,
        /// The batch's shots.
        expected: usize,
        /// The rows answered.
        got: usize,
    },
    /// The readouts of `batches[batch]`, a batch its sender marked
    /// [`checked`](crate::Batch::checked), have rows of `got` values, where
    /// the batch's instructions have `expected` readout slots.
    Slots {
        /// The batch's position in the `execute`.
        batch: usize,
        /// The readout slots of the batch's instructions.
        expected: u32,
        /// The width of the rows answered.
        got: u32,
    },
}

impl fmt::Display for ShapeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RowWidth { expected, got } => write!(
                f,
                "a row of {got} readout values, where each row has {expected}"
            ),
            Self::Batches { expected, got } => {
                write!(f, "readouts for {got} batches, where {expected} were sent")
            }
            Self::Rows {
                batch,
                expected,
                got,
            } => write!(
                f,
                "{got} rows for batch {batch}, which has {expected} shots"
            ),
            Self::Slots {
                batch,
                expected,
                got,
            } => write!(
                f,
                "rows of {got} readout values for batch {batch}, whose instructions have {expected} readout slots"
            ),
        }
    }
}

impl Error for ShapeFault {}
