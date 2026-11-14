// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fit of the instructions one part sends to those the part below
//! accepts.

use crate::{
    BlockOperand, BlockType, InstructionDef, InstructionMismatch, InstructionSet, Meaning,
    ParameterKind, Pauli,
};

/// The set `name` over qubits of `qubits` qubits each.
fn set_of(name: &str, qubits: u32, instructions: Vec<InstructionDef>) -> InstructionSet {
    InstructionSet::new(name, [(BlockType::qubit(), qubits)], instructions).expect("a valid set")
}

/// The instruction `mnemonic` on one qubit, in place.
fn gate(mnemonic: &str) -> InstructionDef {
    InstructionDef::new(mnemonic).acts_on([BlockType::qubit()])
}

/// Hadamard, as Qodec's `clifford: {X_0: Z_0, Z_0: X_0}`.
fn hadamard() -> Meaning {
    Meaning::declared().clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::z(0), Pauli::x(0))])
}

fn pauli_x() -> Meaning {
    Meaning::declared().pauli(Pauli::x(0))
}

fn missing(mnemonic: &str) -> InstructionMismatch {
    InstructionMismatch::Missing {
        mnemonic: mnemonic.into(),
    }
}

/// Whoever builds the chain fixes every instruction at once: each one the
/// receiver lacks, in the order sent, the middle ones too.
#[test]
fn instructions_the_receiver_lacks_are_each_missing_in_order() {
    let sent = set_of("Gates", 1, vec![gate("h"), gate("t"), gate("x"), gate("s")]);
    let accepted = set_of("Gates", 1, vec![gate("x"), gate("h")]);
    assert_eq!(sent.fits(&accepted), Err(vec![missing("t"), missing("s")]));
}

/// Stim's `H` is a state vector's `h`: matched by what it does.
#[test]
fn declared_instruction_matches_one_doing_the_same_under_another_mnemonic() {
    let stim = set_of("Stim", 1, vec![gate("H").meaning(hadamard())]);
    let state_vector = set_of(
        "StateVector",
        1,
        vec![gate("x").meaning(pauli_x()), gate("h").meaning(hadamard())],
    );
    let map = stim.fits(&state_vector).expect("the same action");
    let sent = stim.opcode("H").expect("an instruction");
    assert_eq!(Some(map.map(sent)), state_vector.opcode("h"));
}

#[test]
fn instruction_of_its_own_mnemonic_is_preferred_among_those_doing_the_same() {
    let sent = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let accepted = set_of(
        "Device",
        1,
        vec![
            gate("h2").meaning(hadamard()),
            gate("h").meaning(hadamard()),
        ],
    );
    let map = sent.fits(&accepted).expect("the same action");
    let h = sent.opcode("h").expect("an instruction");
    assert_eq!(Some(map.map(h)), accepted.opcode("h"));
}

/// QIR's gates written in code declare no action: between sets of one
/// name, their mnemonics match them, whatever their order.
#[test]
fn undeclared_instructions_match_by_mnemonic_between_sets_of_one_name() {
    let sent = set_of("Gates", 1, vec![gate("x"), gate("h")]);
    let accepted = set_of("Gates", 1, vec![gate("h"), gate("x")]);
    let map = sent.fits(&accepted).expect("one set");
    for mnemonic in ["x", "h"] {
        let opcode = sent.opcode(mnemonic).expect("an instruction");
        assert_eq!(Some(map.map(opcode)), accepted.opcode(mnemonic));
    }
}

/// What a match by mnemonic rests on is reported; a match by meaning is
/// checked, so not listed.
#[test]
fn matches_by_mnemonic_alone_are_listed() {
    let sent = set_of(
        "Gates",
        1,
        vec![gate("h").meaning(hadamard()), gate("y"), gate("z")],
    );
    let accepted = set_of(
        "Gates",
        1,
        vec![gate("z"), gate("y"), gate("h").meaning(hadamard())],
    );
    let map = sent.fits(&accepted).expect("one set");
    assert_eq!(map.by_mnemonic(), ["y", "z"]);
}

/// A mnemonic proves nothing between sets of other names: one mismatch
/// says so, not one per instruction.
#[test]
fn undeclared_instructions_of_sets_of_other_names_are_refused_once() {
    let sent = set_of("Gates", 1, vec![gate("x"), gate("h")]);
    let accepted = set_of("Device", 1, vec![gate("x"), gate("h")]);
    assert_eq!(
        sent.fits(&accepted),
        Err(vec![InstructionMismatch::OtherSet])
    );
}

fn blocks(mnemonic: &str) -> InstructionMismatch {
    InstructionMismatch::Blocks {
        mnemonic: mnemonic.into(),
    }
}

#[test]
fn operand_of_another_block_type_is_refused() {
    let sent = set_of("Gates", 1, vec![gate("h")]);
    let carbon = BlockType::new("carbon");
    let accepted = InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1), (carbon.clone(), 1)],
        [InstructionDef::new("h").acts_on([carbon])],
    )
    .expect("a valid set");
    assert_eq!(sent.fits(&accepted), Err(vec![blocks("h")]));
}

/// One name, two sizes: a block of two qubits is not a qubit.
#[test]
fn block_type_of_another_size_is_refused() {
    let sent = set_of("Gates", 1, vec![gate("h")]);
    let accepted = set_of("Gates", 2, vec![gate("h")]);
    assert_eq!(sent.fits(&accepted), Err(vec![blocks("h")]));
}

