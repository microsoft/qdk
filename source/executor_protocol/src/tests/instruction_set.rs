// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a set's declaration, its opcodes by name, and sets made
//! at run time. The faults of a meaning are here too, since a set's
//! declaration reports them ([`DeclarationFault::Meaning`]); its
//! canonical form is in `meaning.rs`, Pauli operators' in `pauli.rs`.

mod batch_check;
mod fit;
mod instruction;
mod meaning;
mod pauli;

use crate::{
    Angle, Argument, BlockOperand, BlockType, DeclarationFault, Guard, InstructionDef,
    InstructionSet, Meaning, MeaningFault, Observable, Opcode, ParameterKind, Pauli,
    instruction_set,
};
use std::sync::Arc;

// Copied in `instruction_set!`'s doc: keep the two the same.
instruction_set! {
    /// QIR's gates, as a state-vector simulator accepts them.
    CoinGates {
        H = h(qubit),
        RZ = rz(qubit; theta: Number),
        CX = cx(qubit, qubit),
        MZ = mz(qubit) -> 1 outcome,
    }
}

instruction_set! {
    /// A code's logical instructions, on blocks of two qubits.
    CodeGates [carbon: 2] {
        MERGE = merge(carbon, carbon),
        MEASURE = measure(carbon; basis: Pauli) -> 2 outcomes,
    }
}

instruction_set! {
    /// A set whose instruction acts on a block type it does not list.
    Faulty {
        A = a(carbon),
    }
}

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

/// The faults of a set of one qubit block type and `instructions`.
fn faults_of(instructions: impl IntoIterator<Item = InstructionDef>) -> Vec<DeclarationFault> {
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], instructions)
        .expect_err("a faulty declaration")
}

#[test]
fn macro_constants_are_the_opcodes_of_their_mnemonics() {
    let set = CoinGates::set();
    for (opcode, mnemonic) in [
        (CoinGates::H, "h"),
        (CoinGates::RZ, "rz"),
        (CoinGates::CX, "cx"),
        (CoinGates::MZ, "mz"),
    ] {
        assert_eq!(set.opcode(mnemonic), Some(opcode), "{mnemonic}");
        assert_eq!(set.mnemonic(opcode), Some(mnemonic));
    }
}

/// A set known only at run time, as a loader reads Qodec's YAML: names
/// unknown when the crate is built, here read from `text`, one
/// instruction per line (`; `), each `mnemonic qubits parameter outcomes`,
/// then optionally an action of one step: `clifford X_0:Z_0,Z_0:X_0`,
/// `observe Z_0`, or `rotate Z_0` by the parameter (a number), `-` for
/// none.
fn load(name: &str, text: &str) -> InstructionSet {
    // A Pauli from Qodec's text, as a loader outside the crate reads it.
    let pauli = |text: &str| match Argument::pauli(text).expect("a Pauli") {
        Argument::Pauli(pauli) => Arc::unwrap_or_clone(pauli),
        other => panic!("`Argument::pauli` gives a Pauli, not {other:?}"),
    };
    let instructions = text.split("; ").map(|line| {
        let fields: Vec<&str> = line.split(' ').collect();
        let [mnemonic, qubits, parameter, outcomes] = fields[..4] else {
            panic!("four fields in {line:?}");
        };
        let qubits: usize = qubits.parse().expect("a count");
        let mut def = InstructionDef::new(mnemonic)
            .acts_on(vec![BlockType::qubit(); qubits])
            .outcomes(outcomes.parse().expect("a count"));
        if parameter != "-" {
            def = def.parameter(parameter, ParameterKind::Number);
        }
        let step = Meaning::declared();
        match fields[4..] {
            [] => def,
            ["clifford", images] => def.meaning(step.clifford(images.split(',').map(|image| {
                let (generator, image) = image.split_once(':').expect("`generator:image`");
                (pauli(generator), pauli(image))
            }))),
            ["observe", observable] => def.meaning(step.observe([pauli(observable)])),
            ["rotate", axis] => def.meaning(step.rotate(pauli(axis), Angle::Parameter(0))),
            _ => panic!("an action of one step in {line:?}"),
        }
    });
    InstructionSet::new(name, [(BlockType::qubit(), 1)], instructions).expect("a valid set")
}

/// `CoinGates`, read at run time.
fn coin_gates_loaded() -> InstructionSet {
    load("CoinGates", "h 1 - 0; rz 1 theta 0; cx 2 - 0; mz 1 - 1")
}

/// A set known only at run time is the set the macro writes: they fit
/// each other, each opcode to itself.
#[test]
fn set_made_at_run_time_fits_as_a_static_one() {
    let loaded = coin_gates_loaded();
    let map = CoinGates::set().fits(&loaded).expect("the same set");
    for opcode in [CoinGates::H, CoinGates::RZ, CoinGates::CX, CoinGates::MZ] {
        assert_eq!(map.map(opcode), opcode);
    }
    loaded
        .fits(&CoinGates::set())
        .expect("the same set, the other way");
}

