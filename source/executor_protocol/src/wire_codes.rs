// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fixed codes of rejections, retry classes, needs, values and shot
//! conditions.
//!
//! A code is how two ends of a link name a reason, a class, a need, a value
//! or a set of conditions, so once an end is deployed a code keeps its
//! meaning, and a new meaning gets a new code, never one used before. Every
//! code is written out here, never derived from an enum's order or a type's
//! representation, so changing a type cannot change what crosses a link. No
//! link exists yet (L1 and L0 are planned, for an executor across a network:
//! a device behind a cloud service, a simulator on another machine): the
//! codes are fixed now so that the first deployed ends agree.
//! The `_of` functions decode a code and return `None` for one they do not
//! know; there is no decoder of reason codes yet (planned, for a far end
//! deployed at a newer version than this end), and one should map an unknown
//! reason to [`Rejected::Unrecognized`], keeping the far end's retry class and
//! text.

use crate::{Need, ReadoutFailure, Rejected, RetryClass, ShotConditions, Value};

/// The code of `rejected`'s reason. [`Rejected::Unrecognized`] has 255: it is
/// relayed as it came, with no reason of this end.
#[must_use]
pub fn reason_code(rejected: &Rejected) -> u8 {
    match rejected {
        Rejected::NoShots => 0,
        Rejected::ShotNotHeld(_) => 1,
        Rejected::RepeatedShot(_) => 2,
        Rejected::IdReused(_) => 3,
        Rejected::Instruction { .. } => 4,
        Rejected::Closed => 5,
        Rejected::Unavailable(_) => 6,
        Rejected::Malformed(_) => 7,
        Rejected::Unmet(_) => 8,
        Rejected::EarlierFailure(_) => 9,
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
        Need::ResumableShots => 1,
        Need::Resets => 2,
        Need::Shots => 3,
        Need::Replayable => 4,
        Need::LogProbability => 5,
        Need::InstructionSet => 6,
    }
}

/// The need of `code`, the inverse of [`need_code`]; `None` for a code this
/// end does not know, which the planned decoder of reasons turns into
/// [`Rejected::Unrecognized`].
#[must_use]
pub fn need_of(code: u8) -> Option<Need> {
    Some(match code {
        0 => Need::Qubits,
        1 => Need::ResumableShots,
        2 => Need::Resets,
        3 => Need::Shots,
        4 => Need::Replayable,
        5 => Need::LogProbability,
        6 => Need::InstructionSet,
        _ => return None,
    })
}

/// The code of a readout's value.
#[must_use]
pub fn value_code(value: Value) -> u8 {
    match value {
        Value::Zero => 0,
        Value::One => 1,
        Value::Missing(ReadoutFailure::Loss) => 2,
        Value::Missing(ReadoutFailure::Other) => 3,
    }
}

/// The value of `code`, the inverse of [`value_code`]; `None` for a code this
/// end does not know.
#[must_use]
pub fn value_of(code: u8) -> Option<Value> {
    Some(match code {
        0 => Value::Zero,
        1 => Value::One,
        2 => Value::Missing(ReadoutFailure::Loss),
        3 => Value::Missing(ReadoutFailure::Other),
        _ => return None,
    })
}

/// The code of a shot's conditions, a bit for each: `1` rejected, `2`
/// inconsistent, `4` unresolved; `0` for none.
#[must_use]
pub fn conditions_code(conditions: ShotConditions) -> u8 {
    u8::from(conditions.contains(ShotConditions::REJECTED))
        | u8::from(conditions.contains(ShotConditions::INCONSISTENT)) << 1
        | u8::from(conditions.contains(ShotConditions::UNRESOLVED)) << 2
}

/// The conditions of `code`, the inverse of [`conditions_code`]; `None` for a
/// code with a bit this end does not know.
#[must_use]
pub fn conditions_of(code: u8) -> Option<ShotConditions> {
    if code & !0b111 != 0 {
        return None;
    }
    let of = |bit: u8, condition| {
        if code & bit == 0 {
            ShotConditions::NONE
        } else {
            condition
        }
    };
    Some(
        of(1, ShotConditions::REJECTED)
            | of(2, ShotConditions::INCONSISTENT)
            | of(4, ShotConditions::UNRESOLVED),
    )
}
