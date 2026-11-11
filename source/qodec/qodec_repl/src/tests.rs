// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    highlight,
    session::{Reply, Session},
    style,
};
use expect_test::{Expect, expect};
use qodec::Qodec;
use qodec_runtime_recipes::Bit;
use rustyline::{Context, completion::Completer, history::DefaultHistory};
use std::{cell::RefCell, collections::VecDeque, fmt::Write, rc::Rc, sync::Arc};

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
    play(&mut session, lines, &asked)
}

/// Evaluates `lines` in `session`, and returns the transcript, with what
/// `side` gathers during each line after the line.
fn play(session: &mut Session, lines: &[&str], side: &Rc<RefCell<String>>) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(format!("qodec> {line}").trim_end());
        text.push('\n');
        let reply = session.eval(line);
        text.push_str(&side.borrow_mut().split_off(0));
        match reply {
            Reply::Print(output) if output.is_empty() => {}
            Reply::Print(output) => {
                text.push_str(output.trim_end());
                text.push('\n');
            }
            Reply::Quit => text.push_str("<quit>\n"),
        }
    }
    style::plain(&text)
}

/// `text` with every style replaced by a readable `<name>` tag, and resets by
/// `</>`. Styles that render identically share a tag, e.g. `<block|value>`.
fn tagged(text: &str) -> String {
    let mut named: Vec<(String, Vec<&str>)> = Vec::new();
    for (role, s) in style::NAMED {
        let code = s.render().to_string();
        match named.iter_mut().find(|(c, _)| *c == code) {
            Some((_, roles)) => roles.push(role),
            None => named.push((code, vec![role])),
        }
    }
    // Longer sequences first: a bold color starts with plain bold.
    named.sort_by_key(|(code, _)| std::cmp::Reverse(code.len()));
    let mut text = text.replace(&anstyle::Reset.render().to_string(), "</>");
    for (code, roles) in named {
        text = text.replace(&code, &format!("<{}>", roles.join("|")));
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
            error: layer 0: invalid instruction: block 0 is not live
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
            "XIF 0 c=[1, 0]",
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
            qodec> XIF 0 c=[1, 0]
            error: argument `c`: parameters take a single value, not a list
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
fn output_is_styled_and_aligned_by_visible_width() {
    let mut session = Session::new(&qodec(false), Box::new(|_: &str| Ok(None))).expect("plans");
    session.eval(":outcomes one");
    let Reply::Print(text) = session.eval("prepare_z 0; measure_reset 0") else {
        panic!("expected output");
    };
    expect![[r#"
          <dim>-- lower batch 1 of 2 --</>
          <mnemonic|command>R</> <block|value>0</>
          <mnemonic|command>R</> <block|value>1</>
          <mnemonic|command>R</> <block|value>2</>
          <mnemonic|command>MZ</> <block|value>0</>  <dim>-></> [<bit>1</>]
          <mnemonic|command>MZ</> <block|value>1</>  <dim>-></> [<bit>1</>]
          <mnemonic|command>MZ</> <block|value>2</>  <dim>-></> [<bit>1</>]
          <dim>-- lower batch 2 of 2 --</>
          <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>1</>
          <mnemonic|command>XIF</> <block|value>1</> <param>c</>=<block|value>1</>
          <mnemonic|command>XIF</> <block|value>2</> <param>c</>=<block|value>1</>
        <mnemonic|command>measure_reset</> <block|value>0</> <dim>=></> [<bit>1</>]
    "#]]
    .assert_eq(&tagged(&text));
    let Reply::Print(text) = session.eval("measure_reset 7") else {
        panic!("expected output");
    };
    expect![[r#"
          <dim>(no bottom instructions)</>
        <error>error:</> layer 0: invalid instruction: block 7 is not live
          input operands must be live blocks of the declared type; usage:
            <mnemonic|command>measure_reset</> <block|value><repetition3></>  <dim>=></> 1 outcome"#]]
    .assert_eq(&tagged(&text));
}

#[test]
fn signatures_and_layers_are_styled() {
    let mut session = Session::new(&physical(), Box::new(|_: &str| Ok(None))).expect("plans");
    let Reply::Print(text) = session.eval(":isa") else {
        panic!("expected output");
    };
    expect![[r#"
        layer 0: <name>physical</>
          <mnemonic|command>R</> <block|value><out:qubit></>
          <mnemonic|command>M</> <block|value><in:qubit></>  <dim>=></> 1 outcome
          <mnemonic|command>MZ</> <block|value><qubit></>  <dim>=></> 1 outcome
          <mnemonic|command>CX</> <block|value><qubit></> <block|value><qubit></>
          <mnemonic|command>X</> <block|value><qubit></>
          <mnemonic|command>Z</> <block|value><qubit></>
          <mnemonic|command>XIF</> <block|value><qubit></> <param>c</>=<block|value><bit></>"#]]
    .assert_eq(&tagged(&text));
}

/// Highlights `lines` against the physical ISA, one per output line.
fn highlighted(lines: &[&str]) -> String {
    let isa = Arc::clone(&physical().layers()[0].instruction_set);
    lines
        .iter()
        .map(|line| {
            let styled = highlight::line(&isa, line);
            // rustyline requires the display width to be unchanged.
            assert_eq!(
                style::plain(&styled),
                *line,
                "highlighting changed `{line}`"
            );
            tagged(&styled)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn typed_instructions_are_highlighted() {
    expect![[r#"
        <mnemonic|command>R</> <block|value>0</><dim>;</> <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>1</><dim>;</><mnemonic|command>MZ</> <block|value>0</>
          <mnemonic|command>XIF</>  <block|value>0</>   <param>c</>=<block|value>true</>
        <mnemonic|command>XIF</> <block|value>0</> <invalid>d</>=<block|value>1</>
        <mnemonic|command>XIF</> <invalid>zero</>
        <invalid>Y</> <block|value>0</>"#]].assert_eq(&highlighted(&[
        "R 0; XIF 0 c=1;MZ 0",
        "  XIF  0   c=true",
        "XIF 0 d=1",
        "XIF zero",
        "Y 0",
    ]));
}

#[test]
fn half_typed_lines_are_not_flagged() {
    expect![[r#"
        <mnemonic|command>X</>
        XI
        <mnemonic|command>XIF</> <block|value>0</> c
        <mnemonic|command>XIF</> <block|value>0</> <param>c</>="#]]
    .assert_eq(&highlighted(&["X", "XI", "XIF 0 c", "XIF 0 c="]));
}

#[test]
fn quotes_and_brackets_stay_in_one_word() {
    expect![[r#"
        <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>"a; b"</>
        <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>[1, 2]</><dim>;</> <mnemonic|command>R</> <block|value>1</>
        <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>"unterminated; R 1</>
        <mnemonic|command>XIF</> <block|value>0</> <param>c</>=<block|value>]]</> <invalid>R</>"#]].assert_eq(&highlighted(&[
        "XIF 0 c=\"a; b\"",
        "XIF 0 c=[1, 2]; R 1",
        "XIF 0 c=\"unterminated; R 1",
        "XIF 0 c=]] R",
    ]));
}

#[test]
fn commands_and_comments_are_highlighted() {
    expect![[r#"
        <mnemonic|command>:isa</> <block|value>bottom</>
        :outc
        <invalid>:nope</>
        <mnemonic|command>:q</>
        <dim>  # R 0</>
    "#]]
    .assert_eq(&highlighted(&[
        ":isa bottom",
        ":outc",
        ":nope",
        ":q",
        "  # R 0",
        "",
    ]));
}

#[test]
fn arguments_select_colors_and_the_qodec_file() {
    use crate::{Args, parse_args};
    let parse =
        |args: &[&str]| match parse_args(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
        {
            Ok(Args::Help) => "help".to_string(),
            Ok(Args::Run { path, color }) => format!("{path} {color:?}"),
            Err(e) => format!("error: {e}"),
        };
    expect![[r#"
        q.yaml Auto
        q.yaml Never
        q.yaml Always
        q.yaml Auto
        error: `--color` expects auto, always or never, got `sometimes`
        error: `--color` needs a value
        error: unknown option `-x`
        error: expected one qodec file
        error: missing the qodec file
        help"#]]
    .assert_eq(
        &[
            &["q.yaml"][..],
            &["--color=never", "q.yaml"],
            &["q.yaml", "--color", "always"],
            &["--color=auto", "q.yaml"],
            &["--color=sometimes", "q.yaml"],
            &["--color"],
            &["-x", "q.yaml"],
            &["a.yaml", "b.yaml"],
            &[],
            &["q.yaml", "--help"],
        ]
        .map(parse)
        .join("\n"),
    );
}

#[test]
fn ill_typed_instructions_leave_the_shot_intact() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let qodec = Qodec::load(dir.join("c422-c832-arch/qodec.yaml")).expect("example should load");
    let text = transcript(
        &qodec,
        &["prepare_z_all_c422 0", "ccz_c832 0", "h_all_c422 0"],
    );
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with("  ")).collect();
    expect![[r#"
        qodec> prepare_z_all_c422 0
        prepare_z_all_c422 0 => [] flags [reject=0]
        qodec> ccz_c832 0
        error: layer 0: invalid instruction: operand has type `c422`, expected `c832`
        qodec> h_all_c422 0"#]]
    .assert_eq(&lines.join("\n"));
    assert!(text.ends_with("H 3\n"), "`h_all_c422 0` lowers:\n{text}");
}

#[test]
fn examples_lower_their_programs() {
    let examples = [
        (
            "bacon-shor/bacon-shor.qodec.yaml",
            "prepare_z 0; idle 0; measure_z 0",
        ),
        (
            "bb-72/bb-72.qodec.yaml",
            "prepare_x_all 0; idle 0; measure_x_all 0",
        ),
        (
            "c422-c832-arch/qodec.yaml",
            "prepare_z_all_c832 0; ccz_c832 0; teleport_c832_to_c422 0 1; measure_z_all_c422 1",
        ),
        (
            "c4c6/qodec.yaml",
            "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0",
        ),
        ("distillation-15/distillation-15.qodec.yaml", "distill_t 0"),
        (
            "five_qubit/five_qubit.qodec.yaml",
            "prepare_z 0; idle 0; cycle 0; measure_x 0",
        ),
        (
            "honeycomb/honeycomb.qodec.yaml",
            "prepare_red_all 0; round_green 0; round_blue 0; round_red 0; measure_red_all 0",
        ),
        (
            "iceberg/iceberg.qodec.yaml",
            "prepare_z_all 0; idle 0; measure_z_all 0",
        ),
        (
            "reed-muller-15/reed-muller-15.qodec.yaml",
            "prepare_x 0; t 0; idle 0; measure_x 0",
        ),
        (
            "repetition3/repetition3.qodec.yaml",
            "prepare_z 0; idle 0; rotate_z 0 theta=0.5; measure_z 0",
        ),
        (
            "steane/steane.qodec.yaml",
            "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
        ),
        (
            "surface/surface.qodec.yaml",
            "prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0",
        ),
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut text = String::new();
    for (path, program) in examples {
        let qodec = Qodec::load(dir.join(path)).expect("example should load");
        // Bottom instructions are indented; keep only the top-level lines.
        for line in transcript(&qodec, &[program]).lines() {
            if !line.starts_with(' ') {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    expect![[r#"
        qodec> prepare_z 0; idle 0; measure_z 0
        measure_z 0 => [0]
        qodec> prepare_x_all 0; idle 0; measure_x_all 0
        measure_x_all 0 => [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        qodec> prepare_z_all_c832 0; ccz_c832 0; teleport_c832_to_c422 0 1; measure_z_all_c422 1
        teleport_c832_to_c422 0 1 => [0, 0]
        measure_z_all_c422 1 => [0, 0]
        qodec> prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0
        prepare_x_all 0 => [] flags [reject=0]
        measure_z_all 0 => [0, 0] (unresolved)
        qodec> distill_t 0
        distill_t 0 => [] flags [reject_1=0, reject_2=0, reject_3=0, reject_4=0]
        qodec> prepare_z 0; idle 0; cycle 0; measure_x 0
        measure_x 0 => [0]
        qodec> prepare_red_all 0; round_green 0; round_blue 0; round_red 0; measure_red_all 0
        measure_red_all 0 => [0, 0]
        qodec> prepare_z_all 0; idle 0; measure_z_all 0
        idle 0 => [] flags [detected_x=0, detected_z=0]
        measure_z_all 0 => [0, 0, 0, 0]
        qodec> prepare_x 0; t 0; idle 0; measure_x 0
        measure_x 0 => [0]
        qodec> prepare_z 0; idle 0; rotate_z 0 theta=0.5; measure_z 0
        measure_z 0 => [0]
        qodec> prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1
        idle_ft 0 => [] flags [flag_0=0, flag_1=0, flag_2=0, flag_3=0, flag_4=0, flag_5=0]
        measure_z 0 => [0]
        measure_x 1 => [0]
        qodec> prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0
        merge_zz 0 1 => [0]
        measure_merged_z 0 => [0]
    "#]]
    .assert_eq(&text);
}

#[test]
fn completions_cover_commands_mnemonics_and_parameters() {
    let session = Session::new(&physical(), Box::new(|_: &str| Ok(None))).expect("plans");
    let completions = crate::Completions::new(&session);
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
        0 []
        6 ["qodec", "qodec.yaml"]
        6 ["physical.isa.yaml"]
        5 []"#]]
    .assert_eq(
        &[
            ":o", "X", "R 0; XI", "XIF 0 ", "XIF 0 d", "Y", ":yaml q", ":edit ph", ":isa ph",
        ]
        .map(complete)
        .join("\n"),
    );
}

/// A session of `qodec`, whose bottom backend returns zeros.
fn session(qodec: &Qodec) -> Session {
    Session::new(qodec, Box::new(|_: &str| Ok(None))).expect("qodec should plan")
}

/// An empty scratch directory for `test`, and how transcripts show it.
fn scratch(test: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("qodec-repl-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creates the scratch directory");
    let shown = dir.display().to_string();
    (dir, shown)
}

#[test]
fn inspection_commands_describe_gadgets_and_codes() {
    check(
        &qodec(true),
        &[
            ":gadgets",
            ":gadgets bottom",
            ":gadget prepare_z",
            ":codes",
            ":code repetition3 1",
            ":yaml measure",
            ":gadget nope",
            ":code nope",
            ":yaml nope",
            ":gadget",
        ],
        &expect![[r#"
            qodec> :gadgets
            layer 0: logical -> repetition3
              measure  (trivial) -> ()  1 call, 0 checks
              prepare  () -> (trivial)  1 call, 0 checks
            layer 1: repetition3 -> physical
              __quantum__qis__m__body  (repetition3) -> ()  3 calls, 2 checks
              idle                     (repetition3) -> (repetition3)  8 calls, 4 checks
              measure_reset            (repetition3) -> (repetition3)  6 calls, 2 checks
              prepare_z                () -> (repetition3)  3 calls, 0 checks
            qodec> :gadgets bottom
            layer 2: physical
              (no gadgets: nothing lies below)
            qodec> :gadget prepare_z
            layer 1: repetition3 -> physical: gadget prepare_z (0 checks)
              circuit:
                source: |
                  - R:
                    - 0
                  - R:
                    - 1
                  - R:
                    - 2
                format: yaml
                out:
                  '0': qubit
                  '1': qubit
                  '2': qubit
              out:
              - repetition3: [0, 1, 2]
            qodec> :codes
            layer 0: logical -> repetition3
              lq  trivial [[1,1]], 0 stabilizers
            layer 1: repetition3 -> physical
              repetition3  repetition3 [[3,1]], 2 stabilizers
            qodec> :code repetition3 1
            layer 1: repetition3 -> physical: code repetition3 for repetition3 [[3,1]], 2 stabilizers
              name: repetition3
              stabilizers: [Z_0 Z_1, Z_1 Z_2]
              x: [X_0 X_1 X_2]
              z: [Z_0]
            qodec> :yaml measure
            ---
            measure.gadget.yaml:
              circuit:
              - __quantum__qis__m__body: [0]
              readouts:
              - ['circuit.readouts[0]', 'in[0].z[0]']
            qodec> :gadget nope
            error: no gadget `nope`; `:gadgets` lists them
            qodec> :code nope
            error: no code `nope`; `:codes` lists them
            qodec> :yaml nope
            error: no item `nope`; `:yaml` lists the qodec's documents
            qodec> :gadget
            error: usage: :gadget <mnemonic> [<layer>]
        "#]],
    );
}

#[test]
fn the_whole_qodec_prints_as_a_bundle_that_loads() {
    let mut session = session(&qodec(false));
    let Reply::Print(bundle) = session.eval(":yaml") else {
        panic!("prints")
    };
    let reloaded = Qodec::from_bundle_str(&bundle).expect("the bundle loads");
    assert_eq!(reloaded, qodec(false));
}

#[test]
fn set_and_unset_change_an_item_and_undo_reverts_them() {
    check(
        &qodec(false),
        &[
            "prepare_z 0",
            ":set prepare_z circuit[3] {X: [0]}",
            "prepare_z 0",
            ":set prepare_z circuit[0] {NOPE: [0]}",
            ":unset prepare_z circuit[0]",
            "prepare_z 0",
            ":undo",
            "prepare_z 0",
            ":undo",
            ":undo",
            ":set prepare_z circuit[9] x",
            ":set prepare_z circuit..source x",
            ":set prepare_z",
        ],
        &expect![[r#"
            qodec> prepare_z 0
              R 0
              R 1
              R 2
            qodec> :set prepare_z circuit[3] {X: [0]}
            Set `circuit[3]` in `prepare_z.gadget.yaml`. The qodec is `repetition3`: repetition3 -> physical.
            Started a new shot.
            qodec> prepare_z 0
              R 0
              R 1
              R 2
              X 0
            qodec> :set prepare_z circuit[0] {NOPE: [0]}
            error: the qodec does not plan: layer 0: gadget `prepare_z`: call to unknown instruction "NOPE" in instruction set "physical"; it is as it was
            qodec> :unset prepare_z circuit[0]
            Removed `circuit[0]` in `prepare_z.gadget.yaml`. The qodec is `repetition3`: repetition3 -> physical.
            Started a new shot.
            qodec> prepare_z 0
              R 0
              R 1
              X 2
            qodec> :undo
            Undid the last change. The qodec is `repetition3`: repetition3 -> physical.
            Started a new shot.
            qodec> prepare_z 0
              R 0
              R 1
              R 2
              X 0
            qodec> :undo
            Undid the last change. The qodec is `repetition3`: repetition3 -> physical.
            Started a new shot.
            qodec> :undo
            error: there is no change to undo
            qodec> :set prepare_z circuit[9] x
            error: index 9 is past the end of a list of 3
            qodec> :set prepare_z circuit..source x
            error: bad path `circuit..source`: empty key
            qodec> :set prepare_z
            error: usage: :set <item> <path> <yaml>
        "#]],
    );
}

#[test]
fn replay_replays_the_shot_on_the_changed_qodec() {
    check(
        &qodec(false),
        &[
            ":replay",
            ":replay on",
            "prepare_z 0",
            "measure_reset 0",
            ":set prepare_z circuit[3] {X: [0]}",
            ":drop top",
            ":replay maybe",
        ],
        &expect![[r#"
            qodec> :replay
            replay: off; a change starts a new shot
            qodec> :replay on
            replay: on; a change replays this shot's lines
            qodec> prepare_z 0
              R 0
              R 1
              R 2
            qodec> measure_reset 0
              -- lower batch 1 of 2 --
              MZ 0  -> [0]
              MZ 1  -> [0]
              MZ 2  -> [0]
              -- lower batch 2 of 2 --
              XIF 0 c=0
              XIF 1 c=0
              XIF 2 c=0
            measure_reset 0 => [0]
            qodec> :set prepare_z circuit[3] {X: [0]}
            Set `circuit[3]` in `prepare_z.gadget.yaml`. The qodec is `repetition3`: repetition3 -> physical.
            Replaying the shot's 2 lines on a new shot:
            qodec> prepare_z 0
              R 0
              R 1
              R 2
              X 0
            qodec> measure_reset 0
              -- lower batch 1 of 2 --
              MZ 0  -> [0]
              MZ 1  -> [0]
              MZ 2  -> [0]
              -- lower batch 2 of 2 --
              XIF 0 c=0
              XIF 1 c=0
              XIF 2 c=0
            measure_reset 0 => [0]
            qodec> :drop top
            Dropped the top layer. The qodec is `repetition3`: physical.
            Replaying the shot's 2 lines on a new shot:
            qodec> prepare_z 0
            error: `prepare_z` is not an instruction of `physical`; see `:isa`
            Stopped replaying at line 1; the new shot has the lines before it.
            qodec> :replay maybe
            error: usage: :replay [on|off]
        "#]],
    );
}

#[test]
fn structure_commands_change_the_layers() {
    check(
        &qodec(true),
        &[
            ":reload",
            ":drop top",
            ":undo",
            ":drop bottom",
            ":isa bottom",
            ":slice 1 1",
            ":drop top",
            ":undo",
            ":undo",
            ":undo",
            ":slice 1 0",
            ":slice 0",
        ],
        &expect![[r#"
            qodec> :reload
            error: the qodec was not loaded from a file; see `:load`
            qodec> :drop top
            Dropped the top layer. The qodec is `logical`: repetition3 -> physical.
            qodec> :undo
            Undid the last change. The qodec is `logical`: logical -> repetition3 -> physical.
            qodec> :drop bottom
            Dropped the bottom layer. The qodec is `logical`: logical -> repetition3.
            qodec> :isa bottom
            layer 1: repetition3
              prepare_z <out:repetition3>
              idle <repetition3>
              __quantum__qis__m__body <in:repetition3>  => 1 outcome
              measure_reset <repetition3>  => 1 outcome
              logical_x <repetition3>
              discard <in:repetition3>
            qodec> :slice 1 1
            Kept layer 1. The qodec is `logical`: repetition3.
            qodec> :drop top
            error: the qodec has one layer; dropping it leaves nothing
            qodec> :undo
            Undid the last change. The qodec is `logical`: logical -> repetition3.
            qodec> :undo
            Undid the last change. The qodec is `logical`: logical -> repetition3 -> physical.
            qodec> :undo
            error: there is no change to undo
            qodec> :slice 1 0
            error: layer 1 is below layer 0; name the top one first
            qodec> :slice 0
            error: usage: :slice <first> <last>
        "#]],
    );
}

#[test]
fn qodecs_stack_at_the_instruction_set_they_share() {
    let (dir, shown) = scratch("stack");
    let logical = qodec(true).slice(0, 2).expect("slices");
    // Windows takes `/` too, so transcripts read the same on every platform.
    let file = format!("{shown}/logical.yaml");
    std::fs::write(&file, logical.to_bundle_string().expect("bundles")).expect("writes");
    let text = transcript(
        &qodec(false),
        &[
            &format!(":stack below {file}"),
            &format!(":stack above {file}"),
            "prepare 0; measure 0",
        ],
    );
    let _ = std::fs::remove_dir_all(&dir);
    expect![[r#"
        qodec> :stack below <dir>/logical.yaml
        error: the upper qodec's bottom instruction set `physical` is not the lower one's top instruction set `logical`
        qodec> :stack above <dir>/logical.yaml
        Stacked `<dir>/logical.yaml` above. The qodec is `repetition3`: logical -> repetition3 -> physical.
        qodec> prepare 0; measure 0
          R 0
          R 1
          R 2
          M 0  -> [0]
          M 1  -> [0]
          M 2  -> [0]
        measure 0 => [0]
    "#]].assert_eq(&text.replace(&shown, "<dir>"));
}

#[test]
fn qodecs_save_load_and_reload() {
    let (dir, shown) = scratch("files");
    let file = format!("{shown}/q.yaml");
    let mut session = session(&qodec(false)).with_source(&file);
    let lines = [
        &format!(":save {file}"),
        ":set prepare_z circuit[3] {X: [0]}",
        ":reload",
        ":gadget prepare_z",
        &format!(":save {shown}/files/"),
        &format!(":load {shown}/files/qodec.yaml"),
        ":layers",
    ];
    let text = play(&mut session, &lines, &Rc::default());
    // The rest of the error is the platform's.
    let missing = session.eval(&format!(":load {shown}/missing.yaml"));
    let _ = std::fs::remove_dir_all(&dir);
    let Reply::Print(missing) = missing else {
        panic!("prints")
    };
    assert!(
        style::plain(&missing).starts_with(&format!("error: cannot load `{shown}/missing.yaml`: ")),
        "{missing}"
    );
    expect![[r#"
        qodec> :save <dir>/q.yaml
        Saved the qodec to `<dir>/q.yaml`.
        qodec> :set prepare_z circuit[3] {X: [0]}
        Set `circuit[3]` in `prepare_z.gadget.yaml`. The qodec is `repetition3`: repetition3 -> physical.
        qodec> :reload
        Reloaded `<dir>/q.yaml`. The qodec is `repetition3`: repetition3 -> physical.
        qodec> :gadget prepare_z
        layer 0: repetition3 -> physical: gadget prepare_z (0 checks)
          circuit:
            source: |
              - R:
                - 0
              - R:
                - 1
              - R:
                - 2
            format: yaml
            out:
              '0': qubit
              '1': qubit
              '2': qubit
          out:
          - repetition3: [0, 1, 2]
        qodec> :save <dir>/files/
        Saved the qodec in `<dir>/files/`; its manifest is `<dir>/files/qodec.yaml`.
        qodec> :load <dir>/files/qodec.yaml
        Loaded `<dir>/files/qodec.yaml`. The qodec is `repetition3`: repetition3 -> physical.
        qodec> :layers
        layer 0: repetition3 [blocks: repetition3]  (top)
        layer 1: physical [blocks: qubit]  (bottom)
    "#]].assert_eq(&style::plain(&text).replace(&shown, "<dir>"));
}

#[test]
fn edits_reopen_until_the_document_plans() {
    type Edit = fn(&str) -> String;
    let shown = Rc::new(RefCell::new(String::new()));
    let edits: Rc<RefCell<VecDeque<Edit>>> = Rc::new(RefCell::new(
        [
            // Unchanged.
            (|text: &str| text.to_string()) as Edit,
            // Broken, then fixed.
            |text| text.replace("R: [2]", "NOPE: [2]"),
            |text| text.replace("NOPE: [2]", "X: [2]"),
            // Broken, then given up.
            |text| text.replace("X: [2]", "NOPE: [2]"),
            |text| text.to_string(),
        ]
        .into(),
    ));
    let editor = {
        let shown = Rc::clone(&shown);
        Box::new(move |name: &str, text: &str| {
            let mut shown = shown.borrow_mut();
            writeln!(shown, "  [editing {name}]").expect("writes");
            for comment in text.lines().filter(|l| l.starts_with('#')) {
                writeln!(shown, "  {comment}").expect("writes");
            }
            let edit = edits.borrow_mut().pop_front().expect("an edit is left");
            Ok(edit(text))
        })
    };
    let mut session = session(&qodec(false)).with_editor(editor);
    let text = play(
        &mut session,
        &[
            ":edit prepare_z",
            ":edit prepare_z.gadget.yaml",
            ":gadget prepare_z",
            ":edit prepare_z",
            ":edit nope",
        ],
        &shown,
    );
    expect![[r#"
        qodec> :edit prepare_z
          [editing prepare_z.gadget.yaml]
        No changes.
        qodec> :edit prepare_z.gadget.yaml
          [editing prepare_z.gadget.yaml]
          [editing prepare_z.gadget.yaml]
          # qodec-repl: the qodec does not plan: layer 0: gadget `prepare_z`: call to unknown instruction "NOPE" in instruction set "physical"; it is as it was
          # qodec-repl: Fix the document and save it, or save it unchanged to give up.
        Edited `prepare_z.gadget.yaml`. The qodec is `repetition3`: repetition3 -> physical.
        qodec> :gadget prepare_z
        layer 0: repetition3 -> physical: gadget prepare_z (0 checks)
          circuit:
            source: |
              - R:
                - 0
              - R:
                - 1
              - X:
                - 2
            format: yaml
            out:
              '0': qubit
              '1': qubit
              '2': qubit
          out:
          - repetition3: [0, 1, 2]
        qodec> :edit prepare_z
          [editing prepare_z.gadget.yaml]
          [editing prepare_z.gadget.yaml]
          # qodec-repl: the qodec does not plan: layer 0: gadget `prepare_z`: call to unknown instruction "NOPE" in instruction set "physical"; it is as it was
          # qodec-repl: Fix the document and save it, or save it unchanged to give up.
        Gave up the edit; the qodec is as it was.
        qodec> :edit nope
        error: no item `nope`; `:yaml` lists the qodec's documents
    "#]].assert_eq(&style::plain(&text));
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
