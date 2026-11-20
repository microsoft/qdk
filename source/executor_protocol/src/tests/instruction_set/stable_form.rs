// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A set's stable form, with the `serde` feature: what a part sends to
//! prove, across a process boundary, which set it runs. Written and read
//! here as JSON, a self-describing format serde serves.

use super::carbon;
use crate::{
    Angle, BlockOperand, BlockType, Guard, InstructionDef, InstructionSet, Meaning, Observable,
    ParameterKind, Pauli,
};
use serde_json::{Value, json};
use std::f64::consts::FRAC_PI_4;

/// A set of every part a declaration holds: two block types, one of two
/// qubits, a variadic operand, parameters of every kind, outcomes, flags,
/// and meanings of every step, guarded both ways, with an angle and an
/// observable of each kind, and one declared with no step.
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
            InstructionDef::new("i")
                .acts_on([qubit()])
                .meaning(Meaning::declared()),
        ],
    )
    .expect("a valid set")
}

/// A set of one instruction, `rz`, whose form `docs/instruction-sets.md`
/// shows.
fn rz_gates() -> InstructionSet {
    InstructionSet::new(
        "Gates",
        [(BlockType::qubit(), 1)],
        [InstructionDef::new("rz")
            .acts_on([BlockType::qubit()])
            .parameter("theta", ParameterKind::Number)
            .meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(0)))],
    )
    .expect("a valid set")
}

