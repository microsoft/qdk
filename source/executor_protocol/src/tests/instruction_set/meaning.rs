// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction does: its canonical form, in which two meanings
//! written otherwise compare equal, and its faults. What a meaning refers
//! to is checked when its set is built, so its faults are reported by the
//! set's declaration ([`DeclarationFault::Meaning`]), through which these
//! tests read them.

#[cfg(feature = "serde")]
mod stable_form;

use super::hash_of;
use crate::{
    Angle, BlockType, DeclarationFault, Guard, InstructionDef, InstructionSet, Meaning,
    MeaningFault, Observable, ParameterKind, Pauli,
};

/// Two meanings are equal exactly when their canonical steps and slips
/// are: equality is syntactic, so the order of Clifford images and of
/// guard terms, unchanged generators and terms that cancel are ignored,
/// while `only_if` and `unless`, declared and undeclared, and angles bit
/// for bit (zero and minus zero) are told apart. Equal meanings hash
/// equally, however they were written, so a set holding them does too.
#[test]
fn meanings_are_equal_exactly_when_their_canonical_forms_are() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    let cx_x = || (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let cx_z = || (Pauli::z(1), Pauli::z(0) * Pauli::z(1));
    let x = || Meaning::declared().pauli(Pauli::x(0));
    let rotate = |angle: f64| Meaning::declared().rotate(Pauli::z(0), Angle::Literal(angle));
    for (row, a, b, equal) in [
        // A `cx` listing that it keeps `Z_0` equals the one that leaves it
        // out.
        (
            "a Clifford listing a generator it keeps",
            cx(vec![cx_x(), (Pauli::z(0), Pauli::z(0))]),
            cx(vec![cx_x()]),
            true,
        ),
        (
            "Clifford images in another order",
            cx(vec![cx_x(), cx_z()]),
            cx(vec![cx_z(), cx_x()]),
            true,
        ),
        (
            "guard terms in another order",
            x().only_if([Guard::Parameter(0), Guard::Outcome(0)]),
            x().only_if([Guard::Outcome(0), Guard::Parameter(0)]),
            true,
        ),
        // A term twice cancels in a parity.
        (
            "a guard term given twice",
            x().only_if([
                Guard::Parameter(0),
                Guard::Parameter(1),
                Guard::Parameter(0),
            ]),
            x().only_if([Guard::Parameter(1)]),
            true,
        ),
        // `unless` an empty parity always runs, so the step is as
        // unguarded.
        ("`unless` no term", x().unless([]), x(), true),
        // A guard before any step is a slip (`GuardWithoutStep`) that the
        // meaning keeps, so it differs from one with the same steps, here
        // none, and no slip.
        (
            "a slip",
            Meaning::declared().only_if([Guard::Parameter(0)]),
            Meaning::declared(),
            false,
        ),
        (
            "`only_if` and `unless` one parity",
            x().only_if([Guard::Parameter(0)]),
            x().unless([Guard::Parameter(0)]),
            false,
        ),
        // Declared with no step, an instruction does nothing; undeclared,
        // it makes no claim on what it does.
        (
            "declared and undeclared",
            Meaning::declared(),
            Meaning::undeclared(),
            false,
        ),
        // Angles compare bit for bit, so a meaning is `Eq`.
        ("one angle", rotate(0.5), rotate(0.5), true),
        ("zero and minus zero", rotate(0.0), rotate(-0.0), false),
        ("NaN and itself", rotate(f64::NAN), rotate(f64::NAN), true),
    ] {
        assert_eq!(a == b, equal, "{row}");
        if equal {
            assert_eq!(hash_of(&a), hash_of(&b), "{row}");
        }
    }
}

/// A loader may start every meaning undeclared and add the steps it reads:
/// a step declares it.
#[test]
fn step_added_to_an_undeclared_meaning_declares_it() {
    assert_eq!(
        Meaning::undeclared().pauli(Pauli::x(0)),
        Meaning::declared().pauli(Pauli::x(0))
    );
}

