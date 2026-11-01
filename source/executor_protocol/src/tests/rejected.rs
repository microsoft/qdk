// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Need, Rejected, RetryClass, UnmetNeed};

/// Each reason this end knows has a fixed retry class: only a resource in
/// use now (`Later`) and a lost verdict (`Resend`) may succeed if tried
/// again; every other reason fails the same way again (`Never`).
#[test]
fn every_known_reason_has_its_retry_class() {
    let unmet = UnmetNeed {
        component: "simulator".into(),
        need: Need::Qubits,
        needed: 9,
        offered: 8,
        mismatches: Vec::new(),
    };
    let never = [
        Rejected::NoShots,
        Rejected::ShotNotHeld(3),
        Rejected::RepeatedShot(3),
        Rejected::IdReused(4),
        Rejected::Instruction {
            at: 1,
            reason: "qubit 9".into(),
        },
        Rejected::Closed,
        Rejected::EarlierFailure("earlier".into()),
        Rejected::Malformed("bytes".into()),
        Rejected::Unmet(unmet),
    ];
    assert!(never.iter().all(|r| r.retry() == RetryClass::Never));
    assert_eq!(
        Rejected::Unavailable("busy".into()).retry(),
        RetryClass::Later
    );
    assert_eq!(
        Rejected::NoReply("timeout".into()).retry(),
        RetryClass::Resend
    );
}

/// A reason a newer far end sent, which this end does not know, keeps what
/// the far end gave it: its retry class and its text, unchanged.
#[test]
fn an_unrecognized_reason_keeps_the_far_ends_class_and_text() {
    let unrecognized = Rejected::Unrecognized {
        retry: RetryClass::Later,
        reason: "newer".into(),
    };
    assert_eq!(unrecognized.retry(), RetryClass::Later);
    assert_eq!(unrecognized.to_string(), "newer");
}

/// A refused instruction says which one and why.
#[test]
fn an_instruction_rejection_says_which_instruction_and_why() {
    let instruction = Rejected::Instruction {
        at: 2,
        reason: "qubit 9 is beyond the 8 qubits".into(),
    };
    assert_eq!(
        instruction.to_string(),
        "instruction 2: qubit 9 is beyond the 8 qubits"
    );
}

/// An instruction rejection from downstream is renumbered as the caller
/// upstream numbers its instructions, keeping its reason: a code that sends
/// each logical instruction as three physical ones reports the 5th physical
/// instruction as the 2nd logical one.
#[test]
fn a_rejection_from_downstream_names_the_instruction_the_caller_sent() {
    let origin = [0, 0, 0, 1, 1, 1];
    let downstream = Rejected::Instruction {
        at: 4,
        reason: "no qubit 7".into(),
    };
    assert_eq!(
        downstream.at_origin(|at| origin[at]),
        Rejected::Instruction {
            at: 1,
            reason: "no qubit 7".into(),
        }
    );
}

/// A rejection that names no instruction comes up from downstream unchanged.
#[test]
fn a_rejection_naming_no_instruction_comes_up_unchanged() {
    let origin = [0, 0, 0, 1, 1, 1];
    assert_eq!(
        Rejected::Closed.at_origin(|at| origin[at]),
        Rejected::Closed
    );
}
