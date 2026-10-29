// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::session::{Reply, Session};
use expect_test::{Expect, expect};
use qodec::Qodec;
use qodec_runtime::Bit;
use rustyline::{Context, completion::Completer, history::DefaultHistory};
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::Arc};

/// A repetition-3 layer over a physical layer, with a logical layer on top
/// when `logical` is set.
fn qodec(logical: bool) -> Qodec {
    let (name, logical_layer) = if logical {
        ("logical", LOGICAL_LAYER)
    } else {
        ("repetition3", "")
    };
    let bundle = format!(
        "qodec.yaml:\n  name: {name}\n  layers:\n{logical_layer}{REPETITION3_LAYER}    - instruction_set: physical.isa.yaml\n{LOGICAL_ARTIFACTS}{ARTIFACTS}"
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// A qodec with only the physical layer.
fn physical() -> Qodec {
    let bundle = format!(
        "qodec.yaml:\n  name: physical\n  layers:\n    - instruction_set: physical.isa.yaml\n{ARTIFACTS}"
    );
    Qodec::from_bundle_str(&bundle).expect("bundle should load")
}

/// Evaluates `lines` in a fresh session, and returns the transcript.
fn transcript(qodec: &Qodec, lines: &[&str]) -> String {
    transcript_asking(qodec, lines, &[])
}

/// Like [`transcript`], answering the bits asked for in `:outcomes ask` mode
/// with `answers`, and recording the questions.
fn transcript_asking(qodec: &Qodec, lines: &[&str], answers: &[Bit]) -> String {
    let answers = Rc::new(RefCell::new(
        answers.iter().copied().collect::<VecDeque<_>>(),
    ));
    let asked = Rc::new(RefCell::new(String::new()));
    let ask = {
        let (answers, asked) = (Rc::clone(&answers), Rc::clone(&asked));
        Box::new(move |what: &str| {
            let mut asked = asked.borrow_mut();
            asked.push_str("  ");
            asked.push_str(what);
            asked.push_str("?\n");
            answers
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "no more answers".to_string())
        })
    };
    let mut session = Session::new(qodec, ask).expect("qodec should plan");
    let mut text = String::new();
    for line in lines {
        text.push_str(format!("qodec> {line}").trim_end());
        text.push('\n');
        let reply = session.eval(line);
        text.push_str(&asked.borrow_mut().split_off(0));
        match reply {
            Reply::Print(output) if output.is_empty() => {}
            Reply::Print(output) => {
                text.push_str(output.trim_end());
                text.push('\n');
            }
            Reply::Quit => text.push_str("<quit>\n"),
        }
    }
    text
}

fn check(qodec: &Qodec, lines: &[&str], expect: &Expect) {
    expect.assert_eq(&transcript(qodec, lines));
}

#[test]
fn instructions_lower_to_bottom_instructions() {
    check(
        &qodec(false),
        &["prepare_z 0", "__quantum__qis__m__body 0"],
        &expect![[r#"
            qodec> prepare_z 0
              R 0
              R 1
              R 2
            qodec> __quantum__qis__m__body 0
              M 0  -> [0]
              M 1  -> [0]
              M 2  -> [0]
            __quantum__qis__m__body 0 => [0]
        "#]],
    );
}

#[test]
fn a_line_with_several_instructions_is_one_batch() {
    check(
        &qodec(false),
        &["prepare_z 0; idle 0; __quantum__qis__m__body 0"],
        &expect![[r#"
            qodec> prepare_z 0; idle 0; __quantum__qis__m__body 0
              R 0
              R 1
              R 2
              R 3
              R 4
              CX 0 3
              CX 1 3
              CX 1 4
              CX 2 4
              M 3     -> [0]
              M 4     -> [0]
              M 0     -> [0]
              M 1     -> [0]
              M 2     -> [0]
            __quantum__qis__m__body 0 => [0]
        "#]],
    );
}

#[test]
fn instructions_lower_through_every_layer() {
    check(
        &qodec(true),
        &["prepare 0; measure 0"],
        &expect![[r#"
        qodec> prepare 0; measure 0
          R 0
          R 1
          R 2
          M 0  -> [0]
          M 1  -> [0]
          M 2  -> [0]
        measure 0 => [0]
    "#]],
    );
}

#[test]
fn bottom_outcomes_drive_adaptive_gadgets() {
    check(
        &qodec(false),
        &[
            "prepare_z 0",
            ":outcomes one",
            "measure_reset 0",
            ":outcomes",
            ":outcomes random 7",
        ],
        &expect![[r#"
            qodec> prepare_z 0
              R 0
              R 1
              R 2
            qodec> :outcomes one
            outcomes: one
            qodec> measure_reset 0
              -- lower batch 1 of 2 --
              MZ 0  -> [1]
              MZ 1  -> [1]
              MZ 2  -> [1]
              -- lower batch 2 of 2 --
              XIF 0 c=1
              XIF 1 c=1
              XIF 2 c=1
            measure_reset 0 => [1]
            qodec> :outcomes
            outcomes: one
            qodec> :outcomes random 7
            outcomes: random 7
        "#]],
    );
}

#[test]
fn ask_mode_asks_for_every_bit() {
    expect![[r#"
        qodec> :outcomes ask
        outcomes: ask
        qodec> prepare_z 0; measure_reset 0
          MZ 0: outcome 0?
          MZ 1: outcome 0?
          MZ 2: outcome 0?
          -- lower batch 1 of 2 --
          R 0
          R 1
          R 2
          MZ 0  -> [1]
          MZ 1  -> [0]
          MZ 2  -> [?]
          -- lower batch 2 of 2 --
          XIF 0 c=1
          XIF 1 c=0
          XIF 2 c=0
        measure_reset 0 => [1] (unresolved)
    "#]]
    .assert_eq(&transcript_asking(
        &qodec(false),
        &[":outcomes ask", "prepare_z 0; measure_reset 0"],
        &[Some(true), Some(false), None],
    ));
}

#[test]
fn printed_instructions_can_be_typed_back() {
    check(
        &physical(),
        &["R 0; XIF 0 c=1; MZ 0", "XIF 0 c=true", ":isa"],
        &expect![[r#"
            qodec> R 0; XIF 0 c=1; MZ 0
              R 0
              XIF 0 c=1
              MZ 0       -> [0]
            MZ 0 => [0]
            qodec> XIF 0 c=true
              XIF 0 c=1
            qodec> :isa
            layer 0: physical
              R <out:qubit>
              M <in:qubit>  => 1 outcome
              MZ <qubit>  => 1 outcome
              CX <qubit> <qubit>
              X <qubit>
              Z <qubit>
              XIF <qubit> c=<bit>
        "#]],
    );
}

#[test]
fn reset_starts_a_new_shot() {
    check(
        &qodec(false),
        &[
            "prepare_z 0",
            "__quantum__qis__m__body 0",
            ":reset",
            "__quantum__qis__m__body 0",
        ],
        &expect![[r#"
            qodec> prepare_z 0
              R 0
              R 1
              R 2
            qodec> __quantum__qis__m__body 0
              M 0  -> [0]
              M 1  -> [0]
              M 2  -> [0]
            __quantum__qis__m__body 0 => [0]
            qodec> :reset
            Started a new shot.
            qodec> __quantum__qis__m__body 0
              (no bottom instructions)
            error: layer 0: invalid instruction
              input operands must be live blocks of the declared type; usage:
                __quantum__qis__m__body <in:repetition3>  => 1 outcome
        "#]],
    );
}

#[test]
fn malformed_lines_are_reported() {
    check(
        &physical(),
        &[
            "Y 0",
            "XIF 0",
            "XIF 0 c=2",
            "XIF c=1 0",
            "XIF 0 d=1",
            "XIF zero c=1",
            "XIF 0 c=1 c=0",
            "R \"0",
            ":nope",
            ":isa 9",
        ],
        &expect![[r#"
            qodec> Y 0
            error: `Y` is not an instruction of `physical`; see `:isa`
            qodec> XIF 0
            error: missing argument `c`; usage: XIF <qubit> c=<bit>
            qodec> XIF 0 c=2
            error: argument `c`: expected 0, 1, false or true, got `2`
            qodec> XIF c=1 0
            error: block `0` must come before the arguments; usage: XIF <qubit> c=<bit>
            qodec> XIF 0 d=1
            error: `XIF` has no parameter `d`; usage: XIF <qubit> c=<bit>
            qodec> XIF zero c=1
            error: `zero` is not a block id, and arguments are written `name=value`; usage: XIF <qubit> c=<bit>
            qodec> XIF 0 c=1 c=0
            error: argument `c` is given twice
            qodec> R "0
            error: unterminated `"`
            qodec> :nope
            error: unknown command `:nope`; see `:help`
            qodec> :isa 9
            error: expected top, bottom, or a layer from 0 to 0
        "#]],
    );
}

#[test]
fn commands_describe_the_qodec() {
    check(
        &qodec(true),
        &[":layers", ":isa", ":isa 1", "# a comment", "", ":quit"],
        &expect![[r#"
            qodec> :layers
            layer 0: logical [blocks: lq]  (top)
            layer 1: repetition3 [blocks: repetition3]
            layer 2: physical [blocks: qubit]  (bottom)
            qodec> :isa
            layer 0: logical
              prepare <out:lq>
              measure <in:lq>  => 1 outcome
            qodec> :isa 1
            layer 1: repetition3
              prepare_z <out:repetition3>
              idle <repetition3>
              __quantum__qis__m__body <in:repetition3>  => 1 outcome
              measure_reset <repetition3>  => 1 outcome
              logical_x <repetition3>
              discard <in:repetition3>
            qodec> # a comment
            qodec>
            qodec> :quit
            <quit>
        "#]],
    );
}

#[test]
fn completions_cover_commands_mnemonics_and_parameters() {
    let completions = crate::Completions(Arc::clone(&physical().layers()[0].instruction_set));
    let history = DefaultHistory::new();
    let complete = |line: &str| {
        let (start, candidates) = completions
            .complete(line, line.len(), &Context::new(&history))
            .expect("completes");
        format!("{start} {candidates:?}")
    };
    expect![[r#"
        0 [":outcomes"]
        0 ["X", "XIF"]
        5 ["XIF"]
        6 ["c="]
        6 []
        0 []"#]]
    .assert_eq(
        &[":o", "X", "R 0; XI", "XIF 0 ", "XIF 0 d", "Y"]
            .map(complete)
            .join("\n"),
    );
}

const LOGICAL_LAYER: &str = r"    - instruction_set: logical.isa.yaml
      codes: {lq: trivial.code.yaml}
      gadgets:
        prepare: prepare.gadget.yaml
        measure: measure.gadget.yaml
";

const REPETITION3_LAYER: &str = r"    - instruction_set: repetition3.isa.yaml
      codes: {repetition3: repetition3.code.yaml}
      gadgets:
        prepare_z: prepare_z.gadget.yaml
        idle: idle.gadget.yaml
        __quantum__qis__m__body: m.gadget.yaml
        measure_reset: measure_reset.gadget.yaml
";

const LOGICAL_ARTIFACTS: &str = r#"---
logical.isa.yaml:
  name: logical
  blocks: {lq: 1}
  instructions:
    - {mnemonic: prepare, description: '', out: [lq], action: [stabilize: Z_0]}
    - {mnemonic: measure, description: '', in: [lq], action: [observe: Z_0]}
---
trivial.code.yaml:
  name: trivial
  stabilizers: []
  x: [X_0]
  z: [Z_0]
---
prepare.gadget.yaml:
  circuit: {source: [{prepare_z: [0]}]}
---
measure.gadget.yaml:
  circuit: {source: [{__quantum__qis__m__body: [0]}]}
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
"#;

const ARTIFACTS: &str = r#"---
repetition3.isa.yaml:
  name: repetition3
  blocks: {repetition3: 1}
  instructions:
    - {mnemonic: prepare_z, description: '', out: [repetition3], action: [stabilize: Z_0]}
    - {mnemonic: idle, description: '', in: [repetition3], out: [repetition3]}
    - {mnemonic: __quantum__qis__m__body, description: '', in: [repetition3], action: [observe: Z_0]}
    - {mnemonic: measure_reset, description: '', in: [repetition3], out: [repetition3], action: [observe: Z_0, stabilize: Z_0]}
    - {mnemonic: logical_x, description: '', in: [repetition3], out: [repetition3], action: [pauli: X_0]}
    - {mnemonic: discard, description: '', in: [repetition3]}
---
physical.isa.yaml:
  name: physical
  blocks: {qubit: 1}
  instructions:
    - {mnemonic: R, description: '', out: [qubit], action: [stabilize: Z_0]}
    - {mnemonic: M, description: '', in: [qubit], action: [observe: Z_0]}
    - {mnemonic: MZ, description: '', in: [qubit], out: [qubit], action: [observe: Z_0]}
    - {mnemonic: CX, description: '', in: [qubit, qubit], out: [qubit, qubit], action: [clifford: {X_0: X_0 X_1, Z_1: Z_0 Z_1}]}
    - {mnemonic: X, description: '', in: [qubit], out: [qubit], action: [pauli: X_0]}
    - {mnemonic: Z, description: '', in: [qubit], out: [qubit], action: [pauli: Z_0]}
    - {mnemonic: XIF, description: '', in: [qubit], out: [qubit], parameters: {c: bit}, action: [{pauli: X_0, if: [c]}]}
---
repetition3.code.yaml:
  name: repetition3
  stabilizers: [Z_0 Z_1, Z_1 Z_2]
  x: [X_0 X_1 X_2]
  z: [Z_0]
---
prepare_z.gadget.yaml:
  circuit: {source: [{R: [0]}, {R: [1]}, {R: [2]}]}
---
idle.gadget.yaml:
  circuit:
    source: [{R: [3]}, {R: [4]}, {CX: [0, 3]}, {CX: [1, 3]}, {CX: [1, 4]}, {CX: [2, 4]}, {M: [3]}, {M: [4]}]
  checks:
    - ["circuit.readouts[0]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "in[0].stabilizers[1]"]
    - ["circuit.readouts[0]", "out[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "out[0].stabilizers[1]"]
---
m.gadget.yaml:
  circuit: {source: [{M: [0]}, {M: [1]}, {M: [2]}]}
  checks:
    - ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
---
measure_reset.gadget.yaml:
  circuit:
    source:
      - MZ: [0]
      - MZ: [1]
      - MZ: [2]
      - XIF: [0, {c: "circuit.readouts[0]"}]
      - XIF: [1, {c: "circuit.readouts[1]"}]
      - XIF: [2, {c: "circuit.readouts[2]"}]
  checks:
    - ["circuit.readouts[0]", "circuit.readouts[1]", "in[0].stabilizers[0]"]
    - ["circuit.readouts[1]", "circuit.readouts[2]", "in[0].stabilizers[1]"]
  readouts: [["circuit.readouts[0]", "in[0].z[0]"]]
"#;