/// `rotate_z0`'s action, for parameters `correction` (a bit) then `theta`:
/// rotate by `theta`, observe `Z_0`, then `X_0` if `correction` and the
/// outcome are odd.
fn rotate_z0() -> InstructionDef {
    InstructionDef::new("rotate_z0")
        .acts_on([BlockType::qubit()])
        .parameter("correction", ParameterKind::Bit)
        .parameter("theta", ParameterKind::Number)
        .outcomes(1)
        .meaning(
            Meaning::declared()
                .rotate(Pauli::z(0), Angle::Parameter(1))
                .observe([Pauli::z(0)])
                .pauli(Pauli::x(0))
                .only_if([Guard::Parameter(0), Guard::Outcome(0)]),
        )
}

/// The faults of `def`'s meaning, reported by the declaration of a set of
/// one qubit block type and `def`.
fn meaning_faults(def: InstructionDef) -> Vec<MeaningFault> {
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [def])
        .expect_err("a faulty declaration")
        .into_iter()
        .map(|fault| match fault {
            DeclarationFault::Meaning { fault, .. } => fault,
            other => panic!("a meaning fault, not {other:?}"),
        })
        .collect()
}

/// A meaning reads only its instruction's parameters, each of the kind
/// its step needs: `rotate_z0`'s meaning, reading its two parameters and
/// its one outcome, is accepted; a parameter it lacks is refused, and so
/// is one of another kind (an angle needs a `number`, a guard a `bit`, an
/// observable a `pauli`).
#[test]
fn meaning_reads_only_its_instructions_parameters_of_the_kind_needed() {
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [rotate_z0()]).expect("a valid set");
    let lacking = rotate_z0()
        .outcomes(0)
        .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(2)));
    let other_kinds = rotate_z0().meaning(
        Meaning::declared()
            .rotate(Pauli::z(0), Angle::Parameter(0))
            .pauli(Pauli::x(0))
            .only_if([Guard::Parameter(1)])
            .observe([Observable::Parameter(1)]),
    );
    let wrong_kind = |step, parameter, expected| MeaningFault::WrongParameterKind {
        step,
        parameter,
        expected,
    };
    for (row, def, expected) in [
        (
            "a parameter it lacks",
            lacking,
            vec![MeaningFault::NotAParameter {
                step: 0,
                parameter: 2,
            }],
        ),
        (
            "parameters of other kinds",
            other_kinds,
            vec![
                wrong_kind(0, 0, ParameterKind::Number),
                wrong_kind(1, 1, ParameterKind::Bit),
                wrong_kind(2, 1, ParameterKind::Pauli),
            ],
        ),
    ] {
        assert_eq!(meaning_faults(def), expected, "{row}");
    }
}

/// A guard reads outcomes of earlier observations only: outcome 0 is
/// observed by step 1, so step 0 cannot read it.
#[test]
fn guard_on_an_outcome_not_observed_before_its_step_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .pauli(Pauli::x(0))
            .only_if([Guard::Outcome(0)])
            .observe([Pauli::z(0)]),
    );
    assert_eq!(
        meaning_faults(def),
        [MeaningFault::OutcomeNotYet {
            step: 0,
            outcome: 0
        }]
    );
}

/// Qodec's outcome count must not depend on a run-time bit.
#[test]
fn guarded_observation_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .observe([Pauli::z(0)])
            .only_if([Guard::Parameter(0)]),
    );
    assert_eq!(
        meaning_faults(def),
        [MeaningFault::GuardedObserve { step: 0 }]
    );
}

/// A loader that reads a guard before the step it guards: the guard
/// guards nothing.
#[test]
fn guard_before_any_step_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .only_if([Guard::Parameter(0)])
            .observe([Pauli::z(0)]),
    );
    assert_eq!(meaning_faults(def), [MeaningFault::GuardWithoutStep]);
}

/// A second guard on one step would replace the first unseen; a guard on
/// each of two steps is accepted.
#[test]
fn step_guarded_twice_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .observe([Pauli::z(0)])
            .pauli(Pauli::x(0))
            .only_if([Guard::Parameter(0)])
            .unless([Guard::Outcome(0)]),
    );
    assert_eq!(meaning_faults(def), [MeaningFault::GuardTwice { step: 1 }]);
    let each_step_once = rotate_z0().meaning(
        Meaning::declared()
            .observe([Pauli::z(0)])
            .pauli(Pauli::x(0))
            .only_if([Guard::Parameter(0)])
            .pauli(Pauli::z(0))
            .unless([Guard::Outcome(0)]),
    );
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [each_step_once])
        .expect("each step guarded once");
}

