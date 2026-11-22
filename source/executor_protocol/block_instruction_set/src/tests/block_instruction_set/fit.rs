// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fit of the instructions one part sends to those the part below
//! accepts.

use crate::{
    BlockInstructionSet, BlockOperand, BlockType, InstructionDef, InstructionMismatch, Meaning,
    ParameterKind, Pauli,
};
use executor_protocol_core::BoundaryCompatible;

/// The set `name` over qubits of `qubits` qubits each.
fn set_of(name: &str, qubits: u32, instructions: Vec<InstructionDef>) -> BlockInstructionSet {
    BlockInstructionSet::new(name, [(BlockType::qubit(), qubits)], instructions)
        .expect("a valid set")
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
    assert_eq!(
        sent.fits(&accepted).map_err(|m| m.mismatches().to_vec()),
        Err(vec![missing("t"), missing("s")])
    );
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

/// A receiver's `h2` and `h` are both Hadamards: the sent `h` maps to `h`,
/// though `h2` comes first.
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
        sent.fits(&accepted).map_err(|m| m.mismatches().to_vec()),
        Err(vec![InstructionMismatch::OtherSet])
    );
}

/// The set of `instruction` over qubits and `carbon` blocks.
fn qubit_and_carbon(instruction: InstructionDef) -> BlockInstructionSet {
    BlockInstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1), (BlockType::new("carbon"), 1)],
        [instruction],
    )
    .expect("a valid set")
}

fn blocks(mnemonic: &str) -> InstructionMismatch {
    InstructionMismatch::Blocks {
        mnemonic: mnemonic.into(),
    }
}

/// Between sets, an instruction fits one of the same shape only: the same
/// block operands (type, size, order, variadic or not, input or output),
/// the same parameter kinds in order and the same readout counts. A match
/// by meaning needs the same shape too: a Hadamard on a block of two
/// qubits does not take a qubit's.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per shape")]
fn an_instruction_fits_only_one_of_the_same_shape() {
    let carbon = BlockType::new("carbon");
    let load = |operands: [BlockType; 2]| InstructionDef::new("load").acts_on(operands);
    let mpp = |operand: BlockOperand| InstructionDef::new("mpp").acts_on([operand]);
    let measure = || InstructionDef::new("m").takes([BlockType::qubit()]);
    let rz = |kind| gate("rz").parameter("theta", kind);
    let u = |first, second| gate("u").parameter("theta", first).parameter("k", second);
    let m = |flags: &[&str]| gate("m").outcomes(1).flags(flags.iter().copied());
    let parameters = |mnemonic: &str| {
        Err(vec![InstructionMismatch::Parameters {
            mnemonic: mnemonic.into(),
        }])
    };
    for (row, sent, accepted, fit) in [
        (
            "an operand of another block type",
            set_of("Gates", 1, vec![gate("h")]),
            qubit_and_carbon(InstructionDef::new("h").acts_on([carbon.clone()])),
            Err(vec![blocks("h")]),
        ),
        // Operands are read by position.
        (
            "the same block types in another order",
            qubit_and_carbon(load([BlockType::qubit(), carbon.clone()])),
            qubit_and_carbon(load([carbon.clone(), BlockType::qubit()])),
            Err(vec![blocks("load")]),
        ),
        // One name, two sizes: a block of two qubits is not a qubit.
        (
            "a block type of another size",
            set_of("Gates", 1, vec![gate("h")]),
            set_of("Gates", 2, vec![gate("h")]),
            Err(vec![blocks("h")]),
        ),
        (
            "a block type of another size, both doing the same",
            set_of("Gates", 1, vec![gate("h").meaning(hadamard())]),
            set_of("Gates", 2, vec![gate("h").meaning(hadamard())]),
            Err(vec![blocks("h")]),
        ),
        // An `mpp` on any number of qubits is not one on exactly one.
        (
            "a variadic operand against a fixed one",
            set_of(
                "Gates",
                1,
                vec![mpp(BlockOperand::variadic(BlockType::qubit()))],
            ),
            set_of("Gates", 1, vec![mpp(BlockType::qubit().into())]),
            Err(vec![blocks("mpp")]),
        ),
        // A measurement that keeps its qubit is not one that consumes it.
        (
            "an output against none",
            set_of("Gates", 1, vec![measure()]),
            set_of("Gates", 1, vec![measure().gives([BlockType::qubit()])]),
            Err(vec![blocks("m")]),
        ),
        // An instruction acting on its qubit in place takes it and gives
        // it back.
        (
            "in place against taking and giving the same block",
            set_of("Gates", 1, vec![gate("h")]),
            set_of(
                "Gates",
                1,
                vec![
                    InstructionDef::new("h")
                        .takes([BlockType::qubit()])
                        .gives([BlockType::qubit()]),
                ],
            ),
            Ok(()),
        ),
        (
            "a parameter of another kind",
            set_of("Gates", 1, vec![rz(ParameterKind::Number)]),
            set_of("Gates", 1, vec![rz(ParameterKind::Integer)]),
            parameters("rz"),
        ),
        // Arguments travel by position.
        (
            "parameter kinds in another order",
            set_of(
                "Gates",
                1,
                vec![u(ParameterKind::Number, ParameterKind::Integer)],
            ),
            set_of(
                "Gates",
                1,
                vec![u(ParameterKind::Integer, ParameterKind::Number)],
            ),
            parameters("u"),
        ),
        // An `m` with a `leak` flag against one without: one readout slot
        // more.
        (
            "a flag more",
            set_of("Gates", 1, vec![m(&["leak"])]),
            set_of("Gates", 1, vec![m(&[])]),
            Err(vec![InstructionMismatch::Readouts {
                mnemonic: "m".into(),
                sent: (1, 1),
                accepted: (1, 0),
            }]),
        ),
    ] {
        assert_eq!(
            sent.fits(&accepted)
                .map(|_| ())
                .map_err(|m| m.mismatches().to_vec()),
            fit,
            "{row}"
        );
    }
}

