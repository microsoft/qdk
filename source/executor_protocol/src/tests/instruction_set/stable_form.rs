// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A set's stable form, with the `serde` feature: what a part sends to
//! prove, across a process boundary, which set it runs. Written and read
//! here as JSON, one of the formats serde serves.

use super::carbon;
use crate::{
    Angle, BlockOperand, BlockType, Guard, InstructionDef, InstructionSet, Meaning, Observable,
    ParameterKind, Pauli,
};
use serde_json::json;
use std::f64::consts::FRAC_PI_4;

/// A set of every part a declaration holds: two block types, a variadic
/// operand, parameters of every kind, outcomes, flags, and meanings of
/// every step, guarded both ways, with an angle of each kind.
fn every_part() -> InstructionSet {
    let qubit = || BlockType::qubit();
    InstructionSet::new(
        "Everything",
        [(qubit(), 1), (carbon(), 2)],
        [
            InstructionDef::new("h").acts_on([qubit()]).meaning(
                Meaning::declared()
                    .clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::z(0), Pauli::x(0))]),
            ),
            InstructionDef::new("t")
                .acts_on([qubit()])
                .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Literal(FRAC_PI_4))),
            InstructionDef::new("rz")
                .acts_on([qubit()])
                .parameter("theta", ParameterKind::Number)
                .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(0))),
            InstructionDef::new("mpp")
                .acts_on([BlockOperand::variadic(qubit())])
                .parameter("p", ParameterKind::Pauli)
                .outcomes(1)
                .flags(["leak"])
                .meaning(Meaning::declared().observe([Observable::Parameter(0)])),
            InstructionDef::new("prepare")
                .gives([carbon()])
                .meaning(Meaning::declared().stabilize([Pauli::x(0) * Pauli::x(1)])),
            InstructionDef::new("measure_correct")
                .acts_on([qubit()])
                .parameter("correction", ParameterKind::Bit)
                .outcomes(1)
                .meaning(
                    Meaning::declared()
                        .observe([Pauli::z(0)])
                        .pauli(Pauli::x(0))
                        .only_if([Guard::Parameter(0), Guard::Outcome(0)])
                        .pauli(-Pauli::z(0))
                        .unless([Guard::Outcome(0)]),
                ),
            InstructionDef::new("note")
                .parameter("n", ParameterKind::Integer)
                .parameter("on", ParameterKind::Boolean)
                .parameter("text", ParameterKind::Text),
        ],
    )
    .expect("a valid set")
}

/// The far end of a link decodes the near end's set and compares it with
/// its own: a set read back from its stable form is equal to the one
/// written, and what it derives (its opcodes, its readout slots) is
/// derived again.
#[test]
fn a_set_read_back_from_its_stable_form_is_equal_to_the_one_written() {
    let set = every_part();
    let text = serde_json::to_string(&set).expect("a set writes its stable form");
    let read: InstructionSet = serde_json::from_str(&text).expect("the form reads back");
    assert_eq!(read, set);
    let mpp = set.opcode("mpp").expect("an instruction of the set");
    assert_eq!(read.opcode("mpp"), Some(mpp));
    assert_eq!(read.readout_slots(mpp), Some(2));
}

/// Two builds that exchange sets read each other's form: the form a set
/// writes does not change, so a build reads what an earlier one wrote.
/// Each step, guard and kind is written in Qodec's words.
#[test]
fn a_sets_stable_form_does_not_change() {
    let set = InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("rz")
                .acts_on([BlockType::qubit()])
                .parameter("theta", ParameterKind::Number)
                .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(0))),
            InstructionDef::new("mz")
                .acts_on([BlockOperand::variadic(BlockType::qubit())])
                .parameter("c", ParameterKind::Bit)
                .outcomes(1)
                .flags(["leak"])
                .meaning(
                    Meaning::declared()
                        .observe([Pauli::z(0)])
                        .pauli(-Pauli::x(0))
                        .only_if([Guard::Parameter(0)]),
                ),
            InstructionDef::new("i"),
        ],
    )
    .expect("a valid set");
    let form = json!({
        "name": "Gates",
        "block_types": [["qubit", 1]],
        "instructions": [
            {
                "mnemonic": "rz",
                "inputs": [{"block_type": "qubit", "variadic": false}],
                "outputs": [{"block_type": "qubit", "variadic": false}],
                "parameters": [{"name": "theta", "kind": "number"}],
                "outcomes": 0,
                "flags": [],
                "meaning": [
                    {
                        "operation": {"rotate": {"pauli": "Z_0", "angle": {"parameter": 0}}},
                        "guard": null
                    }
                ]
            },
            {
                "mnemonic": "mz",
                "inputs": [{"block_type": "qubit", "variadic": true}],
                "outputs": [{"block_type": "qubit", "variadic": true}],
                "parameters": [{"name": "c", "kind": "bit"}],
                "outcomes": 1,
                "flags": ["leak"],
                "meaning": [
                    {"operation": {"observe": [{"pauli": "Z_0"}]}, "guard": null},
                    {"operation": {"pauli": "-X_0"}, "guard": {"if": [{"parameter": 0}]}}
                ]
            },
            {
                "mnemonic": "i",
                "inputs": [],
                "outputs": [],
                "parameters": [],
                "outcomes": 0,
                "flags": [],
                "meaning": null
            }
        ]
    });
    assert_eq!(
        serde_json::to_value(&set).expect("a set writes its stable form"),
        form
    );
    let read: InstructionSet = serde_json::from_value(form).expect("the form reads back");
    assert_eq!(read, set);
}

/// A form written by hand, or by a faulty peer, is checked as a built set
/// is: a declaration [`InstructionSet::new`] refuses is refused when it is
/// read, naming each of its faults, those of a meaning included.
#[test]
fn a_faulty_declaration_is_refused_when_read_naming_its_faults() {
    let h = || InstructionDef::new("h").acts_on([BlockType::qubit()]);
    let written = |instructions: &[InstructionDef]| {
        instructions
            .iter()
            .map(|def| serde_json::to_value(def).expect("an instruction writes its form"))
            .collect::<Vec<_>>()
    };
    let twice = h().meaning(
        Meaning::declared().clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::x(0), Pauli::y(0))]),
    );
    let mut twice_form = serde_json::to_value(h()).expect("an instruction writes its form");
    twice_form["meaning"] = json!([
        {"operation": {"clifford": [["X_0", "Z_0"], ["X_0", "Y_0"]]}, "guard": null}
    ]);
    for (row, declared, form) in [
        (
            "an operand of an undeclared block type",
            vec![InstructionDef::new("merge").acts_on([carbon()])],
            None,
        ),
        ("a mnemonic declared twice", vec![h(), h()], None),
        (
            "a meaning naming a parameter the instruction lacks",
            vec![h().meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(0)))],
            None,
        ),
        (
            "a Clifford mapping a generator twice",
            vec![twice],
            Some(vec![twice_form]),
        ),
    ] {
        let faults = InstructionSet::new("Gates", [(BlockType::qubit(), 1)], declared.clone())
            .expect_err("a faulty declaration");
        let form = json!({
            "name": "Gates",
            "block_types": [["qubit", 1]],
            "instructions": form.unwrap_or_else(|| written(&declared)),
        });
        let refusal = serde_json::from_value::<InstructionSet>(form)
            .expect_err("a faulty declaration is refused")
            .to_string();
        for fault in faults {
            let fault = fault.to_string();
            assert!(
                refusal.contains(&fault),
                "{row}: {refusal:?} names {fault:?}"
            );
        }
    }
}