/// `only_if` of no guard, or of one guard twice, which cancels: the
/// parity is always even, so the step would never run; one guard is
/// accepted.
#[test]
fn step_guarded_by_a_parity_that_is_always_even_is_refused() {
    let guarded = |guards: &[Guard]| {
        rotate_z0().meaning(
            Meaning::declared()
                .observe([Pauli::z(0)])
                .pauli(Pauli::x(0))
                .only_if(guards.iter().copied()),
        )
    };
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [guarded(&[Guard::Parameter(0)])],
    )
    .expect("a step guarded by one guard");
    assert_eq!(
        meaning_faults(guarded(&[])),
        [MeaningFault::GuardNeverMet { step: 1 }]
    );
    assert_eq!(
        meaning_faults(guarded(&[Guard::Parameter(0), Guard::Parameter(0)])),
        [MeaningFault::GuardNeverMet { step: 1 }]
    );
}

/// Guards that cancel still name what they read: a parameter 7 the
/// instruction lacks, or an outcome not yet observed, is refused as in a
/// guard that does not cancel; under `only_if`, beside the parity of no
/// term, and under `unless`, where they leave the step unguarded, alone.
#[test]
fn guards_that_cancel_are_checked_like_any_other() {
    let meaning = |meaning: Meaning| rotate_z0().meaning(meaning.observe([Pauli::z(0)]));
    let guarded = |guard: Guard| {
        meaning(
            Meaning::declared()
                .pauli(Pauli::x(0))
                .only_if([guard, guard]),
        )
    };
    let unless = |guard: Guard| {
        meaning(
            Meaning::declared()
                .pauli(Pauli::x(0))
                .unless([guard, guard]),
        )
    };
    assert_eq!(
        meaning_faults(unless(Guard::Parameter(7))),
        [MeaningFault::NotAParameter {
            step: 0,
            parameter: 7
        }]
    );
    assert_eq!(
        meaning_faults(unless(Guard::Outcome(0))),
        [MeaningFault::OutcomeNotYet {
            step: 0,
            outcome: 0
        }]
    );
    assert_eq!(
        meaning_faults(guarded(Guard::Parameter(7))),
        [
            MeaningFault::GuardNeverMet { step: 0 },
            MeaningFault::NotAParameter {
                step: 0,
                parameter: 7
            },
        ]
    );
    assert_eq!(
        meaning_faults(guarded(Guard::Outcome(0))),
        [
            MeaningFault::GuardNeverMet { step: 0 },
            MeaningFault::OutcomeNotYet {
                step: 0,
                outcome: 0
            },
        ]
    );
}

/// A second image of one generator would replace the first unseen, even
/// an image that maps it to itself.
#[test]
fn clifford_mapping_a_generator_twice_is_refused() {
    let def = InstructionDef::new("h")
        .acts_on([BlockType::qubit()])
        .meaning(Meaning::declared().clifford([
            (Pauli::x(0), Pauli::z(0)),
            (Pauli::z(0), Pauli::x(0)),
            (Pauli::x(0), Pauli::x(0)),
        ]));
    assert_eq!(
        meaning_faults(def),
        [MeaningFault::GeneratorTwice {
            step: 0,
            generator: Pauli::x(0),
        }]
    );
}

/// An observation gives one outcome per observable: `rotate_z0` observes
/// once, yet declares 2 outcomes; an `mzz` observing `Z_0` and `Z_1` in one
/// step gives 2, not 1. An undeclared meaning makes no claim on the
/// outcomes, so any count is accepted.
#[test]
fn only_a_declared_meanings_observations_must_give_the_declared_outcomes() {
    let mzz = |outcomes| {
        InstructionDef::new("mzz")
            .acts_on([BlockType::qubit(), BlockType::qubit()])
            .outcomes(outcomes)
            .meaning(Meaning::declared().observe([Pauli::z(0), Pauli::z(1)]))
    };
    for (row, def, declared, observed) in [
        ("one observable", rotate_z0().outcomes(2), 2, 1),
        ("two observables in one step", mzz(1), 1, 2),
    ] {
        assert_eq!(
            meaning_faults(def),
            [MeaningFault::OutcomeCount { declared, observed }],
            "{row}"
        );
    }
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [mzz(2)])
        .expect("two observables, 2 outcomes");
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [rotate_z0().outcomes(3).meaning(Meaning::undeclared())],
    )
    .expect("an undeclared meaning");
}
