// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One readout's value: [`ReadoutValue`], `Zero`, `One`, or `Missing` with
//! the reason, a [`ReadoutFailure`].
//!
//! A measurement can lose its value without ending the shot: an atom leaves
//! its trap, and the shot runs on. The parts above must see that the value is
//! lost rather than read a bit made up for it, so a lost value travels as
//! `Missing`, in its own slot of the shot's row, and the shot's other values
//! stand; the driver turns it into its own records.

/// One readout's value, in its slot of a shot's row.
///
/// A byte or two, so a batch's rows stay dense. Non-exhaustive, so that another
/// kind of value can be added without breaking a part that matches on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadoutValue {
    /// The readout found its qubit in |0⟩, or its flag not raised.
    Zero,
    /// The readout found its qubit in |1⟩, or its flag raised.
    One,
    /// The part below lost the value, for the reason given; the shot runs
    /// on, and its other values stand.
    Missing(ReadoutFailure),
}

// A byte or two, as its doc promises.
const _: () = assert!(size_of::<ReadoutValue>() <= 2);

/// A measured bit as a value: `true` is [`ReadoutValue::One`], `false`
/// [`ReadoutValue::Zero`].
impl From<bool> for ReadoutValue {
    fn from(one: bool) -> Self {
        if one { Self::One } else { Self::Zero }
    }
}

/// Why a readout has no value.
///
/// Non-exhaustive, so that a reason can be added without breaking a part
/// that matches on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadoutFailure {
    /// The qubit was lost, such as an atom that left its trap.
    Loss,
    /// Any other reason, with no detail.
    Other,
}
