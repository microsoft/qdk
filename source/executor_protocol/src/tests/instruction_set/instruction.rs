// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Building an instruction by name, its readout parities, its flag
//! patterns, its `pauli` arguments, and copying it.

use crate::tests::allocations;
use crate::{
    Argument, Bit, BlockOperand, BlockType, Instruction, InstructionDef, InstructionFault,
    InstructionSet, Opcode, ParameterKind, Pauli,
};
use smallvec::smallvec;
use std::sync::Arc;

/// A device's gates: QIR's, a feed-forward `x`, Stim's `MPP`, and a
/// measurement with rejection and leakage flags.
fn gates() -> InstructionSet {
    let qubit = || [BlockType::qubit()];
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("h").acts_on(qubit()),
            InstructionDef::new("cx").acts_on([BlockType::qubit(), BlockType::qubit()]),
            InstructionDef::new("u")
                .acts_on(qubit())
                .parameter("theta", ParameterKind::Number)
                .parameter("phi", ParameterKind::Number),
            InstructionDef::new("cond_x")
                .acts_on(qubit())
                .parameter("c", ParameterKind::Bit),
            InstructionDef::new("mpp")
                .acts_on([BlockOperand::variadic(BlockType::qubit())])
                .parameter("p", ParameterKind::Pauli)
                .outcomes(1),
            InstructionDef::new("m")
                .acts_on(qubit())
                .outcomes(1)
                .flags(["reject", "leak"]),
        ],
    )
    .expect("a valid set")
}

fn pauli(text: &str) -> Argument {
    Argument::pauli(text).expect("a Pauli")
}

/// A program names arguments in any order; the instruction holds them in
/// the parameters' order, so that what runs reads them by position.
#[test]
fn instruction_holds_its_arguments_in_parameter_order() {
    let set = gates();
    let u = set
        .instruction("u", [3], &[("phi", 0.2.into()), ("theta", 0.1.into())])
        .expect("a valid instruction");
    assert_eq!(Some(u.opcode), set.opcode("u"));
    assert_eq!(u.blocks.as_slice(), [3]);
    assert_eq!(
        u.arguments.as_slice(),
        [Argument::Number(0.1), Argument::Number(0.2)]
    );
    assert_eq!(u.select, None);
}

#[test]
fn unknown_mnemonic_is_refused() {
    assert_eq!(
        gates().instruction("t", [0], &[]),
        Err(InstructionFault::UnknownMnemonic("t".into()))
    );
}

#[test]
fn argument_for_no_parameter_is_refused() {
    assert_eq!(
        gates().instruction("h", [0], &[("theta", 0.1.into())]),
        Err(InstructionFault::UnknownParameter {
            mnemonic: "h".into(),
            name: "theta".into(),
        })
    );
}

#[test]
fn argument_given_twice_is_refused() {
    let arguments = [
        ("theta", 0.1.into()),
        ("phi", 0.2.into()),
        ("theta", 0.3.into()),
    ];
    assert_eq!(
        gates().instruction("u", [0], &arguments),
        Err(InstructionFault::RepeatedArgument {
            mnemonic: "u".into(),
            parameter: "theta".into(),
        })
    );
}

/// `cx` given one block, or three.
#[test]
fn block_count_other_than_the_instructions_is_refused() {
    let set = gates();
    for blocks in [vec![0], vec![0, 1, 2]] {
        let got = u32::try_from(blocks.len()).expect("a few blocks");
        assert_eq!(
            set.instruction("cx", blocks, &[]),
            Err(InstructionFault::WrongBlockCount {
                mnemonic: "cx".into(),
                expected: 2,
                got,
                variadic: false,
            })
        );
    }
}

/// Stim's `MPP` measures a product over any positive number of qubits.
#[test]
fn variadic_operand_takes_any_positive_number_of_blocks() {
    let set = gates();
    let p = || [("p", pauli("Z_0"))];
    set.instruction("mpp", [7], &p()).expect("one block");
    set.instruction("mpp", 0..20, &p()).expect("twenty blocks");
    assert_eq!(
        set.instruction("mpp", [], &p()),
        Err(InstructionFault::WrongBlockCount {
            mnemonic: "mpp".into(),
            expected: 1,
            got: 0,
            variadic: true,
        })
    );
}

/// A variadic operand's count is its least: the message says so.
#[test]
fn a_variadic_block_count_fault_says_at_least() {
    let set = gates();
    let fault = set
        .instruction("mpp", [], &[("p", pauli("Z_0"))])
        .expect_err("no block");
    assert_eq!(fault.to_string(), "`mpp` takes at least 1 block, given 0");
}

#[test]
fn block_given_twice_is_refused() {
    assert_eq!(
        gates().instruction("cx", [1, 1], &[]),
        Err(InstructionFault::RepeatedBlock {
            mnemonic: "cx".into(),
            block: 1,
        })
    );
}

