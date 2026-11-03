// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::tests::{declared, shots};
use crate::{
    Capabilities, InstructionMismatch, InstructionSet, Need, Needs, OperandKind, Rejected, ShotSet,
    UnmetNeed,
};

/// A small device: two qubits, two shots at once, and none of the yes/no
/// capabilities.
fn small() -> Capabilities {
    Capabilities::default()
        .with_qubits(2)
        .with_resumable_shots(false)
        .with_resets(false)
        .with_replayable(false)
        .with_max_shots(2)
}

/// A set of two instructions, for the needs of the tests.
fn rotations() -> Arc<InstructionSet> {
    Arc::new(declared(
        InstructionSet::new("rotations")
            .with("rz", [OperandKind::Block, OperandKind::Real])
            .with("measure", [OperandKind::Block, OperandKind::Tag]),
    ))
}

/// The need of `needs` on one shot that `small` refuses, with what was
/// needed and offered; `None` when `small` meets every need.
fn unmet_by_small(needs: &Needs) -> Option<(Need, u64, u64)> {
    match small().admit("small", &ShotSet::one(1), needs) {
        Err(Rejected::Unmet(unmet)) => Some((unmet.need, unmet.needed, unmet.offered)),
        _ => None,
    }
}

/// Every run holds its shots at once, so more shots than a component holds
/// at once are refused even when the caller states no need.
#[test]
fn more_shots_than_held_at_once_are_unmet_even_with_the_default_needs() {
    let shots = ShotSet::range(1..=3).expect("1..=3 is not empty");
    assert_eq!(
        small().admit("small", &shots, &Needs::default()),
        Err(Rejected::Unmet(UnmetNeed {
            component: "small".into(),
            need: Need::Shots,
            needed: 3,
            offered: 2,
            mismatches: Vec::new(),
        }))
    );
}

/// A device whose memory holds no shot promises to hold none: a job of one
/// shot is refused before any call, as a larger one is.
#[test]
fn one_shot_is_unmet_where_none_is_held() {
    let holds_none = Capabilities::default().with_max_shots(0);
    let shot = ShotSet::new(vec![0]).expect("one shot, named once");
    assert_eq!(
        holds_none.admit("holds none", &shot, &Needs::default()),
        Err(Rejected::Unmet(UnmetNeed {
            component: "holds none".into(),
            need: Need::Shots,
            needed: 1,
            offered: 0,
            mismatches: Vec::new(),
        }))
    );
}

/// Each need beyond what a component offers is refused, with what was
/// needed and offered; a yes/no capability counts as 1 or 0.
#[test]
fn each_need_beyond_the_offer_is_unmet() {
    assert_eq!(
        unmet_by_small(&Needs::default().with_qubits(3)),
        Some((Need::Qubits, 3, 2))
    );
    assert_eq!(
        unmet_by_small(&Needs::default().with_resumable_shots(true)),
        Some((Need::ResumableShots, 1, 0))
    );
    assert_eq!(
        unmet_by_small(&Needs::default().with_resets(true)),
        Some((Need::Resets, 1, 0))
    );
    assert_eq!(
        unmet_by_small(&Needs::default().with_replayable(true)),
        Some((Need::Replayable, 1, 0))
    );
    assert_eq!(
        unmet_by_small(&Needs::default().with_log_probability(true)),
        Some((Need::LogProbability, 1, 0))
    );
}

/// Needed instructions are matched by their actions between sets of other
/// names, whatever their opcodes: a caller that declares them in another
/// order than the set offered is admitted.
#[test]
fn needed_instructions_in_another_order_are_met() {
    let offered = Arc::new(declared(
        InstructionSet::new("rotations, reversed")
            .with("measure", [OperandKind::Block, OperandKind::Tag])
            .with("rz", [OperandKind::Block, OperandKind::Real]),
    ));
    let device = Capabilities::default().with_instruction_set(Some(offered));
    assert_eq!(
        device.admit(
            "device",
            &ShotSet::one(1),
            &Needs::default().with_instruction_set(Some(rotations()))
        ),
        Ok(())
    );
}

/// A needed instruction the set offered lacks is not offered, and the
/// rejection names it.
#[test]
fn needed_instructions_the_set_lacks_are_unmet() {
    let offered = Arc::new(declared(
        InstructionSet::new("rz only").with("rz", [OperandKind::Block, OperandKind::Real]),
    ));
    let device = Capabilities::default().with_instruction_set(Some(offered));
    assert_eq!(
        device
            .admit(
                "device",
                &ShotSet::one(1),
                &Needs::default().with_instruction_set(Some(rotations()))
            )
            .expect_err("the device lacks `measure`")
            .to_string(),
        "device accepts 1 of the 2 instructions needed: no instruction `measure`"
    );
}