/// A part that names its instructions at run time writes the opcodes a
/// part written in code matches on.
#[test]
fn instruction_built_by_name_on_a_run_time_set_holds_the_static_constant() {
    let rz = coin_gates_loaded()
        .instruction("rz", [3], &[("theta", 0.25.into())])
        .expect("a valid instruction");
    assert_eq!(rz.opcode, CoinGates::RZ);
}

/// A run-time set that declares what its instructions do fits a set of
/// another name, with other mnemonics and parameter names, doing the
/// same: the logical `lh`, `lmz` and `lrot` are a device's `h`, `mz` and
/// `rz`, matched by meaning, none by mnemonic.
#[test]
fn set_made_at_run_time_declaring_meanings_fits_by_meaning_under_another_name() {
    let logical = load(
        "Logical",
        "lh 1 - 0 clifford X_0:Z_0,Z_0:X_0; lmz 1 - 1 observe Z_0; lrot 1 theta 0 rotate Z_0",
    );
    let device = load(
        "Device",
        "mz 1 - 1 observe Z_0; rz 1 angle 0 rotate Z_0; h 1 - 0 clifford Z_0:X_0,X_0:Z_0",
    );
    let map = logical.fits(&device).expect("the same instructions");
    for (sent, accepted) in [("lh", "h"), ("lmz", "mz"), ("lrot", "rz")] {
        let opcode = logical.opcode(sent).expect("a logical instruction");
        assert_eq!(device.mnemonic(map.map(opcode)), Some(accepted), "{sent}");
    }
    assert!(map.by_mnemonic().is_empty());
}

/// A code's set lists its block type and its size; its instructions take
/// those blocks, and an argument of the declared kind.
#[test]
fn macro_declares_listed_block_types_and_their_sizes() {
    let written = InstructionSet::new(
        "CodeGates",
        [(carbon(), 2)],
        [
            InstructionDef::new("merge").acts_on([carbon(), carbon()]),
            InstructionDef::new("measure")
                .acts_on([carbon()])
                .parameter("basis", ParameterKind::Pauli)
                .outcomes(2),
        ],
    )
    .expect("a valid set");
    let map = CodeGates::set().fits(&written).expect("the same set");
    assert_eq!(map.map(CodeGates::MERGE), CodeGates::MERGE);
    assert_eq!(map.map(CodeGates::MEASURE), CodeGates::MEASURE);
}

#[test]
#[should_panic(expected = "`Faulty` declares a faulty set")]
fn macro_set_with_a_declaration_fault_panics() {
    let _ = Faulty::A;
    let _ = Faulty::set();
}

#[test]
fn macro_set_is_named_for_its_type() {
    assert_eq!(CoinGates::set().name(), "CoinGates");
}

#[test]
fn opcode_of_an_unknown_mnemonic_is_none() {
    assert_eq!(CoinGates::set().opcode("t"), None);
}

#[test]
fn mnemonic_beyond_the_set_is_none() {
    assert_eq!(CodeGates::set().mnemonic(CoinGates::MZ), None);
}

/// Opcodes are the instructions' places, in the order given.
#[test]
fn opcodes_number_instructions_in_order() {
    let set = InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("x"), InstructionDef::new("h")],
    )
    .expect("a valid set");
    assert_eq!(set.opcode("x"), Some(Opcode::__from_macro(0)));
    assert_eq!(set.opcode("h"), Some(Opcode::__from_macro(1)));
}

#[test]
fn block_type_declared_twice_is_refused() {
    let faults = InstructionSet::new("Gates", [(carbon(), 2), (carbon(), 3)], [])
        .expect_err("a faulty declaration");
    assert_eq!(
        faults,
        [DeclarationFault::RepeatedBlockType {
            block_type: carbon()
        }]
    );
}

#[test]
fn mnemonic_declared_twice_is_refused() {
    let faults = faults_of([InstructionDef::new("h"), InstructionDef::new("h")]);
    assert_eq!(
        faults,
        [DeclarationFault::RepeatedMnemonic {
            mnemonic: "h".into()
        }]
    );
}

/// An undeclared type is reported once per instruction, though it is on
/// several of its operands, on `in` and `out`.
#[test]
fn operand_of_an_undeclared_block_type_is_refused_once() {
    let faults = faults_of([InstructionDef::new("merge")
        .takes([carbon(), carbon()])
        .gives([carbon()])]);
    assert_eq!(
        faults,
        [DeclarationFault::UnknownBlockType {
            mnemonic: "merge".into(),
            block_type: carbon(),
        }]
    );
}

#[test]
fn variadic_operand_before_another_is_refused() {
    let faults = faults_of([InstructionDef::new("mpp").acts_on([
        BlockOperand::variadic(BlockType::qubit()),
        BlockType::qubit().into(),
    ])]);
    assert_eq!(
        faults,
        [DeclarationFault::MisplacedVariadic {
            mnemonic: "mpp".into()
        }]
    );
}

/// A position variadic on `in` and fixed on `out` would make one block
/// of many.
#[test]
fn variadic_on_one_side_only_is_refused() {
    let faults = faults_of([InstructionDef::new("fuse")
        .takes([BlockOperand::variadic(BlockType::qubit())])
        .gives([BlockType::qubit()])]);
    assert_eq!(
        faults,
        [DeclarationFault::MisplacedVariadic {
            mnemonic: "fuse".into()
        }]
    );
}

