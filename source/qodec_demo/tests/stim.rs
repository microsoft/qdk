//! The Stim parser on repetition3's physical set, `stim+rz`: the rules
//! that its module doc states.

mod common;

use qodec::{Argument, InstructionCall, Operand};
use qodec_demo::stim::parse;

use common::qodec_stim_rz;

/// Each call's mnemonic and qubits.
fn targets(calls: &[InstructionCall]) -> Vec<(&str, Vec<usize>)> {
    calls
        .iter()
        .map(|c| {
            let qubits = c
                .operands
                .iter()
                .map(|o| match o {
                    Operand::Index(q) => *q,
                    other => panic!("a qubit, not {other:?}"),
                })
                .collect();
            (c.mnemonic.as_str(), qubits)
        })
        .collect()
}

/// Repetition3's `idle` gadget copies the data's parities into the
/// ancillas 3 and 4: `CX 0 3 1 3` is two calls, CX(0, 3) then CX(1, 3),
/// each group's control first.
#[test]
fn targets_split_into_calls_of_the_instructions_arity() {
    let source = "R 3 4\nCX 0 3 1 3\nCX 1 4 2 4\nM 3 4";
    let calls = parse(source, &qodec_stim_rz()).expect("the idle gadget parses");

    assert_eq!(
        targets(&calls),
        [
            ("R", vec![3]),
            ("R", vec![4]),
            ("CX", vec![0, 3]),
            ("CX", vec![1, 3]),
            ("CX", vec![1, 4]),
            ("CX", vec![2, 4]),
            ("M", vec![3]),
            ("M", vec![4]),
        ]
    );
}

/// A gadget written for Stim's own tools carries annotations that do not
/// act on qubits, in any case: they give no call.
#[test]
fn annotations_are_skipped() {
    let source = "QUBIT_COORDS(0,0) 0\nR 0\ntick\nM 0\n\
                  DETECTOR rec[-1]\nOBSERVABLE_INCLUDE(0) rec[-1]\nSHIFT_COORDS(0,1)";
    let calls = parse(source, &qodec_stim_rz()).expect("the annotations are skipped");

    assert_eq!(targets(&calls), [("R", vec![0]), ("M", vec![0])]);
}

/// `#` starts a comment, on its own line or after a call.
#[test]
fn comments_are_stripped() {
    let source = "# prepare the data\nR 0 # then read it\nM 0";
    let calls = parse(source, &qodec_stim_rz()).expect("the comments are stripped");

    assert_eq!(targets(&calls), [("R", vec![0]), ("M", vec![0])]);
}

/// Stim's names are case-insensitive: `cx` is `CX`, when the set has no
/// instruction named `cx` exactly.
#[test]
fn a_name_matches_regardless_of_case() {
    let calls = parse("cx 0 3", &qodec_stim_rz()).expect("a CX");

    assert_eq!(targets(&calls), [("CX", vec![0, 3])]);
}

/// An exact name wins over one that matches regardless of case: in a set
/// that declares `R`, then `r`, `r 0` is a call of `r`.
#[test]
fn an_exact_name_wins_over_a_match_regardless_of_case() {
    let mut set = (*qodec_stim_rz()).clone();
    let mut lower = set
        .instructions
        .iter()
        .find(|i| i.mnemonic == "R")
        .expect("stim+rz declares R")
        .clone();
    lower.mnemonic = "r".into();
    set.instructions.push(lower);
    let calls = parse("r 0\nR 1", &set).expect("both names are in the set");

    assert_eq!(targets(&calls), [("r", vec![0]), ("R", vec![1])]);
}

/// An instruction on no qubits takes no targets: in a set that also
/// declares `NOP` on none, `NOP` is one call with no operands, and `NOP 0`
/// is refused.
#[test]
fn an_instruction_on_no_qubits_takes_no_targets() {
    let mut set = (*qodec_stim_rz()).clone();
    let mut nop = set
        .instructions
        .iter()
        .find(|i| i.mnemonic == "R")
        .expect("stim+rz declares R")
        .clone();
    nop.mnemonic = "NOP".into();
    nop.inputs.clear();
    nop.outputs.clear();
    set.instructions.push(nop);
    let calls = parse("NOP", &set).expect("NOP is in the set");

    assert_eq!(targets(&calls), [("NOP", vec![])]);
    let refused = parse("NOP 0", &set).expect_err("NOP takes no targets");
    assert!(refused.contains("takes no targets"), "{refused}");
}

/// Three targets do not make CX calls: a pair and a lone qubit is refused,
/// as are no targets at all.
#[test]
fn a_partial_target_group_is_refused() {
    let set = qodec_stim_rz();
    assert!(parse("CX 0 3 1", &set).is_err(), "a partial group");
    assert!(parse("CX", &set).is_err(), "no targets");
}

/// `rotate_z(0.7)` binds the instruction's parameter, `theta`, to 0.7,
/// on each call of the line.
#[test]
fn arguments_bind_the_instructions_parameters() {
    let calls = parse("rotate_z(0.7) 0 2", &qodec_stim_rz()).expect("a rotation of two qubits");

    assert_eq!(calls.len(), 2);
    for call in &calls {
        let theta = call.arguments.get("theta");
        assert_eq!(theta, Some(&Argument::Number(0.7)), "{call:?}");
        assert_eq!(call.arguments.len(), 1, "{call:?}");
    }
}

/// `rotate_z` takes one number: none, two, or a word is refused.
#[test]
fn arguments_that_do_not_match_the_parameters_are_refused() {
    let set = qodec_stim_rz();
    assert!(parse("rotate_z 0", &set).is_err(), "none");
    assert!(parse("rotate_z(0.7,1) 0", &set).is_err(), "two");
    assert!(parse("rotate_z(theta) 0", &set).is_err(), "a word");
}

/// What a call of the set cannot represent is refused: repetitions,
/// measurement-record and inverted targets, and an unknown name.
#[test]
fn what_the_parser_cannot_represent_is_refused() {
    let set = qodec_stim_rz();
    assert!(parse("REPEAT 2 {\nR 0\n}", &set).is_err(), "REPEAT");
    let unsupported = |line| parse(line, &set).expect_err("refused");
    assert!(unsupported("CX rec[-1] 0").contains("not supported"));
    assert!(unsupported("M !0").contains("not supported"));
    assert!(parse("H 0", &set).is_err(), "not in stim+rz");
}

/// A refusal names the line it is on, counting from 1.
#[test]
fn a_refusal_names_its_line() {
    let refused =
        parse("R 0\n# a comment\nCX 0 3 1", &qodec_stim_rz()).expect_err("a partial group");

    assert!(refused.starts_with("stim line 3:"), "{refused}");
}
