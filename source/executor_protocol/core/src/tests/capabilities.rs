// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires against what the part below provides, at one
//! boundary: every mismatch of each kind, the set's own capabilities
//! among them, at once; and the set's capabilities, which flow down a
//! chain as its resources and features do. Each kind's file pins how that
//! kind fits.

mod features;
mod resources;

/// The boundary a run's batches cross.
mod boundary {
    mod batch_limits;
}

use std::any::type_name;

use crate::prelude::*;
use crate::tests::example_sets::{QubitLimited, Qubits, TooFewQubits};
use crate::tests::fake_parts::{
    Log, PassesTheRun, QubitPassThrough, QubitProgram, QubitRegister, in_each_bracketing,
};
use crate::{
    Capabilities, CapabilitiesMismatch, FeatureMismatch, Features, PartId, ResourceMismatch,
    Resources, ShotSet,
};

/// A driver requiring 9 qubits, runs of 10 shots and batches of 4, using
/// every feature, over an executor taking 8 qubits, runs of 5 and batches
/// of 1, and no feature.
fn every_kind_short() -> (Capabilities<QubitLimited>, Capabilities<QubitLimited>) {
    let required = Capabilities::new(
        Resources::unconstrained()
            .with_shots(10)
            .with_instructions_per_batch(4),
        Features::unconstrained().with_replay(true),
        Qubits(9),
    );
    let provided = Capabilities::new(
        Resources::unconstrained()
            .with_shots(5)
            .with_instructions_per_batch(1),
        Features::unconstrained().with_seeded_runs(false),
        Qubits(8),
    );
    (required, provided)
}

/// The set's own mismatch in `mismatch`, read as its type.
fn qubits_short(mismatch: &CapabilitiesMismatch) -> Option<&TooFewQubits> {
    (mismatch.set_capabilities.as_ref())
        .map(|mismatch| mismatch.downcast_ref().expect("the set's own mismatch"))
}

/// Whoever builds the chain fixes every mismatch at once: each short
/// amount, the shots, then the instructions per batch; each feature not
/// provided, replay, then seeded runs; and the set's own capabilities
/// short, as the set's own mismatch, which reads as its type.
#[test]
fn every_mismatch_of_each_kind_is_listed_at_once_in_order() {
    let (required, provided) = every_kind_short();
    let mismatch = required.fits(&provided).expect_err("nothing fits");
    assert_eq!(
        mismatch.resources,
        [
            ResourceMismatch::Shots {
                required: 10,
                provided: 5
            },
            ResourceMismatch::InstructionsPerBatch {
                required: 4,
                provided: 1
            },
        ]
    );
    assert_eq!(
        mismatch.features,
        [FeatureMismatch::Replay, FeatureMismatch::SeededRuns]
    );
    assert_eq!(
        qubits_short(&mismatch),
        Some(&TooFewQubits {
            required: 9,
            provided: 8
        })
    );
}

/// Capabilities fit only when every kind fits: a resource short alone, a
/// feature missing alone, or the set's capabilities short alone, is a
/// mismatch listing it and nothing of the other kinds. A fit reading some
/// kinds alone fails the row of another.
#[test]
fn capabilities_fit_only_when_every_kind_fits() {
    let required = Capabilities::<QubitLimited>::new(
        Resources::unconstrained().with_shots(9),
        Features::unconstrained().with_replay(true),
        Qubits(9),
    );
    let fitting = || required.clone();
    for (row, provided, shots_short, replay_missing, qubits_short_by) in [
        (
            "a resource short alone",
            fitting().map_resources(|resources| resources.with_shots(8)),
            true,
            false,
            None,
        ),
        (
            "a feature missing alone",
            fitting().map_features(|features| features.with_replay(false)),
            false,
            true,
            None,
        ),
        (
            "the set's capabilities short alone",
            Capabilities::new(*required.resources(), *required.features(), Qubits(8)),
            false,
            false,
            Some(TooFewQubits {
                required: 9,
                provided: 8,
            }),
        ),
    ] {
        let mismatch = required.fits(&provided).expect_err(row);
        assert_eq!(!mismatch.resources.is_empty(), shots_short, "{row}");
        assert_eq!(!mismatch.features.is_empty(), replay_missing, "{row}");
        assert_eq!(qubits_short(&mismatch), qubits_short_by.as_ref(), "{row}");
    }
    assert!(required.fits(&fitting()).is_ok(), "what it requires itself");
}

