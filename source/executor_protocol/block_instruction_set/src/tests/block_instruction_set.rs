// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The checks of a set's declaration, its opcodes by name, and the sets
//! the macro writes, and the check of an instruction written by hand
//! against the shots' requirement, with the readout slots it agrees with.
//! The faults of a meaning, which a set's declaration reports
//! ([`DeclarationFault::Meaning`]), are in `meaning.rs`, Pauli operators'
//! in `pauli.rs`.

mod block_capabilities;
mod block_instruction;
mod fit;
mod meaning;
mod pauli;
mod shot_conditions;

use crate::{
    Argument, Bit, BlockCapabilities, BlockInstruction, BlockInstructionSet, BlockOperand,
    BlockType, DeclarationFault, InstructionDef, InstructionFault, Opcode, ParameterKind,
    instruction_set,
};
use executor_protocol_core::{BoundaryCompatible, InstructionSet};
use smallvec::smallvec;

// Copied in `instruction_set!`'s doc: keep the two the same.
instruction_set! {
    /// QIR's gates, as a state-vector simulator accepts them.
    RotationGates {
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
    BlockInstructionSet::new("Gates", [(BlockType::qubit(), 1)], instructions)
        .expect_err("a faulty declaration")
}

#[expect(dead_code, reason = "the test reads a few of the 200 constants")]
mod many_gates {
    use crate::instruction_set;

    instruction_set! {
        /// A merged set of 200 instructions, more than a macro counting by
        /// recursion could number.
        pub ManyGates {
            I000 = i000(qubit), I001 = i001(qubit), I002 = i002(qubit), I003 = i003(qubit),
            I004 = i004(qubit), I005 = i005(qubit), I006 = i006(qubit), I007 = i007(qubit),
            I008 = i008(qubit), I009 = i009(qubit), I010 = i010(qubit), I011 = i011(qubit),
            I012 = i012(qubit), I013 = i013(qubit), I014 = i014(qubit), I015 = i015(qubit),
            I016 = i016(qubit), I017 = i017(qubit), I018 = i018(qubit), I019 = i019(qubit),
            I020 = i020(qubit), I021 = i021(qubit), I022 = i022(qubit), I023 = i023(qubit),
            I024 = i024(qubit), I025 = i025(qubit), I026 = i026(qubit), I027 = i027(qubit),
            I028 = i028(qubit), I029 = i029(qubit), I030 = i030(qubit), I031 = i031(qubit),
            I032 = i032(qubit), I033 = i033(qubit), I034 = i034(qubit), I035 = i035(qubit),
            I036 = i036(qubit), I037 = i037(qubit), I038 = i038(qubit), I039 = i039(qubit),
            I040 = i040(qubit), I041 = i041(qubit), I042 = i042(qubit), I043 = i043(qubit),
            I044 = i044(qubit), I045 = i045(qubit), I046 = i046(qubit), I047 = i047(qubit),
            I048 = i048(qubit), I049 = i049(qubit), I050 = i050(qubit), I051 = i051(qubit),
            I052 = i052(qubit), I053 = i053(qubit), I054 = i054(qubit), I055 = i055(qubit),
            I056 = i056(qubit), I057 = i057(qubit), I058 = i058(qubit), I059 = i059(qubit),
            I060 = i060(qubit), I061 = i061(qubit), I062 = i062(qubit), I063 = i063(qubit),
            I064 = i064(qubit), I065 = i065(qubit), I066 = i066(qubit), I067 = i067(qubit),
            I068 = i068(qubit), I069 = i069(qubit), I070 = i070(qubit), I071 = i071(qubit),
            I072 = i072(qubit), I073 = i073(qubit), I074 = i074(qubit), I075 = i075(qubit),
            I076 = i076(qubit), I077 = i077(qubit), I078 = i078(qubit), I079 = i079(qubit),
            I080 = i080(qubit), I081 = i081(qubit), I082 = i082(qubit), I083 = i083(qubit),
            I084 = i084(qubit), I085 = i085(qubit), I086 = i086(qubit), I087 = i087(qubit),
            I088 = i088(qubit), I089 = i089(qubit), I090 = i090(qubit), I091 = i091(qubit),
            I092 = i092(qubit), I093 = i093(qubit), I094 = i094(qubit), I095 = i095(qubit),
            I096 = i096(qubit), I097 = i097(qubit), I098 = i098(qubit), I099 = i099(qubit),
            I100 = i100(qubit), I101 = i101(qubit), I102 = i102(qubit), I103 = i103(qubit),
            I104 = i104(qubit), I105 = i105(qubit), I106 = i106(qubit), I107 = i107(qubit),
            I108 = i108(qubit), I109 = i109(qubit), I110 = i110(qubit), I111 = i111(qubit),
            I112 = i112(qubit), I113 = i113(qubit), I114 = i114(qubit), I115 = i115(qubit),
            I116 = i116(qubit), I117 = i117(qubit), I118 = i118(qubit), I119 = i119(qubit),
            I120 = i120(qubit), I121 = i121(qubit), I122 = i122(qubit), I123 = i123(qubit),
            I124 = i124(qubit), I125 = i125(qubit), I126 = i126(qubit), I127 = i127(qubit),
            I128 = i128(qubit), I129 = i129(qubit), I130 = i130(qubit), I131 = i131(qubit),
            I132 = i132(qubit), I133 = i133(qubit), I134 = i134(qubit), I135 = i135(qubit),
            I136 = i136(qubit), I137 = i137(qubit), I138 = i138(qubit), I139 = i139(qubit),
            I140 = i140(qubit), I141 = i141(qubit), I142 = i142(qubit), I143 = i143(qubit),
            I144 = i144(qubit), I145 = i145(qubit), I146 = i146(qubit), I147 = i147(qubit),
            I148 = i148(qubit), I149 = i149(qubit), I150 = i150(qubit), I151 = i151(qubit),
            I152 = i152(qubit), I153 = i153(qubit), I154 = i154(qubit), I155 = i155(qubit),
            I156 = i156(qubit), I157 = i157(qubit), I158 = i158(qubit), I159 = i159(qubit),
            I160 = i160(qubit), I161 = i161(qubit), I162 = i162(qubit), I163 = i163(qubit),
            I164 = i164(qubit), I165 = i165(qubit), I166 = i166(qubit), I167 = i167(qubit),
            I168 = i168(qubit), I169 = i169(qubit), I170 = i170(qubit), I171 = i171(qubit),
            I172 = i172(qubit), I173 = i173(qubit), I174 = i174(qubit), I175 = i175(qubit),
            I176 = i176(qubit), I177 = i177(qubit), I178 = i178(qubit), I179 = i179(qubit),
            I180 = i180(qubit), I181 = i181(qubit), I182 = i182(qubit), I183 = i183(qubit),
            I184 = i184(qubit), I185 = i185(qubit), I186 = i186(qubit), I187 = i187(qubit),
            I188 = i188(qubit), I189 = i189(qubit), I190 = i190(qubit), I191 = i191(qubit),
            I192 = i192(qubit), I193 = i193(qubit), I194 = i194(qubit), I195 = i195(qubit),
            I196 = i196(qubit), I197 = i197(qubit), I198 = i198(qubit), I199 = i199(qubit),
        }
    }
}
use many_gates::ManyGates;

/// A part matches on the macro's constants: each is the opcode of its
/// mnemonic, both ways. A device's set merged with a code's may hold
/// hundreds of instructions: the macro numbers each in place, the last
/// too.
#[test]
fn macro_constants_are_the_opcodes_of_their_mnemonics() {
    let rotation = RotationGates::set();
    let many = ManyGates::set();
    for (set, opcode, mnemonic) in [
        (&rotation, RotationGates::H, "h"),
        (&rotation, RotationGates::RZ, "rz"),
        (&rotation, RotationGates::CX, "cx"),
        (&rotation, RotationGates::MZ, "mz"),
        (&many, ManyGates::I000, "i000"),
        (&many, ManyGates::I127, "i127"),
        (&many, ManyGates::I128, "i128"),
        (&many, ManyGates::I199, "i199"),
    ] {
        assert_eq!(set.opcode(mnemonic), Some(opcode), "{mnemonic}");
        assert_eq!(set.mnemonic(opcode), Some(mnemonic), "{mnemonic}");
    }
}

/// A set built at run time, as a loader reads Qodec's YAML, from the
/// declaration a macro's set is written from, is the macro's set: they
/// fit each other, each opcode to itself, so a part matching on the
/// macro's constants reads what a part naming its instructions at run
/// time writes. Both on the default qubit blocks and on listed block
/// types of their own size.
#[test]
fn macro_set_is_the_set_its_declaration_builds() {
    let qubit = || [BlockType::qubit()];
    for (row, set, built, opcodes) in [
        (
            "qubit blocks",
            RotationGates::set(),
            BlockInstructionSet::new(
                "RotationGates",
                [(BlockType::qubit(), 1)],
                [
                    InstructionDef::new("h").acts_on(qubit()),
                    InstructionDef::new("rz")
                        .acts_on(qubit())
                        .parameter("theta", ParameterKind::Number),
                    InstructionDef::new("cx").acts_on([BlockType::qubit(), BlockType::qubit()]),
                    InstructionDef::new("mz").acts_on(qubit()).outcomes(1),
                ],
            ),
            &[
                RotationGates::H,
                RotationGates::RZ,
                RotationGates::CX,
                RotationGates::MZ,
            ][..],
        ),
        (
            "listed block types",
            CodeGates::set(),
            BlockInstructionSet::new(
                "CodeGates",
                [(carbon(), 2)],
                [
                    InstructionDef::new("merge").acts_on([carbon(), carbon()]),
                    InstructionDef::new("measure")
                        .acts_on([carbon()])
                        .parameter("basis", ParameterKind::Pauli)
                        .outcomes(2),
                ],
            ),
            &[CodeGates::MERGE, CodeGates::MEASURE],
        ),
    ] {
        let built = built.expect("a valid set");
        let map = set.fits(&built).expect("the same set");
        for &opcode in opcodes {
            assert_eq!(map.map(opcode), opcode, "{row}");
        }
        built.fits(&set).expect("the same set, the other way");
    }
}

/// A macro's set whose declaration is faulty panics on every call: a
/// first panic leaves nothing cached for the second. The message names the
/// set and its fault, here the block type it does not list.
#[test]
fn macro_set_with_a_declaration_fault_panics() {
    let _ = Faulty::A;
    for call in ["first", "second"] {
        let panic =
            std::panic::catch_unwind(Faulty::set).expect_err(&format!("the {call} call panics"));
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or_else(|| panic!("the {call} call panics with text"));
        for said in ["`Faulty`", "carbon"] {
            assert!(
                message.contains(said),
                "the {call} call: {message:?} says {said:?}"
            );
        }
    }
}

/// The macro names the set for its type, as its doc promises: undeclared
/// instructions match by mnemonic only between sets of one name.
#[test]
fn macro_set_is_named_for_its_type() {
    assert_eq!(RotationGates::set().name(), "RotationGates");
}

/// A mnemonic not in the set has no opcode, and an opcode past the set's
/// instructions, `RotationGates::MZ`, opcode 3, past `CodeGates`' two
/// instructions, has no mnemonic.
#[test]
fn lookups_outside_the_set_give_none() {
    assert_eq!(RotationGates::set().opcode("t"), None);
    assert_eq!(CodeGates::set().mnemonic(RotationGates::MZ), None);
}

/// A set declares each name once: a block type, a mnemonic, and, in one
/// instruction, a parameter or a flag declared twice is refused.
#[test]
fn a_name_declared_twice_in_a_set_is_refused() {
    for (row, faults, expected) in [
        (
            "a block type",
            BlockInstructionSet::new("Gates", [(carbon(), 2), (carbon(), 3)], [])
                .expect_err("a faulty declaration"),
            DeclarationFault::RepeatedBlockType {
                block_type: carbon(),
            },
        ),
        (
            "a mnemonic",
            faults_of([InstructionDef::new("h"), InstructionDef::new("h")]),
            DeclarationFault::RepeatedMnemonic {
                mnemonic: "h".into(),
            },
        ),
        (
            "a parameter",
            faults_of([InstructionDef::new("u")
                .parameter("theta", ParameterKind::Number)
                .parameter("theta", ParameterKind::Number)]),
            DeclarationFault::RepeatedParameter {
                mnemonic: "u".into(),
                parameter: "theta".into(),
            },
        ),
        (
            "a flag",
            faults_of([InstructionDef::new("m").flags(["leak", "leak"])]),
            DeclarationFault::RepeatedFlag {
                mnemonic: "m".into(),
                flag: "leak".into(),
            },
        ),
    ] {
        assert_eq!(faults, [expected], "{row}");
    }
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

/// A variadic operand is the last, and variadic on both sides: otherwise
/// no block count would say where it ends, or one block would become many,
/// or many one.
#[test]
fn a_variadic_operand_is_last_and_variadic_on_both_sides() {
    let qubits = || BlockOperand::variadic(BlockType::qubit());
    let misplaced = |mnemonic: &str| {
        Err(vec![DeclarationFault::MisplacedVariadic {
            mnemonic: mnemonic.into(),
        }])
    };
    for (row, instruction, declared) in [
        // `mpp` on any number of qubits, then one more: no block count
        // would say where the variadic operand ends.
        (
            "a variadic operand before another",
            InstructionDef::new("mpp").acts_on([qubits(), BlockType::qubit().into()]),
            misplaced("mpp"),
        ),
        // A position variadic on `in` and fixed on `out` would make one
        // block of many.
        (
            "variadic on the input side only",
            InstructionDef::new("fuse")
                .takes([qubits()])
                .gives([BlockType::qubit()]),
            misplaced("fuse"),
        ),
        // And fixed on `in`, variadic on `out`, many blocks of one.
        (
            "variadic on the output side only",
            InstructionDef::new("split")
                .takes([BlockType::qubit()])
                .gives([qubits()]),
            misplaced("split"),
        ),
        (
            "a variadic last operand on both sides",
            InstructionDef::new("mpp")
                .acts_on([qubits()])
                .parameter("p", ParameterKind::Pauli)
                .outcomes(1),
            Ok(()),
        ),
    ] {
        let set = BlockInstructionSet::new("Gates", [(BlockType::qubit(), 1)], [instruction]);
        assert_eq!(set.map(|_| ()), declared, "{row}");
    }
}

/// A flag pattern is a `u64`: 64 flags are accepted, 65 refused.
#[test]
fn more_than_64_flags_are_refused() {
    let flags = |n: usize| (0..n).map(|k| format!("f{k}")).collect::<Vec<_>>();
    BlockInstructionSet::new(
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

/// A batch's readout slots are a `u32`: an instruction whose outcomes and
/// flags together a `u32` counts is accepted, up to `u32::MAX`, and one
/// with one more readout slot refused.
#[test]
fn more_readout_slots_than_a_u32_counts_are_refused() {
    for (outcomes, flags) in [(u32::MAX, vec![]), (u32::MAX - 1, vec!["leak"])] {
        BlockInstructionSet::new(
            "Gates",
            [(BlockType::qubit(), 1)],
            [InstructionDef::new("m").outcomes(outcomes).flags(flags)],
        )
        .expect("u32::MAX readout slots");
    }
    let faults = faults_of([InstructionDef::new("m").outcomes(u32::MAX).flags(["leak"])]);
    assert_eq!(
        faults,
        [DeclarationFault::TooManyReadoutSlots {
            mnemonic: "m".into(),
            outcomes: u32::MAX,
            flags: 1,
        }]
    );
}

/// A sender states its batch's readout slots as the set counts them for
/// its instructions: each one's outcomes and flags, summed; none for an
/// opcode beyond the set, or for a sum more than a `u32` counts.
#[test]
fn a_sets_readout_slots_of_instructions_sum_each_ones_outcomes_and_flags() {
    let set = BlockInstructionSet::new(
        "Readouts",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("h").acts_on([BlockType::qubit()]),
            InstructionDef::new("m")
                .acts_on([BlockType::qubit()])
                .outcomes(1)
                .flags(["reject", "leak"]),
            InstructionDef::new("mpp")
                .acts_on([BlockOperand::variadic(BlockType::qubit())])
                .outcomes(1),
            InstructionDef::new("half").outcomes(1 << 31),
            InstructionDef::new("rest").outcomes((1 << 31) - 1),
        ],
    )
    .expect("a set");
    let instruction = |mnemonic: &str| BlockInstruction {
        opcode: set.opcode(mnemonic).expect("an instruction of the set"),
        blocks: smallvec![],
        arguments: smallvec![],
        select: None,
    };
    let mut beyond = instruction("h");
    beyond.opcode = Opcode::__from_macro(99);
    for (scene, instructions, expected) in [
        ("none", vec![], Some(0)),
        (
            "h, m and mpp",
            vec![instruction("h"), instruction("m"), instruction("mpp")],
            Some(4),
        ),
        (
            "m, then an opcode beyond the set",
            vec![instruction("m"), beyond],
            None,
        ),
        (
            "2^31, then 2^31 - 1",
            vec![instruction("half"), instruction("rest")],
            Some(u32::MAX),
        ),
        ("twice 2^31", vec![instruction("half"); 2], None),
    ] {
        assert_eq!(set.readout_slots_of(&instructions), expected, "{scene}");
    }
}

/// Whoever writes a set fixes it in one pass: faults of the set and of
/// several instructions come together, in declaration order, and an
/// instruction's in the order `BlockInstructionSet::new` lists them, its flags
/// and its readout slots both refused when both overflow.
#[test]
fn every_declaration_fault_is_reported_at_once() {
    let mut flags: Vec<String> = (0..64).map(|k| format!("f{k}")).collect();
    flags.push("f0".into());
    let faults = BlockInstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1), (BlockType::qubit(), 1)],
        [
            InstructionDef::new("m").flags(["leak", "leak"]),
            InstructionDef::new("h").acts_on([carbon()]),
            InstructionDef::new("m").outcomes(u32::MAX).flags(flags),
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
            DeclarationFault::TooManyFlags {
                mnemonic: "m".into(),
                flags: 65,
            },
            DeclarationFault::TooManyReadoutSlots {
                mnemonic: "m".into(),
                outcomes: u32::MAX,
                flags: 65,
            },
            DeclarationFault::RepeatedFlag {
                mnemonic: "m".into(),
                flag: "f0".into(),
            },
        ]
    );
}

