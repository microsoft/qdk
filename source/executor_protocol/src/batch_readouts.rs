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
//! ([`ShapeFault`]). That R is the batch's number of readout slots is not
//! checked here: it is for the composed executor to check, since it holds
//! both the batch and its readouts, and only that check makes sure no value
//! is missing, repeated or unexpected.
//!
//! The rows are dense: every value of the batch in one buffer, a byte or
//! two each, so a shot's row is a slice, and a batch's readouts, made with
//! its number of shots, take at most two allocations, whatever that number.

mod readout_value;
mod shot_conditions;

use std::error::Error;
use std::fmt;

pub use readout_value::{ReadoutFailure, ReadoutValue};
pub use shot_conditions::ShotConditions;

/// One batch's readouts: a row per shot, in its shot set's order, of one
/// [`ReadoutValue`] per readout slot, and each shot's conditions. An
/// `execute` returns one per batch, in the batches' order.
///
/// Every row has exactly R values, R being the width given to
/// [`with_capacity`](Self::with_capacity) (`push_row` refuses others);
/// that R is the batch's number of readout slots is for the composed
/// executor to check; slot k is the same readout on every shot; and a lost
/// value is [`ReadoutValue::Missing`]. A shot is read by its position in
/// the batch's shot set, as `row(i)`.
#[derive(Clone, PartialEq, Eq)]
pub struct BatchReadouts {
    /// R, the number of values in each row.
    readouts: u32,
    /// The rows, one after another: shot i's row is
    /// `values[i * R..(i + 1) * R]`.
    values: Vec<ReadoutValue>,
    /// Each shot's conditions, by its position: also how many rows there
    /// are, since a row of no slots holds no value.
    conditions: Vec<ShotConditions>,
}

impl BatchReadouts {
    /// No rows yet, of `readouts` values each, with room for `shots` rows.
    ///
    /// # Panics
    ///
    /// When the room for `shots` rows exceeds `isize::MAX` bytes, as
    /// [`Vec::with_capacity`] does.
    #[must_use]
    pub fn with_capacity(shots: usize, readouts: u32) -> Self {
        Self {
            readouts,
            values: Vec::with_capacity(shots.saturating_mul(readouts as usize)),
            conditions: Vec::with_capacity(shots),
        }
    }

    /// Adds the next shot's row, `values`, with its `conditions`.
    ///
    /// # Errors
    ///
    /// [`ShapeFault::RowWidth`] when `values` does not hold exactly
    /// [`readouts`](Self::readouts) values; nothing is added then.
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
        self.conditions.push(conditions);
        Ok(())
    }

    /// R, the number of values in each row, as given to
    /// [`with_capacity`](Self::with_capacity).
    #[must_use]
    pub fn readouts(&self) -> u32 {
        self.readouts
    }

    /// How many rows it holds: one per shot, once complete.
    #[must_use]
    pub fn shots(&self) -> usize {
        self.conditions.len()
    }

    /// The row of the shot at `position` in the batch's shot set: its R
    /// values, by slot.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
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
    pub fn conditions(&self, position: usize) -> ShotConditions {
        self.check_position(position);
        self.conditions[position]
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
            joined.conditions.push(conditions);
        }
        joined
    }

    /// Panics, naming the rows, when `position` is not a row's.
    #[track_caller]
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
            (0..self.shots()).map(|position| (self.row(position), self.conditions[position]));
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
/// which [`BatchReadouts::value`] checks.
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

/// Why a row does not have the readouts' width, the one given to
/// [`BatchReadouts::with_capacity`].
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
}

impl fmt::Display for ShapeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RowWidth { expected, got } => write!(
                f,
                "a row of {got} readout values, where each row has {expected}"
            ),
        }
    }
}

impl Error for ShapeFault {}