/// A sender maps only its own opcodes: one beyond its set's last
/// instruction, here an opcode of a larger set, panics.
#[test]
#[should_panic]
#[expect(
    clippy::should_panic_without_expect,
    reason = "`OpcodeMap::map` promises a panic, not its message"
)]
fn mapping_an_opcode_beyond_the_sending_set_panics() {
    let sent = set_of("Gates", 1, vec![gate("h")]);
    let larger = set_of("Gates", 1, vec![gate("h"), gate("x")]);
    let map = sent.fits(&larger).expect("`h` fits");
    let beyond = larger.opcode("x").expect("`x` is in the larger set");
    let _ = map.map(beyond);
}

/// Arguments and flags travel by position, as readouts do, so their
/// names are the sender's own.
#[test]
fn parameter_and_flag_names_are_the_senders_own() {
    let rz = |name| gate("rz").parameter(name, ParameterKind::Number);
    let m = |flag| gate("m").outcomes(1).flags([flag]);
    for (row, sent, accepted) in [
        (
            "parameter names",
            set_of("Gates", 1, vec![rz("theta")]),
            set_of("Gates", 1, vec![rz("angle")]),
        ),
        (
            "flag names",
            set_of("Gates", 1, vec![m("leak")]),
            set_of("Gates", 1, vec![m("erasure")]),
        ),
    ] {
        sent.fits(&accepted)
            .unwrap_or_else(|mismatches| panic!("{row}: {mismatches:?}"));
    }
}

/// A `u` that differs in its blocks, its parameters and its readouts: three
/// mismatches, not the first alone.
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
        sent.fits(&accepted).map_err(|m| m.mismatches().to_vec()),
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