/// A mismatch's message names what is at fault, for a user who reads which
/// requirement to lower, or which part to change: each of its mismatches,
/// a short amount with both amounts, the set's own in the set's words.
#[test]
fn a_mismatch_names_what_is_at_fault() {
    let (required, provided) = every_kind_short();
    let message = required
        .fits(&provided)
        .expect_err("nothing fits")
        .to_string();
    for named in [
        &["shots", "10", "5"][..],
        &["instructions per batch", "4", "1"],
        &["replay"],
        &["seeded runs"],
        &["qubits", "9", "8"],
    ] {
        let piece = message
            .split("; ")
            .find(|piece| named.iter().all(|word| piece.contains(word)));
        assert!(piece.is_some(), "one mismatch names {named:?}: {message}");
    }
}

/// The set's capabilities flow down the chain as its resources and
/// features do, in each bracketing of a program through two adapters on a
/// register: what the driver requires passes both adapters and is fit
/// against the executor's at `on`, which refuses a short one naming the
/// driver and the executor, with the set's own mismatch; and a chain that
/// fits gets each part ready for the qubits the driver requires. A
/// default `requires` dropping the set's capabilities, a join passing
/// them down unstated, or a fit not reading them, fails.
#[test]
fn the_sets_capabilities_flow_down_and_are_fit_at_on() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = QubitProgram::new(&[0, 2], 3);
            let a = QubitPassThrough::new(&log);
            let b = QubitPassThrough::new(&log);
            let e = QubitRegister::new(2, &log);
        },
        |chain, brackets| {
            let faults = chain.err().expect(brackets);
            let [fault] = faults.faults() else {
                panic!("{brackets}: one fault: {faults}");
            };
            assert_eq!(
                fault.requirer,
                PartId::new(0, type_name::<QubitProgram>()),
                "{brackets}"
            );
            assert_eq!(
                fault.provider,
                PartId::new(3, type_name::<QubitRegister>()),
                "{brackets}"
            );
            assert_eq!(
                qubits_short(&fault.mismatch.capabilities),
                Some(&TooFewQubits {
                    required: 3,
                    provided: 2
                }),
                "{brackets}"
            );
        }
    );
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = QubitProgram::new(&[0, 2], 3);
            let a = QubitPassThrough::new(&log);
            let b = QubitPassThrough::new(&log);
            let e = QubitRegister::new(4, &log);
        },
        |chain, brackets| {
            let Ok(mut runner) = chain else {
                panic!("{brackets}: 3 qubits of 4 compose");
            };
            runner.run(&ShotSet::ids([1]), 7).expect(brackets);
            assert_eq!(
                log.entries(),
                [
                    "QubitRegister ready for 3 qubits",
                    "QubitPassThrough ready for 3 qubits",
                    "QubitPassThrough ready for 3 qubits",
                    "QubitRegister execute",
                ],
                "{brackets}"
            );
        }
    );
}

/// A driver that states nothing of the set, such as one sending only
/// batches of no gate, requires the set's capabilities that need nothing,
/// no qubit, so it composes over a register of any size and runs, in each
/// bracketing, each part ready for 0 qubits. A requirement of nothing
/// read as every qubit (`u32::MAX`), as a set stating "no limit" by its
/// largest value would, is refused by every register, and fails.
#[test]
fn a_driver_stating_nothing_of_the_set_composes_over_any_register() {
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let d = PassesTheRun(QubitProgram::new(&[], 1)).stating_limits();
            let a = QubitPassThrough::new(&log);
            let b = QubitPassThrough::new(&log);
            let e = QubitRegister::new(4, &log);
        },
        |chain, brackets| {
            let Ok(mut runner) = chain else {
                panic!("{brackets}: a requirement of nothing fits 4 qubits");
            };
            runner.run(&ShotSet::ids([1]), 7).expect(brackets);
            assert_eq!(
                log.entries(),
                [
                    "QubitRegister ready for 0 qubits",
                    "QubitPassThrough ready for 0 qubits",
                    "QubitPassThrough ready for 0 qubits",
                    "QubitRegister execute",
                ],
                "{brackets}"
            );
        }
    );
}