/// A device's set over qubits and a code's `carbon` blocks: gates, Stim's
/// `MPP`, a flagged measurement, an encoding that turns a qubit into a
/// block, a preparation that makes a block, and an `x` run if a parity of
/// earlier readouts is odd.
fn device() -> BlockInstructionSet {
    let qubit = || [BlockType::qubit()];
    BlockInstructionSet::new(
        "Device",
        [(BlockType::qubit(), 1), (carbon(), 2)],
        [
            InstructionDef::new("h").acts_on(qubit()),
            InstructionDef::new("rz")
                .acts_on(qubit())
                .parameter("theta", ParameterKind::Number),
            InstructionDef::new("mpp")
                .acts_on([BlockOperand::variadic(BlockType::qubit())])
                .outcomes(1),
            InstructionDef::new("m")
                .acts_on(qubit())
                .outcomes(1)
                .flags(["reject", "leak"]),
            InstructionDef::new("encode")
                .takes(qubit())
                .gives([carbon()]),
            InstructionDef::new("prepare").gives([carbon()]),
            InstructionDef::new("cond_x")
                .acts_on(qubit())
                .parameter("c", ParameterKind::Bit),
        ],
    )
    .expect("a valid set")
}

/// `mnemonic` on `blocks`, with no argument and no flag pattern, as a part
/// writes it on its hot path: unchecked.
fn literal(set: &BlockInstructionSet, mnemonic: &str, blocks: &[u32]) -> BlockInstruction {
    BlockInstruction {
        opcode: set.opcode(mnemonic).expect("an instruction of the set"),
        blocks: blocks.iter().copied().collect(),
        arguments: smallvec![],
        select: None,
    }
}