/// A declared instruction is missing unless the receiver has its namesake
/// or one doing the same on the same blocks: a receiver with only an `x`,
/// or with a `had` doing what `h` does on two qubits instead of one, has
/// no match for `h`.
#[test]
fn declared_instruction_with_no_namesake_nor_one_doing_the_same_on_its_blocks_is_missing() {
    let sent = set_of("Gates", 1, vec![gate("h").meaning(hadamard())]);
    let had = InstructionDef::new("had")
        .acts_on([BlockType::qubit(), BlockType::qubit()])
        .meaning(hadamard());
    for (row, accepted) in [
        (
            "an `x` alone",
            set_of("Gates", 1, vec![gate("x").meaning(pauli_x())]),
        ),
        ("a `had` on other blocks", set_of("Gates", 1, vec![had])),
    ] {
        assert_eq!(
            sent.fits(&accepted).map_err(|m| m.mismatches().to_vec()),
            Err(vec![missing("h")]),
            "{row}"
        );
    }
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
        sent.fits(&accepted).map_err(|m| m.mismatches().to_vec()),
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
    assert_eq!(
        declared
            .fits(&undeclared)
            .map_err(|m| m.mismatches().to_vec()),
        meaning
    );
    assert_eq!(
        undeclared
            .fits(&declared)
            .map_err(|m| m.mismatches().to_vec()),
        meaning
    );
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
    assert_eq!(
        declared
            .fits(&undeclared)
            .map_err(|m| m.mismatches().to_vec()),
        Err(vec![blocks("h")])
    );
}

/// A mismatch's message names what is at fault, for whoever builds the
/// chain and must change a part: the instruction, by its mnemonic, which
/// nothing else in the message says, and what differs, in the words of
/// its kind (blocks, parameters, readouts, what it does, or the set),
/// with, for readouts, the sent counts before the word "accept" and the
/// accepted ones after it, so a reader tells which side has which; and
/// for one mnemonic, no two kinds say the same. A formatter that drops
/// the mnemonic, a count or what differs, says one kind's difference for
/// another, swaps the sides, or says the same for two kinds, fails. A fit
/// refused for several instructions names each of them, so the chain's
/// builder fixes all at once; a message of the first alone fails.
#[test]
fn a_mismatch_names_its_instruction_and_what_differs() {
    let cz = || "cz".to_owned();
    let rows = [
        (missing("cz"), vec!["cz", "not"], None),
        (blocks("cz"), vec!["cz", "block"], None),
        (
            InstructionMismatch::Parameters { mnemonic: cz() },
            vec!["cz", "parameter"],
            None,
        ),
        (
            InstructionMismatch::Readouts {
                mnemonic: cz(),
                sent: (3, 5),
                accepted: (7, 9),
            },
            vec!["cz", "outcome", "flag"],
            Some((["3", "5"], ["7", "9"])),
        ),
        (
            InstructionMismatch::Meaning { mnemonic: cz() },
            vec!["cz", "do"],
            None,
        ),
        (InstructionMismatch::OtherSet, vec!["set"], None),
    ];
    let messages: Vec<String> = rows
        .iter()
        .map(|(mismatch, ..)| mismatch.to_string())
        .collect();
    for ((mismatch, words, sides), message) in rows.iter().zip(&messages) {
        for word in words {
            assert!(
                message.contains(word),
                "{mismatch:?} says {word}: {message}"
            );
        }
        let Some((sent, accepted)) = sides else {
            continue;
        };
        let (before, after) = message.split_once("accept").unwrap_or((message, ""));
        for word in sent {
            assert!(
                before.contains(word),
                "{mismatch:?} names {word} first: {message}"
            );
        }
        for word in accepted {
            assert!(
                after.contains(word),
                "{mismatch:?} names {word} as accepted: {message}"
            );
        }
    }
    for (i, message) in messages.iter().enumerate() {
        assert!(
            !messages[i + 1..].contains(message),
            "two kinds say {message}"
        );
    }
    let sent = set_of("Gates", 1, vec![gate("cz"), gate("swap")]);
    let refused = sent
        .fits(&set_of("Gates", 1, vec![]))
        .expect_err("a receiver of no instruction")
        .to_string();
    for mnemonic in ["cz", "swap"] {
        assert!(
            refused.contains(mnemonic),
            "a fit missing `cz` and `swap` names {mnemonic}: {refused}"
        );
    }
}
