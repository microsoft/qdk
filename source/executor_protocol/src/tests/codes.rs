// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ErrorKind, Need, Rejected, RetryClass, UnmetNeed, Value, need_code, need_of, reason_code,
    retry_code, retry_of, value_code, value_of,
};

/// Each reason has a fixed code, so a code keeps its meaning between two
/// ends built at different times.
#[test]
fn reason_codes_are_fixed() {
    let unmet = UnmetNeed {
        component: String::new(),
        need: Need::Feedback,
        needed: 1,
        offered: 0,
        mismatches: Vec::new(),
    };
    let reasons = [
        (Rejected::NoShots, 0),
        (Rejected::ShotNotHeld(1), 1),
        (Rejected::RepeatedShot(1), 2),
        (Rejected::IdReused(1), 3),
        (
            Rejected::Instruction {
                at: 0,
                reason: String::new(),
            },
            4,
        ),
        (Rejected::Closed, 5),
        (Rejected::Unavailable(String::new()), 6),
        (Rejected::Malformed(String::new()), 7),
        (Rejected::Unmet(unmet), 8),
        (Rejected::EarlierFailure(String::new()), 9),
        (Rejected::NoReply(String::new()), 10),
        (
            Rejected::Unrecognized {
                retry: RetryClass::Never,
                reason: String::new(),
            },
            255,
        ),
    ];
    for (rejected, code) in reasons {
        assert_eq!(reason_code(&rejected), code, "{rejected:?}");
    }
}

/// Each need has a fixed code, which decodes back to it.
#[test]
fn need_codes_are_fixed_and_decode_back() {
    let needs = [
        (Need::Qubits, 0),
        (Need::Feedback, 1),
        (Need::Resets, 2),
        (Need::States, 3),
        (Need::Replayable, 4),
        (Need::Probabilities, 5),
        (Need::Instructions, 6),
    ];
    for (need, code) in needs {
        assert_eq!(need_code(need), code);
        assert_eq!(need_of(code), Some(need));
    }
}

/// Each retry class has a fixed code, which decodes back to it.
#[test]
fn retry_codes_are_fixed_and_decode_back() {
    for (retry, code) in [
        (RetryClass::Never, 0),
        (RetryClass::Later, 1),
        (RetryClass::Resend, 2),
    ] {
        assert_eq!(retry_code(retry), code);
        assert_eq!(retry_of(code), Some(retry));
    }
}

/// Each value has a fixed code, which decodes back to it.
#[test]
fn value_codes_are_fixed_and_decode_back() {
    let values = [
        (Value::Zero, 0),
        (Value::One, 1),
        (Value::Error(ErrorKind::Loss), 2),
        (Value::Error(ErrorKind::Other), 3),
    ];
    for (value, code) in values {
        assert_eq!(value_code(value), code);
        assert_eq!(value_of(code), Some(value));
    }
}

/// A code this end does not know decodes to nothing: it is not guessed.
#[test]
fn an_unknown_code_decodes_to_nothing() {
    assert_eq!(need_of(7), None);
    assert_eq!(retry_of(3), None);
    assert_eq!(value_of(4), None);
}