/// Shots of `qubits` qubits and `carbons` blocks, using no readout
/// parity.
fn shots_of(qubits: u32, carbons: u32) -> BlockCapabilities {
    BlockCapabilities::NONE
        .with_qubits(qubits)
        .with_blocks(carbon(), carbons)
}

/// Checks each row's instruction of `device()` as a receiver checks a
/// checked batch's, against the requirement of the shots it runs on: its
/// readout slots, or its fault.
fn check_rows(
    rows: Vec<(
        &str,
        BlockInstruction,
        BlockCapabilities,
        Result<u32, InstructionFault>,
    )>,
) {
    let set = device();
    for (row, instruction, shots, expected) in rows {
        let requirement = set.requirement(&shots);
        assert_eq!(set.check(&instruction, &requirement), expected, "{row}");
    }
}

/// An instruction its set does not declare, or whose arguments or flag
/// patterns its instruction does not take, is refused; the same
/// instruction, with what it takes, is checked to its readout slots. A
/// readout parity, even of no readout, is taken only when the shots
/// require readout parities; a bit known when written, whatever they
/// require.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn an_instruction_its_set_does_not_take_is_refused() {
    let set = device();
    let mut beyond = literal(&set, "h", &[0]);
    beyond.opcode = Opcode::__from_macro(99);
    let rz = |arguments: &[f64]| {
        let mut rz = literal(&set, "rz", &[0]);
        rz.arguments = arguments.iter().map(|&a| Argument::Number(a)).collect();
        rz
    };
    let m = literal(&set, "m", &[0])
        .select(&set, [[("leak", 0)]])
        .expect("a declared flag");
    let mut h_with_ms_pattern = m.clone();
    h_with_ms_pattern.opcode = set.opcode("h").expect("an instruction of the set");
    let mut h_with_ms_first_flag = literal(&set, "m", &[0])
        .select(&set, [[("reject", 1)]])
        .expect("a declared flag");
    h_with_ms_first_flag.opcode = h_with_ms_pattern.opcode;
    let mut select_of_none = m.clone();
    select_of_none.select = Some(Box::new([]));
    let cond_x = |argument| {
        let mut cond_x = literal(&set, "cond_x", &[0]);
        cond_x.arguments = smallvec![argument];
        cond_x
    };
    let qubit = || shots_of(1, 0);
    let with_parity = || qubit().with_readout_parity(true);
    check_rows(vec![
        (
            "an opcode beyond the set",
            beyond,
            qubit(),
            Err(InstructionFault::UnknownOpcode(Opcode::__from_macro(99))),
        ),
        ("`rz` with its angle", rz(&[0.1]), qubit(), Ok(0)),
        (
            "`rz` with two angles",
            rz(&[0.1, 0.2]),
            qubit(),
            Err(InstructionFault::ExtraArguments {
                mnemonic: "rz".into(),
                expected: 1,
                got: 2,
            }),
        ),
        ("`m` selecting on its flags", m, qubit(), Ok(3)),
        (
            "a pattern of `m`'s flags on `h`, which has none",
            h_with_ms_pattern,
            qubit(),
            Err(InstructionFault::UnknownFlag {
                mnemonic: "h".into(),
                name: "#1".into(),
            }),
        ),
        (
            "a pattern on flag 0 on `h`, just past its no flags",
            h_with_ms_first_flag,
            qubit(),
            Err(InstructionFault::UnknownFlag {
                mnemonic: "h".into(),
                name: "#0".into(),
            }),
        ),
        (
            "a `select` of no patterns, which no shot's flags match",
            select_of_none,
            qubit(),
            Err(InstructionFault::EmptySelect {
                mnemonic: "m".into(),
            }),
        ),
        (
            "`cond_x` on the latest readout",
            cond_x(Argument::recent(1)),
            with_parity(),
            Ok(0),
        ),
        (
            "`cond_x` on a bit known when written, requiring no parity",
            cond_x(Argument::Bit(Bit::Value(true))),
            qubit(),
            Ok(0),
        ),
        (
            "`cond_x` on the parity of no readout, requiring none",
            cond_x(Argument::parity([], true)),
            qubit(),
            Err(InstructionFault::ParityNotRequired {
                mnemonic: "cond_x".into(),
                parameter: "c".into(),
            }),
        ),
        (
            "`cond_x` on the parity of no readout, shots stating parities `false`",
            cond_x(Argument::parity([], true)),
            qubit().with_readout_parity(false),
            Err(InstructionFault::ParityNotRequired {
                mnemonic: "cond_x".into(),
                parameter: "c".into(),
            }),
        ),
        (
            "`cond_x` on the parity of no readout, requiring parities",
            cond_x(Argument::parity([], true)),
            with_parity(),
            Ok(0),
        ),
    ]);
}

