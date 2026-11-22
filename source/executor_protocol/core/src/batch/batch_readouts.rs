// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What comes back up for a batch: its [`BatchReadouts`], a row per shot,
//! in its shot set's order, of one value per readout slot, with the
//! shot's conditions, both of the set's types; a slot's position in a row,
//! a [`ReadoutSlot`]; and the buffer a caller keeps them in, [`Readouts`].
//!
//! A part that sends a batch must match each value it gets back to the
//! readout it asked for, on each shot. Rows by position make that an index:
//! the slots are each instruction's readouts in instruction order, so slot
//! k is the same readout on every shot of the batch; and a value a part
//! could not read (a lost qubit's, in a set that has them) is still a
//! value of the set, never left out. So the protocol shapes each batch's
//! readouts before the receiver's `execute` runs, from the batch: a row
//! per shot, of a value per readout slot of its instructions, as its
//! sender stated them, each value the set's `UNWRITTEN_READOUT` and each
//! shot clear of conditions. The executor fills them in place, with
//! methods that keep their shape ([`row_mut`](BatchReadouts::row_mut),
//! [`set_conditions`](BatchReadouts::set_conditions)), and the part above
//! indexes them with no check of its own.
//!
//! The rows are dense: every value of the batch in one buffer, one
//! `S::Readout` each, so a shot's row is a slice; and the shots' conditions
//! are kept only once a shot has any. Shaping keeps the memory of the
//! readouts it shapes, so a caller that keeps its [`Readouts`] from one
//! `execute` to the next allocates nothing once they have held readouts as
//! large.
//!
//! The crate's `docs/batches-and-readouts.md` gives the batches and their
//! readouts as a part that sends or answers them sees them.

mod readout_slot;
mod readouts;

use std::fmt;

pub use readout_slot::{ReadoutSlot, ReadoutSlots};
pub use readouts::Readouts;

use crate::InstructionSet;

/// One batch's readouts: a row per shot, in its shot set's order, of one
/// [`S::Readout`](InstructionSet::Readout) per readout slot, and each
/// shot's [`S::ShotConditions`](InstructionSet::ShotConditions). An
/// `execute` fills one per batch, shaped by the protocol from the batch,
/// as [`Executor::execute`](crate::Executor::execute) says.
///
/// Every row has exactly one value per readout slot it was shaped with;
/// slot k is the same readout on every shot; and a value a part could not
/// read (a lost qubit's, in a set that has them) is still a value of the
/// set, never left out. A value no part wrote is
/// [`S::UNWRITTEN_READOUT`](InstructionSet::UNWRITTEN_READOUT). A shot is
/// read by its position in the batch's shot set, as `row(i)`, and written
/// as `row_mut(i)`.
#[derive(Clone)]
pub struct BatchReadouts<S: InstructionSet> {
    /// The number of values in each row: its readout slots.
    readout_slots: ReadoutSlots,
    /// How many rows there are.
    shots: usize,
    /// The rows, one after another: shot i's row is
    /// `values[i * readout_slots..(i + 1) * readout_slots]`.
    values: Vec<S::Readout>,
    /// Each shot's conditions, by its position; empty while every shot's
    /// conditions are the default, so that readouts of clear shots take no
    /// allocation for them.
    conditions: Vec<S::ShotConditions>,
}

/// The same readout slots, rows and conditions, whatever memory each keeps.
impl<S: InstructionSet> PartialEq for BatchReadouts<S> {
    fn eq(&self, other: &Self) -> bool {
        self.readout_slots == other.readout_slots
            && self.shots == other.shots
            && self.values == other.values
            && (0..self.shots).all(|i| self.conditions(i) == other.conditions(i))
    }
}

impl<S: InstructionSet> Eq for BatchReadouts<S>
where
    S::Readout: Eq,
    S::ShotConditions: Eq,
{
}

// The small methods a part calls per batch or per shot, to fill readouts
// or to read them, are `#[inline]`: parts live in other crates, where,
// without it, these methods are not inlined, and their calls cost more
// than their work.
impl<S: InstructionSet> BatchReadouts<S> {
    /// A row of `readout_slots` values for each of `shots` shots, each
    /// value the set's `UNWRITTEN_READOUT` and each shot clear: the shape
    /// the protocol gives an executor for a batch of `shots` shots whose
    /// instructions have `readout_slots` readout slots, here for a part that
    /// builds readouts itself, such as a test's expected readouts, or the
    /// readouts it passes to an executor's `execute` called outside a
    /// chain, where no boundary shapes them.
    ///
    /// # Panics
    ///
    /// When `shots` rows of `readout_slots` values are more than a `Vec`
    /// holds.
    #[must_use]
    #[inline]
    pub fn new(shots: usize, readout_slots: ReadoutSlots) -> Self {
        let mut new = Self {
            readout_slots: 0,
            shots: 0,
            values: Vec::new(),
            conditions: Vec::new(),
        };
        new.shape(shots, readout_slots);
        new
    }

    /// Shapes it as [`new`](Self::new) does, keeping its memory: no
    /// allocation where its memory has the room.
    #[inline]
    pub(crate) fn shape(&mut self, shots: usize, readout_slots: ReadoutSlots) {
        let values = shots
            .checked_mul(readout_slots as usize)
            .unwrap_or_else(|| {
                panic!(
                    "{shots} shots of {readout_slots} readout slots are more values \
                     than a `Vec` holds"
                )
            });
        self.readout_slots = readout_slots;
        self.shots = shots;
        self.values.clear();
        self.values.resize(values, S::UNWRITTEN_READOUT);
        self.conditions.clear();
    }