/// A needed instruction that the set offered has under the same mnemonic,
/// taking other operands, is not offered: what the caller sends could not
/// be read there.
#[test]
fn a_needed_instruction_taking_other_operands_is_unmet() {
    let offered = Arc::new(declared(
        InstructionSet::new("rz without angle")
            .with("rz", [OperandKind::Block])
            .with("measure", [OperandKind::Block, OperandKind::Tag]),
    ));
    let device = Capabilities::default().with_instruction_set(Some(offered));
    assert_eq!(
        device
            .admit(
                "device",
                &ShotSet::one(1),
                &Needs::default().with_instruction_set(Some(rotations()))
            )
            .expect_err("the device's `rz` takes no angle")
            .to_string(),
        "device accepts 1 of the 2 instructions needed: `rz` takes other operands"
    );
}

/// Every needed instruction not offered is reported, in the order the
/// caller declares them, so that the caller can fix them all at once.
#[test]
fn every_needed_instruction_not_offered_is_reported() {
    let offered = Arc::new(declared(
        InstructionSet::new("rz without angle").with("rz", [OperandKind::Block]),
    ));
    let device = Capabilities::default().with_instruction_set(Some(offered));
    let refused = device.admit(
        "device",
        &ShotSet::one(1),
        &Needs::default().with_instruction_set(Some(rotations())),
    );
    let Err(Rejected::Unmet(unmet)) = refused else {
        panic!("the device offers neither instruction, but: {refused:?}");
    };
    assert_eq!(
        (unmet.needed, unmet.offered, unmet.mismatches),
        (
            2,
            0,
            vec![
                InstructionMismatch::DifferentOperands {
                    mnemonic: "rz".into()
                },
                InstructionMismatch::Missing {
                    mnemonic: "measure".into()
                }
            ]
        )
    );
}

/// A component that describes no instruction set (it does not say) offers
/// none of the instructions a caller needs by description: the need cannot
/// be checked, so it is refused.
#[test]
fn needed_instructions_are_unmet_by_a_component_with_no_instruction_set() {
    assert_eq!(
        unmet_by_small(&Needs::default().with_instruction_set(Some(rotations()))),
        Some((Need::InstructionSet, 2, 0))
    );
}

/// When several needs are unmet, the first in a fixed order is reported:
/// qubits, resumable shots, resets, replayable, log probability,
/// instruction set, then the run's shots at once. A caller that meets each
/// reported need in turn meets them in that order.
#[test]
fn several_unmet_needs_are_reported_one_at_a_time_in_a_fixed_order() {
    let three = ShotSet::range(1..=3).expect("1..=3 is not empty");
    let mut needs = Needs::default()
        .with_qubits(3)
        .with_resumable_shots(true)
        .with_resets(true)
        .with_replayable(true)
        .with_log_probability(true)
        .with_instruction_set(Some(rotations()));
    let mut reported = Vec::new();
    while let Err(rejected) = small().admit("small", &three, &needs) {
        let Rejected::Unmet(unmet) = rejected else {
            panic!("admission refuses only with `Rejected::Unmet`, but: {rejected:?}");
        };
        reported.push(unmet.need);
        match unmet.need {
            Need::Qubits => needs.qubits = None,
            Need::ResumableShots => needs.resumable_shots = false,
            Need::Resets => needs.resets = false,
            Need::Replayable => needs.replayable = false,
            Need::LogProbability => needs.log_probability = false,
            Need::InstructionSet => needs.instruction_set = None,
            Need::Shots => break,
        }
    }
    assert_eq!(
        reported,
        [
            Need::Qubits,
            Need::ResumableShots,
            Need::Resets,
            Need::Replayable,
            Need::LogProbability,
            Need::InstructionSet,
            Need::Shots
        ]
    );
}

/// A need equal to what a component offers is met.
#[test]
fn a_need_equal_to_the_offer_is_met() {
    assert_eq!(unmet_by_small(&Needs::default().with_qubits(2)), None);
    let two = ShotSet::range(1..=2).expect("1..=2 is not empty");
    assert_eq!(small().admit("small", &two, &Needs::default()), Ok(()));
}

/// A component with no limits, which also computes the log probability and
/// accepts the instructions needed, meets every need.
#[test]
fn a_component_with_no_limits_meets_every_need() {
    let shots = ShotSet::range(1..=3).expect("1..=3 is not empty");
    assert_eq!(
        Capabilities::default()
            .with_resumable_shots(true)
            .with_replayable(true)
            .with_resets(true)
            .with_log_probability(true)
            .with_instruction_set(Some(rotations()))
            .admit(
                "any",
                &shots,
                &Needs::default()
                    .with_qubits(1000)
                    .with_resumable_shots(true)
                    .with_resets(true)
                    .with_replayable(true)
                    .with_log_probability(true)
                    .with_instruction_set(Some(rotations()))
            ),
        Ok(())
    );
}