/// Each block an instruction names is below the shots' blocks of its
/// type: for one type, for its own type among several, for a type the
/// shots need none of or do not state, for both types of a block that
/// changes type, and for each block of a variadic operand.
#[test]
fn a_block_beyond_the_shots_blocks_of_its_type_is_refused() {
    let set = device();
    let beyond = |mnemonic: &str, block, required| {
        Err(InstructionFault::BlockBeyondRequired {
            mnemonic: mnemonic.into(),
            block,
            required,
        })
    };
    let on = |mnemonic, blocks| literal(&set, mnemonic, blocks);
    check_rows(vec![
        (
            "`h` on the last of 4 qubits",
            on("h", &[3]),
            shots_of(4, 0),
            Ok(0),
        ),
        (
            "`h` on qubit 4 of 4",
            on("h", &[4]),
            shots_of(4, 0),
            beyond("h", 4, 4),
        ),
        (
            "`prepare` making block 2, of 3 blocks and 1 qubit",
            on("prepare", &[2]),
            shots_of(1, 3),
            Ok(0),
        ),
        (
            "`prepare` making block 3, of 3 blocks and 1 qubit",
            on("prepare", &[3]),
            shots_of(1, 3),
            beyond("prepare", 3, 3),
        ),
        (
            "`prepare` making block 0, of shots requiring no block of its type",
            on("prepare", &[0]),
            shots_of(1, 0),
            beyond("prepare", 0, 0),
        ),
        (
            "`h` on qubit 0, of shots not stating qubits",
            on("h", &[0]),
            BlockCapabilities::NONE,
            beyond("h", 0, 0),
        ),
        (
            "`encode` turning qubit 1 into block 1, of 4 qubits and 2 blocks",
            on("encode", &[1]),
            shots_of(4, 2),
            Ok(0),
        ),
        (
            "`encode` turning qubit 2 into block 2, of 4 qubits and 2 blocks",
            on("encode", &[2]),
            shots_of(4, 2),
            beyond("encode", 2, 2),
        ),
        (
            "`mpp` on qubits 0, 1 and 5 of 4",
            on("mpp", &[0, 1, 5]),
            shots_of(4, 0),
            beyond("mpp", 5, 4),
        ),
    ]);
}

