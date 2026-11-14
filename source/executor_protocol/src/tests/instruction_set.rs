// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a set's declaration, its opcodes by name, and the
//! canonical form of what its instructions do.

mod batch_check;
mod fit;
mod instruction;

use crate::{
    Angle, BlockOperand, BlockType, DeclarationFault, Guard, InstructionDef, InstructionSet,
    Meaning, MeaningFault, Observable, Opcode, ParameterKind, Pauli, instruction_set,
};

instruction_set! {
    /// QIR's gates, as a state-vector simulator accepts them.
    CoinGates {
        H = h(qubit),
        RZ = rz(qubit; theta: Number),
        CX = cx(qubit, qubit),
        MZ = mz(qubit) -> 1 readout,
    }
}

instruction_set! {
    /// A code's logical instructions, on blocks of two qubits.
    CodeGates [carbon: 2] {
        MERGE = merge(carbon, carbon),
        MEASURE = measure(carbon; basis: Pauli) -> 2 readouts,
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

/// A set known only at run time, as a loader reads Qodec's YAML (names
/// unknown when the crate is built, here from text), is the set the
/// macro writes: they fit each other, each opcode to itself, and an
/// instruction built by name on it holds the macro's constant.
#[test]
fn set_made_at_run_time_fits_as_a_static_one() {
    // mnemonic, qubits, parameter of kind number, outcomes
    let text = "h 1 - 0; rz 1 theta 0; cx 2 - 0; mz 1 - 1";
    let instructions = text.split("; ").map(|line| {
        let [mnemonic, qubits, parameter, outcomes] = line
            .split(' ')
            .collect::<Vec<_>>()
            .try_into()
            .expect("four fields");
        let qubits: usize = qubits.parse().expect("a count");
        let def = InstructionDef::new(mnemonic)
            .acts_on(vec![BlockType::qubit(); qubits])
            .outcomes(outcomes.parse().expect("a count"));
        if parameter == "-" {
            def
        } else {
            def.parameter(parameter, ParameterKind::Number)
        }
    });
    let loaded = InstructionSet::new("CoinGates", [(BlockType::qubit(), 1)], instructions)
        .expect("a valid set");
    let map = CoinGates::set().fits(&loaded).expect("the same set");
    for opcode in [CoinGates::H, CoinGates::RZ, CoinGates::CX, CoinGates::MZ] {
        assert_eq!(map.map(opcode), opcode);
    }
    loaded
        .fits(&CoinGates::set())
        .expect("the same set, the other way");
    let rz = loaded
        .instruction("rz", [3], &[("theta", 0.25.into())])
        .expect("a valid instruction");
    assert_eq!(rz.opcode, CoinGates::RZ);
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

#[test]
fn clifford_lists_only_the_generators_it_changes() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    assert_eq!(
        cx(vec![
            (Pauli::x(0), Pauli::x(0) * Pauli::x(1)),
            (Pauli::z(0), Pauli::z(0)),
        ]),
        cx(vec![(Pauli::x(0), Pauli::x(0) * Pauli::x(1))]),
    );
}

#[test]
fn clifford_images_compare_in_any_order() {
    let cx = |images: Vec<(Pauli, Pauli)>| Meaning::declared().clifford(images);
    let x = (Pauli::x(0), Pauli::x(0) * Pauli::x(1));
    let z = (Pauli::z(1), Pauli::z(0) * Pauli::z(1));
    assert_eq!(cx(vec![x.clone(), z.clone()]), cx(vec![z, x]));
}

#[test]
fn guard_terms_compare_in_any_order() {
    let guarded = |guards: [Guard; 2]| Meaning::declared().pauli(Pauli::x(0)).only_if(guards);
    assert_eq!(
        guarded([Guard::Parameter(0), Guard::Outcome(0)]),
        guarded([Guard::Outcome(0), Guard::Parameter(0)]),
    );
}

/// A term twice cancels in a parity.
#[test]
fn guard_term_given_twice_cancels() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_eq!(
        x().only_if([
            Guard::Parameter(0),
            Guard::Parameter(1),
            Guard::Parameter(0)
        ]),
        x().only_if([Guard::Parameter(1)]),
    );
}

/// `unless` an empty parity always runs; `if` it never does.
#[test]
fn unless_no_term_is_no_guard() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_eq!(x().unless([]), x());
    assert_ne!(x().only_if([]), x());
}

#[test]
fn if_and_unless_differ() {
    let x = || Meaning::declared().pauli(Pauli::x(0));
    assert_ne!(
        x().only_if([Guard::Parameter(0)]),
        x().unless([Guard::Parameter(0)])
    );
}

#[test]
fn declared_and_undeclared_meanings_differ() {
    assert_ne!(Meaning::declared(), Meaning::undeclared());
}

/// Angles compare bit for bit, so a meaning is `Eq`.
#[test]
fn angles_compare_bit_for_bit() {
    let rotate = |angle: f64| Meaning::declared().rotate(Pauli::z(0), Angle::Literal(angle));
    assert_eq!(rotate(0.5), rotate(0.5));
    assert_ne!(rotate(0.0), rotate(-0.0));
    assert_eq!(rotate(f64::NAN), rotate(f64::NAN));
}

/// `X_0 · Y_0 = i Z_0`, and `Y_0 · X_0 = -i Z_0`.
#[test]
fn pauli_factors_on_one_qubit_multiply_into_one() {
    assert_eq!((Pauli::x(0) * Pauli::y(0)).to_string(), "i Z_0");
    assert_eq!((Pauli::y(0) * Pauli::x(0)).to_string(), "-i Z_0");
}

#[test]
fn pauli_squared_is_the_identity() {
    let xz = Pauli::x(0) * Pauli::z(3);
    assert_eq!((xz.clone() * xz).to_string(), "I");
}

#[test]
fn pauli_factors_are_sorted_by_qubit() {
    assert_eq!(Pauli::z(2) * Pauli::x(0), Pauli::x(0) * Pauli::z(2));
    assert_eq!((Pauli::z(2) * Pauli::x(0)).to_string(), "X_0 Z_2");
}

#[test]
fn pauli_negation_flips_its_sign() {
    assert_eq!((-Pauli::z(0)).to_string(), "-Z_0");
    assert_eq!(-(-Pauli::z(0)), Pauli::z(0));
    assert_eq!((-(Pauli::x(0) * Pauli::x(0))).to_string(), "-I");
}
