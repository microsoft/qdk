// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Operand storage.
//!
//! Instructions reference operands by handle rather than owning a list, and
//! equal lists share one handle. A million two-qubit gates over the same pair
//! therefore store one operand list, not a million.

use std::hash::Hash;

use rustc_hash::FxHashMap;

/// A qubit, addressed by dense index within the circuit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Qubit(pub u32);

/// A classical bit, addressed by dense index within the circuit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Clbit(pub u32);

/// A handle to a stored operand list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperandsId(pub u32);

/// The handle for the empty operand list, which every table defines.
pub const EMPTY_OPERANDS: OperandsId = OperandsId(0);

/// Stores operand lists behind stable handles.
#[derive(Clone, Debug)]
pub struct OperandTable<T> {
    /// Flattened operand values for every stored list.
    values: Vec<T>,
    /// Start offset of each list in `values`, with a trailing end sentinel.
    offsets: Vec<u32>,
    /// Maps a list to its handle. Holds one entry per distinct pattern.
    index: FxHashMap<Box<[T]>, OperandsId>,
}

impl<T: Copy + Eq + Hash> Default for OperandTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Eq + Hash> OperandTable<T> {
    /// Creates a table containing only the empty list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            offsets: vec![0, 0],
            index: FxHashMap::default(),
        }
    }

    /// Number of distinct lists stored, including the empty list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// Whether the table holds only the empty list.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }

    /// Stores an operand list and returns its handle, reusing the handle of an
    /// equal list if one is already stored.
    pub fn insert(&mut self, operands: &[T]) -> OperandsId {
        if operands.is_empty() {
            return EMPTY_OPERANDS;
        }
        if let Some(existing) = self.index.get(operands) {
            return *existing;
        }
        let id = self.push(operands);
        self.index.insert(operands.into(), id);
        id
    }

    /// Appends a list without checking for an existing equal one.
    fn push(&mut self, operands: &[T]) -> OperandsId {
        let id = u32::try_from(self.len()).expect("operand table exceeded u32 range");
        self.values.extend_from_slice(operands);
        let end = u32::try_from(self.values.len()).expect("operand table exceeded u32 range");
        self.offsets.push(end);
        OperandsId(id)
    }

    /// Returns the operand list for a handle, or `None` if the handle is unknown.
    #[must_use]
    pub fn get(&self, id: OperandsId) -> Option<&[T]> {
        let index = id.0 as usize;
        let start = *self.offsets.get(index)? as usize;
        let end = *self.offsets.get(index + 1)? as usize;
        self.values.get(start..end)
    }

    /// Reserves room for `lists` additional distinct lists holding `values`
    /// operands in total.
    pub fn reserve(&mut self, lists: usize, values: usize) {
        self.offsets.reserve(lists);
        self.values.reserve(values);
        self.index.reserve(lists);
    }
}