#[test]
fn variadic_and_fixed_operands_differ() {
    let mpp = |operand: BlockOperand| InstructionDef::new("mpp").acts_on([operand]);
    let sent = set_of(
        "Gates",
        1,
        vec![mpp(BlockOperand::variadic(BlockType::qubit()))],
    );
    let accepted = set_of("Gates", 1, vec![mpp(BlockType::qubit().into())]);
    assert_eq!(sent.fits(&accepted), Err(vec![blocks("mpp")]));
}

/// A measurement that keeps its qubit is not one that consumes it.
#[test]
fn outputs_are_compared_as_inputs_are() {
    let measure = || InstructionDef::new("m").takes([BlockType::qubit()]);
    let sent = set_of("Gates", 1, vec![measure()]);
    let accepted = set_of("Gates", 1, vec![measure().gives([BlockType::qubit()])]);
    assert_eq!(sent.fits(&accepted), Err(vec![blocks("m")]));
}

#[test]
fn parameters_of_other_kinds_are_refused() {
    let rz = |kind| gate("rz").parameter("theta", kind);
    let sent = set_of("Gates", 1, vec![rz(ParameterKind::Number)]);
    let accepted = set_of("Gates", 1, vec![rz(ParameterKind::Integer)]);
    assert_eq!(
        sent.fits(&accepted),
        Err(vec![InstructionMismatch::Parameters {
            mnemonic: "rz".into()
        }])
    );
}

/// Arguments travel by position, so names are the sender's own.
#[test]
fn parameter_names_may_differ() {
    let rz = |name| gate("rz").parameter(name, ParameterKind::Number);
    let sent = set_of("Gates", 1, vec![rz("theta")]);
    let accepted = set_of("Gates", 1, vec![rz("angle")]);
    sent.fits(&accepted).expect("the same kinds");
}

#[test]
fn readouts_are_compared_as_outcome_and_flag_counts() {
    let m = |flags: &[&str]| gate("m").outcomes(1).flags(flags.iter().copied());
    let sent = set_of("Gates", 1, vec![m(&["leak"])]);
    let accepted = set_of("Gates", 1, vec![m(&[])]);
    assert_eq!(
        sent.fits(&accepted),
        Err(vec![InstructionMismatch::Readouts {
            mnemonic: "m".into(),
            sent: (1, 1),
            accepted: (1, 0),
        }])
    );
}

/// Flags travel by position, as readouts, so names are the sender's own.
#[test]
fn flag_names_may_differ() {
    let m = |flag| gate("m").outcomes(1).flags([flag]);
    let sent = set_of("Gates", 1, vec![m("leak")]);
    let accepted = set_of("Gates", 1, vec![m("erasure")]);
    sent.fits(&accepted).expect("the same readouts");
}

#[test]
fn each_shape_difference_is_reported() {
    let sent = set_of(
        "Gates",
        1,
        vec![
            gate("u")
                .parameter("theta", ParameterKind::Number)
                .outcomes(1),
        ],
    );
    let accepted = set_of(
        "Gates",
        1,
        vec![InstructionDef::new("u").acts_on([BlockType::qubit(), BlockType::qubit()])],
    );
    assert_eq!(
        sent.fits(&accepted),
        Err(vec![
            blocks("u"),
            InstructionMismatch::Parameters {
                mnemonic: "u".into()
            },
            InstructionMismatch::Readouts {
                mnemonic: "u".into(),
                sent: (1, 0),
                accepted: (0, 0),
            },
        ])
    );
}

/// A match by meaning needs the same shape too: a Hadamard on a block of
/// two qubits does not take a qubit's.
#[test]
fn instruction_doing_the_same_on_other_blocks_is_refused() {
    let sent = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let accepted = set_of("Gates", 2, vec![gate("h").meaning(hadamard())]);
    assert_eq!(sent.fits(&accepted), Err(vec![blocks("h")]));
}

#[test]
fn declared_instruction_with_no_namesake_nor_one_doing_the_same_is_missing() {
    let sent = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let accepted = set_of("Gates", 1, vec![gate("x").meaning(pauli_x())]);
    assert_eq!(sent.fits(&accepted), Err(vec![missing("h")]));
}

/// One mnemonic declared to do two things at one boundary is a collision,
/// refused though another instruction does the same.
#[test]
fn mnemonic_declared_to_do_something_else_is_refused() {
    let sent = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let accepted = set_of(
        "Gates",
        1,
        vec![
            gate("h").meaning(pauli_x()),
            gate("had").meaning(hadamard()),
        ],
    );
    assert_eq!(
        sent.fits(&accepted),
        Err(vec![InstructionMismatch::Meaning {
            mnemonic: "h".into()
        }])
    );
}

/// What one side declares, the other does not confirm: refused both ways.
#[test]
fn meaning_declared_on_one_side_only_is_refused() {
    let declared = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let undeclared = set_of("Gates", 1, vec![gate("h")]);
    let meaning = Err(vec![InstructionMismatch::Meaning {
        mnemonic: "h".into(),
    }]);
    assert_eq!(declared.fits(&undeclared), meaning);
    assert_eq!(undeclared.fits(&declared), meaning);
}

/// The shape is compared before a one-sided meaning: a declared `h` on
/// two qubits, against an undeclared `h` on one, differs in its blocks
/// first.
#[test]
fn shape_mismatch_is_reported_before_a_one_sided_meaning() {
    let declared = set_of(
        "Gates",
        1,
        vec![
            InstructionDef::new("h")
                .acts_on([BlockType::qubit(), BlockType::qubit()])
                .meaning(Meaning::declared().pauli(Pauli::x(0) * Pauli::x(1))),
        ],
    );
    let undeclared = set_of("Gates", 1, vec![gate("h")]);
    assert_eq!(declared.fits(&undeclared), Err(vec![blocks("h")]));
}