/// An unmet need that counts says which component offers how much of it,
/// and how much is needed.
#[test]
fn an_unmet_count_says_what_was_offered_and_needed() {
    let shots = ShotSet::range(1..=3).expect("1..=3 is not empty");
    assert_eq!(
        small()
            .admit("small", &shots, &Needs::default())
            .expect_err("two shots at once are fewer than three")
            .to_string(),
        "small holds at most 2 shots at once, 3 needed"
    );
}

/// An unmet count of one reads in the singular: a device of one qubit that
/// holds one shot, given a program of two qubits, then two shots.
#[test]
fn an_unmet_count_of_one_reads_in_the_singular() {
    let one = Capabilities::default().with_qubits(1).with_max_shots(1);
    let two_qubits = one
        .admit("one", &ShotSet::one(1), &Needs::default().with_qubits(2))
        .expect_err("one qubit is too few");
    let two_shots = one
        .admit("one", &shots(&[1, 2]), &Needs::default())
        .expect_err("one state is too few");
    assert_eq!(
        [two_qubits.to_string(), two_shots.to_string()],
        [
            "one offers 1 qubit per shot, 2 needed",
            "one holds at most 1 shot at once, 2 needed"
        ]
    );
}

/// An unmet yes/no need says which component does not offer it, naming
/// the capability.
#[test]
fn an_unmet_yes_or_no_need_says_which_component_lacks_it() {
    let refusal = |needs: Needs| {
        small()
            .admit("small", &ShotSet::one(1), &needs)
            .expect_err("`small` offers none of the yes/no capabilities")
            .to_string()
    };
    assert_eq!(
        [
            refusal(Needs::default().with_resumable_shots(true)),
            refusal(Needs::default().with_resets(true)),
            refusal(Needs::default().with_replayable(true)),
            refusal(Needs::default().with_log_probability(true)),
        ],
        [
            "small does not offer resumable shots",
            "small does not offer resets",
            "small does not offer replayable shots",
            "small does not offer each shot's log probability at close",
        ]
    );
}

/// A component that offers one yes/no capability alone, such as a
/// simulator that resumes shots and does nothing else, or a device that only
/// resets, meets the need for it and no other yes/no need: each need is
/// checked against its own capability, whichever others are offered.
#[test]
fn a_yes_or_no_need_is_met_by_its_own_capability_alone() {
    let offering_only = [
        Capabilities::default().with_resumable_shots(true),
        Capabilities::default().with_resets(true),
        Capabilities::default().with_replayable(true),
        Capabilities::default().with_log_probability(true),
    ];
    let needing_only = [
        Needs::default().with_resumable_shots(true),
        Needs::default().with_resets(true),
        Needs::default().with_replayable(true),
        Needs::default().with_log_probability(true),
    ];
    let met: Vec<Vec<bool>> = (offering_only.iter())
        .map(|offered| {
            (needing_only.iter())
                .map(|needs| offered.admit("lone", &ShotSet::one(1), needs).is_ok())
                .collect()
        })
        .collect();
    let its_own_alone: Vec<Vec<bool>> = (0..4)
        .map(|offered| (0..4).map(|needed| needed == offered).collect())
        .collect();
    assert_eq!(met, its_own_alone);
}

/// An executor that checks a need at call time lets the call through when
/// it offers at least what is needed: a simulator of two qubits, given
/// calls on two qubits, then on one, and a terminal that resumes shots,
/// given a call that continues one.
#[test]
fn a_need_checked_at_call_time_is_met_by_as_much_or_more() {
    assert_eq!(
        [
            UnmetNeed::check("simulator", Need::Qubits, 2, 2),
            UnmetNeed::check("simulator", Need::Qubits, 1, 2),
            UnmetNeed::check("terminal", Need::ResumableShots, 1, 1),
        ],
        [Ok(()), Ok(()), Ok(())]
    );
}

/// An executor that checks a need at call time refuses a call that needs
/// more than it offers as unmet, naming itself, the need, and how much was
/// needed and offered: a terminal that holds each shot for one call alone,
/// given a call that continues a shot.
#[test]
fn a_need_checked_at_call_time_beyond_the_offer_is_refused_as_unmet() {
    assert_eq!(
        UnmetNeed::check("terminal", Need::ResumableShots, 1, 0),
        Err(Rejected::Unmet(UnmetNeed {
            component: "terminal".into(),
            need: Need::ResumableShots,
            needed: 1,
            offered: 0,
            mismatches: Vec::new(),
        }))
    );
}