#[test]
fn variadic_last_operand_is_accepted() {
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("mpp")
            .acts_on([BlockOperand::variadic(BlockType::qubit())])
            .parameter("p", ParameterKind::Pauli)
            .outcomes(1)],
    )
    .expect("a variadic last operand");
}

#[test]
fn parameter_declared_twice_is_refused() {
    let faults = faults_of([InstructionDef::new("u")
        .parameter("theta", ParameterKind::Number)
        .parameter("theta", ParameterKind::Number)]);
    assert_eq!(
        faults,
        [DeclarationFault::RepeatedParameter {
            mnemonic: "u".into(),
            parameter: "theta".into(),
        }]
    );
}

#[test]
fn flag_declared_twice_is_refused() {
    let faults = faults_of([InstructionDef::new("m").flags(["leak", "leak"])]);
    assert_eq!(
        faults,
        [DeclarationFault::RepeatedFlag {
            mnemonic: "m".into(),
            flag: "leak".into(),
        }]
    );
}

/// A flag pattern is a `u64`: 64 flags are accepted, 65 refused.
#[test]
fn more_than_64_flags_are_refused() {
    let flags = |n: usize| (0..n).map(|k| format!("f{k}")).collect::<Vec<_>>();
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("m").flags(flags(64))],
    )
    .expect("64 flags");
    let faults = faults_of([InstructionDef::new("m").flags(flags(65))]);
    assert_eq!(
        faults,
        [DeclarationFault::TooManyFlags {
            mnemonic: "m".into(),
            flags: 65,
        }]
    );
}

/// Whoever writes a set fixes it in one pass: faults of the set and of
/// several instructions come together, in declaration order.
#[test]
fn every_declaration_fault_is_reported_at_once() {
    let faults = InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1), (BlockType::qubit(), 1)],
        [
            InstructionDef::new("m").flags(["leak", "leak"]),
            InstructionDef::new("h").acts_on([carbon()]),
            InstructionDef::new("m"),
        ],
    )
    .expect_err("a faulty declaration");
    assert_eq!(
        faults,
        [
            DeclarationFault::RepeatedBlockType {
                block_type: BlockType::qubit()
            },
            DeclarationFault::RepeatedFlag {
                mnemonic: "m".into(),
                flag: "leak".into(),
            },
            DeclarationFault::UnknownBlockType {
                mnemonic: "h".into(),
                block_type: carbon(),
            },
            DeclarationFault::RepeatedMnemonic {
                mnemonic: "m".into()
            },
        ]
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

fn meaning_faults(def: InstructionDef) -> Vec<MeaningFault> {
    faults_of([def])
        .into_iter()
        .map(|fault| match fault {
            DeclarationFault::Meaning { fault, .. } => fault,
            other => panic!("a meaning fault, not {other:?}"),
        })
        .collect()
}

#[test]
fn meaning_referring_to_what_its_instruction_has_is_accepted() {
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [rotate_z0()]).expect("a valid set");
}

#[test]
fn meaning_naming_a_parameter_its_instruction_lacks_is_refused() {
    let def = rotate_z0()
        .outcomes(0)
        .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(2)));
    assert_eq!(
        meaning_faults(def),
        [MeaningFault::NotAParameter {
            step: 0,
            parameter: 2
        }]
    );
}

/// An angle needs a `number`, a guard a `bit`, an observable a `pauli`.
#[test]
fn meaning_naming_a_parameter_of_another_kind_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .rotate(Pauli::z(0), Angle::Parameter(0))
            .pauli(Pauli::x(0))
            .only_if([Guard::Parameter(1)])
            .observe([Observable::Parameter(1)]),
    );
    assert_eq!(
        meaning_faults(def),
        [
            MeaningFault::WrongParameterKind {
                step: 0,
                parameter: 0,
                expected: ParameterKind::Number,
            },
            MeaningFault::WrongParameterKind {
                step: 1,
                parameter: 1,
                expected: ParameterKind::Bit,
            },
            MeaningFault::WrongParameterKind {
                step: 2,
                parameter: 1,
                expected: ParameterKind::Pauli,
            },
        ]
    );
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

#[test]
fn guard_before_any_step_is_refused() {
    let def = rotate_z0().meaning(
        Meaning::declared()
            .only_if([Guard::Parameter(0)])
            .observe([Pauli::z(0)]),
    );
    assert_eq!(meaning_faults(def), [MeaningFault::GuardWithoutStep]);
}

/// A second guard on one step would replace the first unseen.
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

#[test]
fn observations_giving_other_than_the_declared_outcomes_are_refused() {
    let def = rotate_z0().outcomes(2);
    assert_eq!(
        meaning_faults(def),
        [MeaningFault::OutcomeCount {
            declared: 2,
            observed: 1
        }]
    );
}

/// An undeclared meaning makes no claim on the outcomes.
#[test]
fn undeclared_meaning_accepts_any_outcome_count() {
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [rotate_z0().outcomes(3).meaning(Meaning::undeclared())],
    )
    .expect("an undeclared meaning");
}