/// A sender sums each instruction's readout slots into its batch's, and
/// its receiver checks that sum against what `check` answers for each, so
/// the two agree on every instruction of the set, outcomes and flags alike
/// (`m`, of one outcome and two flags, writes 3); an opcode beyond the
/// set, which `check` refuses, writes none.
#[test]
fn an_instruction_s_readout_slots_are_those_its_check_answers() {
    let set = device();
    let on = |mnemonic, blocks| literal(&set, mnemonic, blocks);
    let mut rz = on("rz", &[0]);
    rz.arguments = smallvec![Argument::Number(0.1)];
    let mut cond_x = on("cond_x", &[0]);
    cond_x.arguments = smallvec![Argument::Bit(Bit::Value(true))];
    let mut beyond = on("m", &[0]);
    beyond.opcode = Opcode::__from_macro(99);
    let requirement = set.requirement(&shots_of(2, 1));
    for (row, instruction, checked) in [
        ("`h`", on("h", &[0]), Ok(0)),
        ("`rz`", rz, Ok(0)),
        ("`mpp` on two qubits", on("mpp", &[0, 1]), Ok(1)),
        ("`m`, of one outcome and two flags", on("m", &[0]), Ok(3)),
        ("`encode`", on("encode", &[0]), Ok(0)),
        ("`prepare`", on("prepare", &[0]), Ok(0)),
        ("`cond_x`", cond_x, Ok(0)),
        (
            "an opcode beyond the set",
            beyond,
            Err(InstructionFault::UnknownOpcode(Opcode::__from_macro(99))),
        ),
    ] {
        assert_eq!(set.check(&instruction, &requirement), checked, "{row}");
        assert_eq!(
            set.readout_slots(&instruction),
            checked.unwrap_or(0),
            "{row}'s readout slots"
        );
    }
}