    /// The number of values in each row: the readout slots of the batch's
    /// instructions, as its sender stated them
    /// ([`Batch::readout_slots`](crate::Batch::readout_slots)).
    #[must_use]
    #[inline]
    pub fn readout_slots(&self) -> ReadoutSlots {
        self.readout_slots
    }

    /// How many rows it holds: one per shot of the batch.
    #[must_use]
    #[inline]
    pub fn shots(&self) -> usize {
        self.shots
    }

    /// The row of the shot at `position` in the batch's shot set: its value
    /// in each readout slot, by slot.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn row(&self, position: usize) -> &[S::Readout] {
        self.check_position(position);
        let slots = self.readout_slots as usize;
        &self.values[position * slots..(position + 1) * slots]
    }

    /// The row of the shot at `position` in the batch's shot set, to fill:
    /// its value in each readout slot, by slot, each the set's
    /// `UNWRITTEN_READOUT` until written.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn row_mut(&mut self, position: usize) -> &mut [S::Readout] {
        self.check_position(position);
        let slots = self.readout_slots as usize;
        &mut self.values[position * slots..(position + 1) * slots]
    }

    /// The value in readout slot `slot` of the row of the shot at `position`
    /// in the batch's shot set.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots), or `slot` not
    /// below [`readout_slots`](Self::readout_slots), where it would read
    /// another shot's value.
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn value(&self, position: usize, slot: ReadoutSlot) -> S::Readout {
        assert!(
            slot.get() < self.readout_slots,
            "slot {} is beyond the rows' {} readout slots",
            slot.get(),
            self.readout_slots
        );
        self.row(position)[slot.get() as usize]
    }

    /// The conditions of the shot at `position` in the batch's shot set.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[must_use]
    #[track_caller]
    #[inline]
    pub fn conditions(&self, position: usize) -> S::ShotConditions {
        self.check_position(position);
        self.conditions.get(position).copied().unwrap_or_default()
    }

    /// Sets the conditions of the shot at `position` in the batch's shot
    /// set, which are the default until set. The first
    /// shot given any takes room for every shot's.
    ///
    /// # Panics
    ///
    /// When `position` is not below [`shots`](Self::shots).
    #[track_caller]
    #[inline]
    pub fn set_conditions(&mut self, position: usize, conditions: S::ShotConditions) {
        self.check_position(position);
        if self.conditions.is_empty() {
            if conditions == S::ShotConditions::default() {
                return;
            }
            self.conditions
                .resize(self.shots, S::ShotConditions::default());
        }
        self.conditions[position] = conditions;
    }

    /// Fills these readouts with `parts` joined, the readouts of one
    /// batch's instructions sent below in parts, as consecutive `execute`s
    /// on the same shots, such as a stepping debugger sends them: each
    /// shot's row is its rows in the parts, in order, and its conditions
    /// theirs joined (`|=`).
    ///
    /// # Panics
    ///
    /// When a part holds another number of rows than these readouts (the
    /// parts must answer the same shots), or when the parts' readout slots
    /// do not add up to these readouts' (they must answer all of the
    /// batch's instructions).
    #[track_caller]
    pub fn concat_from(&mut self, parts: &[Self]) {
        if let Some(other) = parts.iter().find(|part| part.shots() != self.shots) {
            panic!(
                "readouts of {} shot{} cannot fill readouts of {}: the parts must answer the same shots",
                other.shots(),
                if other.shots() == 1 { "" } else { "s" },
                self.shots
            );
        }
        let slots: u64 = parts.iter().map(|part| u64::from(part.readout_slots)).sum();
        assert!(
            slots == u64::from(self.readout_slots),
            "parts of {slots} readout slots in all cannot fill rows of {}: they must answer all of the batch's instructions",
            self.readout_slots
        );
        for position in 0..self.shots {
            let mut conditions = S::ShotConditions::default();
            let mut from = position * self.readout_slots as usize;
            for part in parts {
                let row = part.row(position);
                self.values[from..from + row.len()].copy_from_slice(row);
                from += row.len();
                conditions |= part.conditions(position);
            }
            self.set_conditions(position, conditions);
        }
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
/// shows them by shot, with the set's `Debug` of each: for instance, for a
/// set whose readouts print `One` and whose clear conditions print `NONE`,
/// `BatchReadouts { readout_slots: 1, rows: [([One], NONE)] }`.
#[expect(
    clippy::missing_fields_in_debug,
    reason = "the rows show `values` and `conditions` by shot"
)]
impl<S: InstructionSet> fmt::Debug for BatchReadouts<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows =
            (0..self.shots()).map(|position| (self.row(position), self.conditions(position)));
        f.debug_struct("BatchReadouts")
            .field("readout_slots", &self.readout_slots)
            .field("rows", &DebugRows(rows))
            .finish()
    }
}

/// Lists the rows for `BatchReadouts`' `Debug`.
struct DebugRows<I>(I);

impl<'a, I, R, C> fmt::Debug for DebugRows<I>
where
    I: Iterator<Item = (&'a [R], C)> + Clone,
    R: fmt::Debug + 'a,
    C: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.clone()).finish()
    }
}
