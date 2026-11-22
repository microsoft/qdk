// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Building an instruction by name, its readout parities, its flag
//! patterns, its `pauli` arguments, and copying it.

use crate::tests::allocations;
use crate::{
    Argument, Bit, BlockInstruction, BlockInstructionSet, BlockOperand, BlockType, InstructionDef,
    InstructionFault, Opcode, ParameterKind, Pauli, ReadoutParity,
};
use smallvec::smallvec;
use std::sync::Arc;

/// A device's gates: QIR's, a feed-forward `x`, Stim's `MPP`, and a
/// measurement with rejection and leakage flags.
fn gates() -> BlockInstructionSet {
    let qubit = || [BlockType::qubit()];
    BlockInstructionSet::new(
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

/// A program naming `t`, which the device's gates lack, is refused when it
/// builds the instruction.
#[test]
fn unknown_mnemonic_is_refused() {
    assert_eq!(
        gates().instruction("t", [0], &[]),
        Err(InstructionFault::UnknownMnemonic("t".into()))
    );
}

/// A program giving `h` an angle, as if it were `u`, is refused rather
/// than run with the angle silently dropped.
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

/// A program giving `u` two values of `theta` is refused rather than run
/// with one of them chosen for it.
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

/// Stim's `MPP` measures a product over any positive number of qubits:
/// given none, it is refused with a message saying it takes at least one.
#[test]
fn variadic_operand_takes_any_positive_number_of_blocks() {
    let set = gates();
    let p = || [("p", pauli("Z_0"))];
    set.instruction("mpp", [7], &p()).expect("one block");
    set.instruction("mpp", 0..20, &p()).expect("twenty blocks");
    let fault = set.instruction("mpp", [], &p()).expect_err("no block");
    assert_eq!(
        fault,
        InstructionFault::WrongBlockCount {
            mnemonic: "mpp".into(),
            expected: 1,
            got: 0,
            variadic: true,
        }
    );
    let message = fault.to_string();
    for said in ["`mpp`", "at least 1"] {
        assert!(message.contains(said), "{message:?} says {said:?}");
    }
}

/// Instructions over qubits and a code's `carbon` blocks, for the check of
/// blocks given twice: `cx`, Stim's `MPP`, and instructions consuming,
/// making, keeping or coupling blocks of one type or two, few or many.
fn blocks_of_two_types() -> BlockInstructionSet {
    let carbon = || BlockType::new("carbon");
    let qubit = BlockType::qubit;
    let qubits = || BlockOperand::variadic(BlockType::qubit());
    BlockInstructionSet::new(
        "TwoTypes",
        [(qubit(), 1), (carbon(), 2)],
        [
            InstructionDef::new("cx").acts_on([qubit(), qubit()]),
            InstructionDef::new("mpp").acts_on([qubits()]).outcomes(1),
            InstructionDef::new("measure_pair").takes([qubit(), qubit()]),
            InstructionDef::new("prepare_pair").gives([carbon(), carbon()]),
            InstructionDef::new("encode_beside")
                .takes([qubit(), carbon()])
                .gives([carbon(), carbon()]),
            InstructionDef::new("merge_many")
                .takes([carbon().into(), qubits()])
                .gives([carbon()]),
            InstructionDef::new("split_many")
                .takes([carbon()])
                .gives([carbon().into(), qubits()]),
            InstructionDef::new("couple").acts_on([qubit(), carbon()]),
            InstructionDef::new("couple_many").acts_on([carbon().into(), qubits()]),
            InstructionDef::new("measure_mixed").takes([qubit(), carbon()]),
            InstructionDef::new("prepare_mixed").gives([qubit(), carbon()]),
            InstructionDef::new("encode_consuming")
                .takes([qubit(), carbon()])
                .gives([carbon()]),
        ],
    )
    .expect("a valid set")
}

/// Within one instruction, a block, a type and an index, is not consumed
/// twice, made twice, nor made where one survives, for few blocks as for
/// many; of several given twice, the fault names the one at the earliest
/// position that repeats a block before it. Blocks of different types, or
/// one consumed and one made, are different units of the shot's state,
/// so they may share an index, for few blocks as for many. A check keying
/// blocks by index alone fails the rows that share one; a check of the
/// few blocks alone, or of the many alone, fails the rows of the others;
/// one naming the smallest repeated block fails "qubits 5, 3, 5, 3".
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn block_given_twice_is_refused() {
    let set = blocks_of_two_types();
    let repeated = |mnemonic: &str, block| {
        Err(InstructionFault::RepeatedBlock {
            mnemonic: mnemonic.into(),
            block,
        })
    };
    let distinct: Vec<u32> = (0..30).rev().collect();
    let mut with_17_twice = distinct.clone();
    with_17_twice.push(17);
    let mut block_0_and_20_qubits = vec![0];
    block_0_and_20_qubits.extend(0..20);
    let mut block_0_and_20_qubits_one_twice = block_0_and_20_qubits.clone();
    block_0_and_20_qubits_one_twice.push(5);
    let five_and_three_twice = vec![5, 3, 5, 3];
    let mut many_with_five_and_three_twice = five_and_three_twice.clone();
    many_with_five_and_three_twice.extend(10..30);
    for (scene, mnemonic, blocks, expected) in [
        ("`cx` on qubit 1 twice", "cx", vec![1, 1], repeated("cx", 1)),
        (
            "measuring qubit 0 twice in one pair",
            "measure_pair",
            vec![0, 0],
            repeated("measure_pair", 0),
        ),
        (
            "preparing block 0 twice in one pair",
            "prepare_pair",
            vec![0, 0],
            repeated("prepare_pair", 0),
        ),
        (
            "encoding qubit 0 as block 0 beside block 0, which survives",
            "encode_beside",
            vec![0, 0],
            repeated("encode_beside", 0),
        ),
        ("`mpp` on 30 distinct qubits", "mpp", distinct, Ok(())),
        (
            "`mpp` on 30 qubits, then qubit 17 again",
            "mpp",
            with_17_twice,
            repeated("mpp", 17),
        ),
        (
            "merging 20 qubits, qubit 5 twice, into block 0",
            "merge_many",
            block_0_and_20_qubits_one_twice.clone(),
            repeated("merge_many", 5),
        ),
        (
            "splitting 20 qubits, qubit 5 twice, out of block 0",
            "split_many",
            block_0_and_20_qubits_one_twice,
            repeated("split_many", 5),
        ),
        (
            "`mpp` on qubits 5, 3, 5, 3: block 5, not 3, the smaller",
            "mpp",
            five_and_three_twice,
            repeated("mpp", 5),
        ),
        (
            "`mpp` on qubits 5, 3, 5, 3, then 10 to 29: block 5",
            "mpp",
            many_with_five_and_three_twice,
            repeated("mpp", 5),
        ),
        ("coupling qubit 0 to block 0", "couple", vec![0, 0], Ok(())),
        (
            "measuring qubit 0 and block 0",
            "measure_mixed",
            vec![0, 0],
            Ok(()),
        ),
        (
            "preparing qubit 0 and block 0",
            "prepare_mixed",
            vec![0, 0],
            Ok(()),
        ),
        (
            "encoding qubit 0 as block 0, consuming block 0",
            "encode_consuming",
            vec![0, 0],
            Ok(()),
        ),
        (
            "coupling block 0 to qubits 0 to 19",
            "couple_many",
            block_0_and_20_qubits.clone(),
            Ok(()),
        ),
        (
            "splitting block 0 into block 0 and qubits 1 to 19",
            "split_many",
            block_0_and_20_qubits,
            Ok(()),
        ),
    ] {
        assert_eq!(
            set.instruction(mnemonic, blocks, &[]).map(drop),
            expected,
            "{scene}"
        );
    }
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

/// Counts back start at 1, the shot's latest readout, as Stim's `rec[-1]`:
/// a parity counting back 0 is refused wherever 0 is among its readouts,
/// held inline or apart, and one counting back from 1 is taken. A check
/// reading the first readout alone fails "0 second"; one reading the
/// inline readouts alone fails "0 last of six".
#[test]
fn readout_parity_counting_back_zero_is_refused() {
    let set = gates();
    let zero_lookback = Err(InstructionFault::ZeroLookback {
        mnemonic: "cond_x".into(),
    });
    for (scene, parity, expected) in [
        ("the latest readout", Argument::recent(1), Ok(())),
        ("0 back, alone", Argument::recent(0), zero_lookback.clone()),
        (
            "0 second",
            Argument::parity([2, 0], true),
            zero_lookback.clone(),
        ),
        (
            "six readouts, none 0",
            Argument::parity([1, 2, 3, 4, 5, 6], false),
            Ok(()),
        ),
        (
            "0 last of six",
            Argument::parity([1, 2, 3, 4, 5, 0], false),
            zero_lookback,
        ),
    ] {
        assert_eq!(
            set.instruction("cond_x", [0], &[("c", parity)]).map(drop),
            expected,
            "{scene}"
        );
    }
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

/// The readout parities of the tests below, by the readouts they count
/// back and their flip.
fn parity_of(back: &[u32], flip: bool) -> ReadoutParity {
    match Argument::parity(back.iter().copied(), flip) {
        Argument::Bit(Bit::Parity(parity)) => parity,
        other => panic!("a readout parity, not {other:?}"),
    }
}

/// Parities from none, the constant `flip`, through a code's logical
/// readout of five, held inline, to more than five, held apart.
const PARITY_READOUTS: [&[u32]; 5] = [
    &[],
    &[1],
    &[1, 2, 3, 4, 5],
    &[1, 2, 3, 4, 5, 6],
    &[9, 2, 7, 4, 5, 6, 3, 8],
];

/// A decoder compares parities to find a repeated check: two are equal
/// exactly when their readouts and flip are, whether held inline or not.
#[test]
fn readout_parities_are_equal_exactly_when_their_readouts_and_flip_are() {
    for back in PARITY_READOUTS {
        for flip in [false, true] {
            let row = format!("{back:?}, flip {flip}");
            let parity = parity_of(back, flip);
            assert_eq!(parity, parity_of(back, flip), "{row}");
            assert_ne!(parity, parity_of(back, !flip), "{row}");
            if let Some((last, rest)) = back.split_last() {
                let other = [rest, &[last + 1]].concat();
                assert_ne!(parity, parity_of(&other, flip), "{row}");
                assert_ne!(parity, parity_of(rest, flip), "{row}");
            }
        }
    }
}

/// A parity in a failing test's output names its readouts and flip,
/// whether held inline or not.
#[test]
fn a_readout_parity_prints_its_readouts_and_flip() {
    for back in PARITY_READOUTS {
        for flip in [false, true] {
            let printed = format!("{:?}", parity_of(back, flip));
            assert!(
                printed.contains(&format!("{back:?}")),
                "{printed} names the readouts {back:?}"
            );
            assert!(
                printed.contains(&flip.to_string()),
                "{printed} names the flip {flip}"
            );
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

fn m() -> crate::BlockInstruction {
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

/// A decoder post-selecting on an `erasure` flag that `m` does not give
/// is refused rather than matched against a flag no shot sets.
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

/// A pattern asking `leak` to be both 0 and 1 is refused rather than read
/// as one of the two.
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

/// A pattern asking a flag for 2, which no flag bit holds, is refused
/// rather than read as 0 or 1.
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
    let small = BlockInstructionSet::new(
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

/// A Pauli argument is read into the canonical form, so equal operators
/// written otherwise are equal arguments: a `+` sign and an `I_k` factor
/// drop out, a `Y` with its phase is the product it stands for, and
/// whitespace of any kind and length around the text, after the sign and
/// between factors, a qubit's leading zeros, and a bare `I`, repeated or
/// beside other factors, change nothing.
#[test]
fn pauli_argument_is_canonical() {
    let identity = Pauli::x(0) * Pauli::x(0);
    for (text, expected) in [
        ("+X_0 I_3", Pauli::x(0)),
        ("-i Y_1", Pauli::x(1) * Pauli::z(1)),
        (" \t-  X_01\nI ", -Pauli::x(1)),
        ("I I", identity),
        ("X_0 I", Pauli::x(0)),
    ] {
        assert_eq!(
            Argument::pauli(text),
            Ok(Argument::Pauli(Arc::new(expected))),
            "{text:?}"
        );
    }
}

/// One text per rule broken: nothing (whitespace alone included), a sign
/// alone, a factor without its qubit, a qubit twice (`I_k` included), a
/// letter or an index outside the grammar, a signed index, an index beyond
/// a `u32`, two signs, `i` twice, a comma between factors.
#[test]
fn malformed_pauli_text_is_refused() {
    for text in [
        "",
        " \t\n",
        "-",
        "Z",
        "X_0 Z_0",
        "I_0 X_0",
        "X_0 I_0",
        "I_1 I_1",
        "W_0",
        "X_a",
        "Z_",
        "X_+1",
        "X_4294967296",
        "+-X_0",
        "i i",
        "X_0,Z_1",
    ] {
        assert_eq!(
            Argument::pauli(text),
            Err(InstructionFault::MalformedPauli(text.into())),
            "{text:?}"
        );
    }
}

/// A part forwards instructions by copying them: with up to three blocks
/// and three arguments, held inline, and its text, Pauli and a readout
/// parity of more than five readouts shared, a copy allocates nothing.
#[test]
fn copying_an_instruction_of_up_to_three_blocks_and_arguments_allocates_nothing() {
    for parity in [Argument::recent(1), Argument::parity(1..=8, true)] {
        let instruction = BlockInstruction {
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