/// The fault names the first parameter missing, not the first given.
#[test]
fn first_missing_argument_is_refused() {
    assert_eq!(
        gates().instruction("u", [0], &[("phi", 0.2.into())]),
        Err(InstructionFault::MissingArgument {
            mnemonic: "u".into(),
            parameter: "theta".into(),
        })
    );
}

/// `u`'s `theta`, a `number`, given an integer.
#[test]
fn argument_of_another_kind_is_refused() {
    let arguments = [("theta", 1i64.into()), ("phi", 0.2.into())];
    assert_eq!(
        gates().instruction("u", [0], &arguments),
        Err(InstructionFault::WrongArgumentKind {
            mnemonic: "u".into(),
            parameter: "theta".into(),
            expected: ParameterKind::Number,
        })
    );
}

/// Counts back start at 1, the shot's latest readout, as Stim's `rec[-1]`.
#[test]
fn readout_parity_counting_back_zero_is_refused() {
    let set = gates();
    set.instruction("cond_x", [0], &[("c", Argument::recent(1))])
        .expect("the latest readout");
    assert_eq!(
        set.instruction("cond_x", [0], &[("c", Argument::parity([2, 0], true))]),
        Err(InstructionFault::ZeroLookback {
            mnemonic: "cond_x".into(),
        })
    );
}

/// `recent(k)` is Stim's `rec[-k]` itself, not its complement: the
/// parity of the one readout `k` back, unflipped.
#[test]
fn recent_is_the_unflipped_parity_of_one_readout() {
    let Argument::Bit(Bit::Parity(parity)) = Argument::recent(3) else {
        panic!("a readout parity");
    };
    assert_eq!(parity.back(), [3]);
    assert!(!parity.flip());
}

/// A parity reads back the readouts and the flip it was built with, and
/// compares and prints by them: from none, the constant `flip`, through
/// a code's logical readout of five, to more than five.
#[test]
fn readout_parity_reads_compares_and_prints_by_its_readouts_and_flip() {
    let parity_of = |back: &[u32], flip: bool| match Argument::parity(back.iter().copied(), flip) {
        Argument::Bit(Bit::Parity(parity)) => parity,
        other => panic!("a readout parity, not {other:?}"),
    };
    for back in [
        &[][..],
        &[1],
        &[1, 2, 3, 4, 5],
        &[1, 2, 3, 4, 5, 6],
        &[9, 2, 7, 4, 5, 6, 3, 8],
    ] {
        for flip in [false, true] {
            let row = format!("{back:?}, flip {flip}");
            let parity = parity_of(back, flip);
            assert_eq!(parity.back(), back, "{row}");
            assert_eq!(parity.flip(), flip, "{row}");
            assert_eq!(
                format!("{parity:?}"),
                format!("ReadoutParity {{ flip: {flip}, back: {back:?} }}"),
                "{row}"
            );
            assert_eq!(parity, parity_of(back, flip), "{row}");
            assert_eq!(parity.clone(), parity, "{row}");
            assert_ne!(parity, parity_of(back, !flip), "{row}");
            if let Some((last, rest)) = back.split_last() {
                let other = [rest, &[last + 1]].concat();
                assert_ne!(parity, parity_of(&other, flip), "{row}");
                assert_ne!(parity, parity_of(rest, flip), "{row}");
            }
        }
    }
}

/// A QEC correction's parity, of up to five readouts, is held inline, so
/// building one allocates nothing; a sixth readout is the first one that
/// does.
#[test]
fn readout_parity_of_up_to_five_readouts_allocates_nothing() {
    for (count, allocates) in [(0, false), (1, false), (5, false), (6, true), (8, true)] {
        let built = allocations(|| {
            std::hint::black_box(Argument::parity(1..=count, false));
        });
        assert_eq!(
            built > 0,
            allocates,
            "{count} readouts: {built} allocations"
        );
    }
}

fn m() -> crate::Instruction {
    gates().instruction("m", [0], &[]).expect("a measurement")
}

/// Qodec's `select: [{reject: 0, leak: 1}]`: one pattern, both flags.
#[test]
fn select_pattern_matches_flags_agreeing_on_every_flag_it_names() {
    let selected = m()
        .select(&gates(), [[("reject", 0), ("leak", 1)]])
        .expect("declared flags");
    let patterns = selected.select.expect("one pattern");
    let [pattern] = *patterns else {
        panic!("one pattern, not {patterns:?}");
    };
    // Flag `k` is bit `k`: reject is bit 0, leak bit 1.
    assert!(pattern.matches(0b10));
    assert!(!pattern.matches(0b11));
    assert!(!pattern.matches(0b00));
}

/// Qodec's `select: [reject: 0, leak: 0]`: either flag 0.
#[test]
fn select_patterns_are_alternatives() {
    let selected = m()
        .select(&gates(), [[("reject", 0)], [("leak", 0)]])
        .expect("declared flags");
    let patterns = selected.select.expect("two patterns");
    let matched = |flags: u64| patterns.iter().any(|pattern| pattern.matches(flags));
    assert!(matched(0b01));
    assert!(matched(0b10));
    assert!(!matched(0b11));
}