/// `rz_gates`' form, to edit.
fn rz_gates_form() -> Value {
    serde_json::from_str(RZ_GATES_FORM).expect("the golden is JSON")
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

/// Two builds that exchange sets read each other's form: the text a set
/// writes does not change, its fields' order included, so a build reads
/// what an earlier one wrote. Steps, guards and kinds are named as Qodec
/// names them, a Pauli is written as Qodec's text, and every other part
/// by its Rust name.
#[test]
fn a_sets_stable_form_does_not_change() {
    for (row, set, form) in [
        ("a set of one instruction", rz_gates(), RZ_GATES_FORM),
        ("a set of every part", every_part(), EVERY_PART_FORM),
    ] {
        assert_eq!(
            serde_json::to_string_pretty(&set).expect("a set writes its stable form"),
            form,
            "{row}"
        );
        let read: InstructionSet = serde_json::from_str(form).expect("the form reads back");
        assert_eq!(read, set, "{row}");
    }
}

/// A form written by hand reads as the set its declaration builds: its
/// meanings are made canonical by the builder, and what it leaves out is
/// what Qodec leaves out, an instruction's meaning (undeclared) or a
/// step's guard (none).
#[test]
fn a_hand_written_form_reads_as_the_set_its_declaration_builds() {
    let x = || {
        InstructionDef::new("x")
            .acts_on([BlockType::qubit()])
            .parameter("a", ParameterKind::Bit)
            .parameter("b", ParameterKind::Bit)
    };
    let gates = |def: InstructionDef| {
        InstructionSet::new("Gates", [(BlockType::qubit(), 1)], [def]).expect("a valid set")
    };
    let x_0 = || Meaning::declared().pauli(Pauli::x(0));
    for (row, meaning_form, built) in [
        // The terms sorted, and the term given twice cancelled.
        (
            "guard terms out of order and given twice",
            Some(json!([{
                "operation": {"pauli": "X_0"},
                "guard": {"if": [{"parameter": 1}, {"parameter": 0}, {"parameter": 1}]}
            }])),
            x_0().only_if([Guard::Parameter(0)]),
        ),
        // The generator it keeps dropped.
        (
            "a Clifford listing a generator it keeps",
            Some(json!([{
                "operation": {"clifford": [["Z_0", "Z_0"], ["X_0", "Z_0"]]},
                "guard": null
            }])),
            Meaning::declared().clifford([(Pauli::x(0), Pauli::z(0))]),
        ),
        (
            "a step without its guard",
            Some(json!([{"operation": {"pauli": "X_0"}}])),
            x_0(),
        ),
        (
            "an instruction without its meaning",
            None,
            Meaning::undeclared(),
        ),
    ] {
        let mut form = serde_json::to_value(gates(x())).expect("a set writes its stable form");
        let instruction = form["instructions"][0]
            .as_object_mut()
            .expect("an instruction's form is an object");
        match meaning_form {
            Some(meaning_form) => instruction.insert("meaning".into(), meaning_form),
            None => instruction.remove("meaning"),
        };
        let read: InstructionSet = serde_json::from_value(form).expect("the form reads back");
        assert_eq!(read, gates(x().meaning(built)), "{row}");
    }
}

/// A form written by hand, or by a faulty peer, is checked as a built set
/// is: a declaration [`InstructionSet::new`] refuses is refused when it is
/// read, naming the set and each of its faults, those of a meaning
/// included.
#[test]
fn a_faulty_declaration_is_refused_when_read_naming_its_faults() {
    let h = || InstructionDef::new("h").acts_on([BlockType::qubit()]);
    let rotate_by_a_parameter =
        || h().meaning(Meaning::declared().rotate(Pauli::z(0), Angle::Parameter(0)));
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
    for (row, declared, form, fault_count) in [
        (
            "an operand of an undeclared block type",
            vec![InstructionDef::new("merge").acts_on([carbon()])],
            None,
            1,
        ),
        ("a mnemonic declared twice", vec![h(), h()], None, 1),
        (
            "a meaning naming a parameter the instruction lacks",
            vec![rotate_by_a_parameter()],
            None,
            1,
        ),
        (
            "a Clifford mapping a generator twice",
            vec![twice],
            Some(vec![twice_form]),
            1,
        ),
        (
            "a mnemonic declared twice, and a meaning naming a parameter it lacks",
            vec![h(), rotate_by_a_parameter()],
            None,
            2,
        ),
    ] {
        let faults = InstructionSet::new("Gates", [(BlockType::qubit(), 1)], declared.clone())
            .expect_err("a faulty declaration");
        assert_eq!(faults.len(), fault_count, "{row}: the faults `new` finds");
        let form = json!({
            "name": "Gates",
            "block_types": [["qubit", 1]],
            "instructions": form.unwrap_or_else(|| written(&declared)),
        });
        let refusal = serde_json::from_value::<InstructionSet>(form)
            .expect_err("a faulty declaration is refused")
            .to_string();
        assert!(
            refusal.contains("Gates"),
            "{row}: {refusal:?} names the set"
        );
        for fault in faults {
            let fault = fault.to_string();
            assert!(
                refusal.contains(&fault),
                "{row}: {refusal:?} names {fault:?}"
            );
        }
    }
}

/// A form that is not a set's is refused, naming what is wrong: a
/// parameter kind Qodec lacks (`ParameterKind` is plain data, with no test
/// file of its own), or a struct's field the form does not have, which a
/// newer build's form may hold, so that a set it extends is never read as
/// another one.
#[test]
fn a_form_that_is_not_a_sets_is_refused_naming_what_is_wrong() {
    for (row, at, field, value, named) in [
        (
            "a parameter kind Qodec lacks",
            "/instructions/0/parameters/0",
            "kind",
            json!("float"),
            "float",
        ),
        ("a set's field it lacks", "", "version", json!(2), "version"),
        (
            "an instruction's field it lacks",
            "/instructions/0",
            "action",
            json!([]),
            "action",
        ),
        (
            "an operand's field it lacks",
            "/instructions/0/inputs/0",
            "size",
            json!(1),
            "size",
        ),
        (
            "a parameter's field it lacks",
            "/instructions/0/parameters/0",
            "default",
            json!(0),
            "default",
        ),
        (
            "a step's field it lacks",
            "/instructions/0/meaning/0",
            "repeat",
            json!(2),
            "repeat",
        ),
        (
            "a rotation's field it lacks",
            "/instructions/0/meaning/0/operation/rotate",
            "axis",
            json!("z"),
            "axis",
        ),
    ] {
        let mut form = rz_gates_form();
        form.pointer_mut(at)
            .and_then(Value::as_object_mut)
            .expect("a part of the form")
            .insert(field.into(), value);
        let refusal = serde_json::from_value::<InstructionSet>(form)
            .expect_err("a form that is not a set's is refused")
            .to_string();
        assert!(
            refusal.contains(named),
            "{row}: {refusal:?} names {named:?}"
        );
    }
}

/// `rz_gates`' form, as `to_string_pretty` writes it.
const RZ_GATES_FORM: &str = r#"{
  "name": "Gates",
  "block_types": [
    [
      "qubit",
      1
    ]
  ],
  "instructions": [
    {
      "mnemonic": "rz",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [
        {
          "name": "theta",
          "kind": "number"
        }
      ],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "rotate": {
              "pauli": "Z_0",
              "angle": {
                "parameter": 0
              }
            }
          },
          "guard": null
        }
      ]
    }
  ]
}"#;

