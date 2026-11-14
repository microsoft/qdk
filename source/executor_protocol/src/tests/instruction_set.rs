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
    InstructionSet::new("Gates", [(BlockType::qubit(), 1)], instructions)
        .expect_err("a faulty declaration")
}

#[test]
fn macro_constants_are_the_opcodes_of_their_mnemonics() {
    let set = RotationGates::set();
    for (opcode, mnemonic) in [
        (RotationGates::H, "h"),
        (RotationGates::RZ, "rz"),
        (RotationGates::CX, "cx"),
        (RotationGates::MZ, "mz"),
    ] {
        assert_eq!(set.opcode(mnemonic), Some(opcode), "{mnemonic}");
        assert_eq!(set.mnemonic(opcode), Some(mnemonic));
    }
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

/// A device's set merged with a code's may hold hundreds of instructions:
/// the macro numbers each in place, the last too.
#[test]
fn macro_numbers_a_set_of_hundreds_of_instructions() {
    let set = ManyGates::set();
    for (opcode, mnemonic) in [
        (ManyGates::I000, "i000"),
        (ManyGates::I127, "i127"),
        (ManyGates::I128, "i128"),
        (ManyGates::I199, "i199"),
    ] {
        assert_eq!(set.opcode(mnemonic), Some(opcode), "{mnemonic}");
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

/// `RotationGates`, read at run time.
fn coin_gates_loaded() -> InstructionSet {
    load("RotationGates", "h 1 - 0; rz 1 theta 0; cx 2 - 0; mz 1 - 1")
}

/// A set known only at run time is the set the macro writes: they fit
/// each other, each opcode to itself.
#[test]
fn set_made_at_run_time_fits_as_a_static_one() {
    let loaded = coin_gates_loaded();
    let map = RotationGates::set().fits(&loaded).expect("the same set");
    for opcode in [
        RotationGates::H,
        RotationGates::RZ,
        RotationGates::CX,
        RotationGates::MZ,
    ] {
        assert_eq!(map.map(opcode), opcode);
    }
    loaded
        .fits(&RotationGates::set())
        .expect("the same set, the other way");
}

/// A part that names its instructions at run time writes the opcodes a
/// part written in code matches on.
#[test]
fn instruction_built_by_name_on_a_run_time_set_holds_the_static_constant() {
    let rz = coin_gates_loaded()
        .instruction("rz", [3], &[("theta", 0.25.into())])
        .expect("a valid instruction");
    assert_eq!(rz.opcode, RotationGates::RZ);
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

/// On every call: a first panic leaves nothing cached for the second.
#[test]
fn macro_set_with_a_declaration_fault_panics() {
    let _ = Faulty::A;
    for call in ["first", "second"] {
        let panic =
            std::panic::catch_unwind(Faulty::set).expect_err(&format!("the {call} call panics"));
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied());
        assert!(
            message.is_some_and(|m| m.contains("`Faulty` declares a faulty set")),
            "the {call} call: {message:?}"
        );
    }
}

#[test]
fn macro_set_is_named_for_its_type() {
    assert_eq!(RotationGates::set().name(), "RotationGates");
}

#[test]
fn opcode_of_an_unknown_mnemonic_is_none() {
    assert_eq!(RotationGates::set().opcode("t"), None);
}

#[test]
fn mnemonic_beyond_the_set_is_none() {
    assert_eq!(CodeGates::set().mnemonic(RotationGates::MZ), None);
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

/// An answer's width is a `u32`: an instruction whose outcomes and flags
/// together a `u32` counts is accepted, up to `u32::MAX`, and one with
/// one more readout slot refused.
#[test]
fn more_readout_slots_than_a_u32_counts_are_refused() {
    for (outcomes, flags) in [(u32::MAX, vec![]), (u32::MAX - 1, vec!["leak"])] {
        InstructionSet::new(
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