/// Qodec's `select: []`: the instruction expects nothing of its flags.
#[test]
fn no_select_pattern_expects_nothing() {
    let none: [[(&str, u8); 0]; 0] = [];
    let selected = m().select(&gates(), none).expect("no pattern");
    assert_eq!(selected.select, None);
}

/// Qodec's `select: [{}]`: one pattern naming no flag.
#[test]
fn empty_select_pattern_matches_every_shots_flags() {
    let empty: [[(&str, u8); 0]; 1] = [[]];
    let selected = m().select(&gates(), empty).expect("an empty pattern");
    let patterns = selected.select.expect("one pattern");
    let [pattern] = *patterns else {
        panic!("one pattern, not {patterns:?}");
    };
    for flags in [0b00, 0b01, 0b10, 0b11] {
        assert!(pattern.matches(flags), "flags {flags:#b}");
    }
}

#[test]
fn select_on_a_flag_the_instruction_lacks_is_refused() {
    assert_eq!(
        m().select(&gates(), [[("erasure", 1)]]),
        Err(InstructionFault::UnknownFlag {
            mnemonic: "m".into(),
            name: "erasure".into(),
        })
    );
}

#[test]
fn select_naming_a_flag_twice_in_one_pattern_is_refused() {
    assert_eq!(
        m().select(&gates(), [[("leak", 0), ("leak", 1)]]),
        Err(InstructionFault::RepeatedFlag {
            mnemonic: "m".into(),
            name: "leak".into(),
        })
    );
}

#[test]
fn select_value_other_than_a_bit_is_refused() {
    assert_eq!(
        m().select(&gates(), [[("leak", 2)]]),
        Err(InstructionFault::FlagValueNotABit {
            mnemonic: "m".into(),
            name: "leak".into(),
            value: 2,
        })
    );
}

/// A set of one instruction, and an `m` of a larger set, whose opcode lies
/// beyond it.
#[test]
fn select_on_an_opcode_beyond_the_set_is_refused() {
    let small = InstructionSet::new(
        "Small",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("h")],
    )
    .expect("a valid set");
    let m = m();
    assert_eq!(
        m.clone().select(&small, [[("leak", 0)]]),
        Err(InstructionFault::UnknownOpcode(m.opcode))
    );
}

/// A Pauli is held in canonical form, so equal operators written otherwise
/// are equal arguments.
#[test]
fn pauli_argument_is_canonical() {
    let canonical = |p: Pauli| Argument::Pauli(Arc::new(p));
    assert_eq!(pauli("Z_2 X_0"), canonical(Pauli::x(0) * Pauli::z(2)));
    assert_eq!(pauli("+X_0 I_3"), canonical(Pauli::x(0)));
    assert_eq!(pauli("-i Y_1"), canonical(Pauli::x(1) * Pauli::z(1)));
}

/// One text per rule broken: nothing, a sign alone, a factor without its
/// qubit, a qubit twice (`I_k` included), a letter or an index outside the
/// grammar, a signed index, `i` twice, a comma between factors.
#[test]
fn malformed_pauli_text_is_refused() {
    for text in [
        "", "-", "Z", "X_0 Z_0", "I_0 X_0", "X_0 I_0", "I_1 I_1", "W_0", "X_a", "Z_", "X_+1",
        "i i", "X_0,Z_1",
    ] {
        assert_eq!(
            Argument::pauli(text),
            Err(InstructionFault::MalformedPauli(text.into())),
            "{text:?}"
        );
    }
}

/// What a Pauli writes, it reads back: messages and loaders agree.
#[test]
fn pauli_text_reads_back_as_the_same_pauli() {
    for p in [
        Pauli::x(0) * Pauli::z(2),
        -Pauli::z(0),
        Pauli::x(1) * Pauli::z(1),
        Pauli::x(0) * Pauli::x(0),
        -(Pauli::z(1) * Pauli::z(1)),
        Pauli::x(0) * Pauli::y(0) * Pauli::z(0),
    ] {
        assert_eq!(
            pauli(&p.to_string()),
            Argument::Pauli(Arc::new(p.clone())),
            "{p}"
        );
    }
}

/// A part forwards instructions by copying them: with up to three blocks
/// and three arguments, held inline, and its text, Pauli and a readout
/// parity of more than five readouts shared, a copy allocates nothing.
#[test]
fn copying_an_instruction_of_up_to_three_blocks_and_arguments_allocates_nothing() {
    for parity in [Argument::recent(1), Argument::parity(1..=8, true)] {
        let instruction = Instruction {
            opcode: Opcode::__from_macro(0),
            blocks: smallvec![0, 1, 2],
            arguments: smallvec![Argument::Text("label".into()), pauli("X_0 Z_2"), parity],
            select: None,
        };
        let copies = allocations(|| {
            std::hint::black_box(instruction.clone());
        });
        assert_eq!(copies, 0, "{instruction:?}");
    }
}