/// `every_part`'s form, as `to_string_pretty` writes it.
const EVERY_PART_FORM: &str = r#"{
  "name": "Everything",
  "block_types": [
    [
      "qubit",
      1
    ],
    [
      "carbon",
      2
    ]
  ],
  "instructions": [
    {
      "mnemonic": "h",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "clifford": [
              [
                "X_0",
                "Z_0"
              ],
              [
                "Z_0",
                "X_0"
              ]
            ]
          },
          "guard": null
        }
      ]
    },
    {
      "mnemonic": "t",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "rotate": {
              "pauli": "Z_0",
              "angle": {
                "literal": 0.7853981633974483
              }
            }
          },
          "guard": null
        }
      ]
    },
    {
      "mnemonic": "rz",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [
        {
          "name": "theta",
          "kind": "number"
        }
      ],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "rotate": {
              "pauli": "Z_0",
              "angle": {
                "parameter": 0
              }
            }
          },
          "guard": null
        }
      ]
    },
    {
      "mnemonic": "mpp",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": true
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": true
        }
      ],
      "parameters": [
        {
          "name": "p",
          "kind": "pauli"
        }
      ],
      "outcomes": 1,
      "flags": [
        "leak"
      ],
      "meaning": [
        {
          "operation": {
            "observe": [
              {
                "parameter": 0
              }
            ]
          },
          "guard": null
        }
      ]
    },
    {
      "mnemonic": "prepare",
      "inputs": [],
      "outputs": [
        {
          "block_type": "carbon",
          "variadic": false
        }
      ],
      "parameters": [],
      "outcomes": 0,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "stabilize": [
              "X_0 X_1"
            ]
          },
          "guard": null
        }
      ]
    },
    {
      "mnemonic": "measure_correct",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [
        {
          "name": "correction",
          "kind": "bit"
        }
      ],
      "outcomes": 1,
      "flags": [],
      "meaning": [
        {
          "operation": {
            "observe": [
              {
                "pauli": "Z_0"
              }
            ]
          },
          "guard": null
        },
        {
          "operation": {
            "pauli": "X_0"
          },
          "guard": {
            "if": [
              {
                "parameter": 0
              },
              {
                "outcome": 0
              }
            ]
          }
        },
        {
          "operation": {
            "pauli": "-Z_0"
          },
          "guard": {
            "unless": [
              {
                "outcome": 0
              }
            ]
          }
        }
      ]
    },
    {
      "mnemonic": "note",
      "inputs": [],
      "outputs": [],
      "parameters": [
        {
          "name": "n",
          "kind": "integer"
        },
        {
          "name": "on",
          "kind": "boolean"
        },
        {
          "name": "text",
          "kind": "string"
        }
      ],
      "outcomes": 0,
      "flags": [],
      "meaning": null
    },
    {
      "mnemonic": "i",
      "inputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "outputs": [
        {
          "block_type": "qubit",
          "variadic": false
        }
      ],
      "parameters": [],
      "outcomes": 0,
      "flags": [],
      "meaning": []
    }
  ]
}"#;
