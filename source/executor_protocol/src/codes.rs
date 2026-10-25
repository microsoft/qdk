// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fixed codes of rejections, retry classes, needs and values.
//!
//! A code is how two ends of a link name a reason, a class, a need or a value,
//! so once an end is deployed a code keeps its meaning, and a new meaning gets
//! a new code. Every code is written out here, never derived from an enum's
//! order, so reordering a type cannot change what crosses a link. An end that
//! reads a reason or a need code it does not know decodes it as
//! [`Rejected::Unrecognized`], keeping the far end's retry class and text.

use crate::{ErrorKind, Need, Rejected, RetryClass, Value};

/// The code of `rejected`'s reason. [`Rejected::Unrecognized`] has 255: it is
/// relayed as it came, with no reason of this end.
#[must_use]
pub fn reason_code(rejected: &Rejected) -> u8 {
    match rejected {
        Rejected::NoShots => 0,
        Rejected::ShotNotInSession(_) => 1,
        Rejected::RepeatedShot(_) => 2,
        Rejected::IdReused(_) => 3,
        Rejected::Instruction { .. } => 4,
        Rejected::Closed => 5,
        Rejected::Unavailable(_) => 6,
        Rejected::Malformed(_) => 7,
        Rejected::Unmet(_) => 8,
        Rejected::SessionFailed(_) => 9,
        Rejected::NoReply(_) => 10,
        Rejected::Unrecognized { .. } => 255,
    }
}

/// The code of a retry class.
#[must_use]
pub fn retry_code(retry: RetryClass) -> u8 {
    match retry {
        RetryClass::Never => 0,
        RetryClass::Later => 1,
        RetryClass::Resend => 2,
    }
}

/// The retry class of `code`, the inverse of [`retry_code`]; `None` for a code
/// this end does not know.
#[must_use]
pub fn retry_of(code: u8) -> Option<RetryClass> {
    Some(match code {
        0 => RetryClass::Never,
        1 => RetryClass::Later,
        2 => RetryClass::Resend,
        _ => return None,
    })
}

/// The code of a need.
#[must_use]
pub fn need_code(need: Need) -> u8 {
    match need {
        Need::Qubits => 0,
        Need::Feedback => 1,
        Need::Resets => 2,
        Need::States => 3,
        Need::Replayable => 4,
        Need::Probabilities => 5,
    }
}

/// The need of `code`, the inverse of [`need_code`]; `None` for a code this
/// end does not know, which a decoder turns into
/// [`Rejected::Unrecognized`].
#[must_use]
pub fn need_of(code: u8) -> Option<Need> {
    Some(match code {
        0 => Need::Qubits,
        1 => Need::Feedback,
        2 => Need::Resets,
        3 => Need::States,
        4 => Need::Replayable,
        5 => Need::Probabilities,
        _ => return None,
    })
}

/// The code of a readout's value.
#[must_use]
pub fn value_code(value: Value) -> u8 {
    match value {
        Value::Zero => 0,
        Value::One => 1,
        Value::Error(ErrorKind::Loss) => 2,
        Value::Error(ErrorKind::Other) => 3,
    }
}

/// The value of `code`, the inverse of [`value_code`]; `None` for a code this
/// end does not know.
#[must_use]
pub fn value_of(code: u8) -> Option<Value> {
    Some(match code {
        0 => Value::Zero,
        1 => Value::One,
        2 => Value::Error(ErrorKind::Loss),
        3 => Value::Error(ErrorKind::Other),
        _ => return None,
    })
}
