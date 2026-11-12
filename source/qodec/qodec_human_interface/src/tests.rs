// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    app::{App, Focus, Mode},
    shot::{Engine, Line, Outcome, Shot},
    syntax,
    trace::{BitId, Outcomes, Policy},
    ui, vocab,
    workbench::{Item, Mark},
};
use expect_test::{Expect, expect};
use qodec::Qodec;
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};
use std::{fmt::Write, path::PathBuf};

/// The example qodec at `path`, relative to the shared examples.
fn example(path: &str) -> Qodec {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(path);
    Qodec::load(&path).unwrap_or_else(|e| panic!("cannot load {}: {e}", path.display()))
}

fn repetition3() -> Qodec {
    example("repetition3/repetition3.qodec.yaml")
}

/// A program of `texts`, with ids 0, 1, ...
fn program(texts: &[&str]) -> Vec<Line> {
    texts
        .iter()
        .zip(0..)
        .map(|(text, id)| Line {
            id,
            text: (*text).to_string(),
        })
        .collect()
}

/// Every line's outcome and its tree of batches, a boundary per indent.
fn outline(engine: &Engine, shot: &Shot) -> String {
    let mut text = String::new();
    for (i, run) in shot.lines.iter().enumerate() {
        writeln!(text, "line {i}: {:?}", run.outcome).expect("writing to a string succeeds");
        if let Some(root) = run.root {
            node(engine, shot, root, &mut text);
        }
    }
    text
}

fn node(engine: &Engine, shot: &Shot, id: usize, text: &mut String) {
    let node = &shot.nodes[id];
    let indent = "  ".repeat(node.boundary + 1);
    let isa = engine.isa(node.boundary);
    for traced in &node.calls {
        let mut line = format!(
            "{indent}{}",
            syntax::plain(&syntax::render(isa, &traced.call))
        );
        if let Some(readout) = &traced.readout {
            let decl = &isa.instructions[traced.call.instruction_id];
            line += " => ";
            line += &syntax::plain(&vocab::readout(decl, readout, &traced.bits));
        }
        writeln!(text, "{line}").expect("writing to a string succeeds");
    }
    if let Some(fault) = &node.fault {
        writeln!(text, "{indent}fault: {fault}").expect("writing to a string succeeds");
    }
    for &child in &node.children {
        self::node(engine, shot, child, text);
    }
}

fn check(qodec: &Qodec, texts: &[&str], outcomes: &Outcomes, expect: &Expect) {
    let engine = Engine::new(qodec).expect("the qodec plans");
    let shot = engine.run(&program(texts), outcomes);
    expect.assert_eq(&outline(&engine, &shot));
}

#[test]
fn every_layer_is_traced_in_the_order_it_ran() {
    check(
        &repetition3(),
        &["prepare_z 0", "idle 0", "measure_z 0"],
        &Outcomes::default(),
        &expect![[r#"
            line 0: Ran
              prepare_z 0
                R 0
                R 1
                R 2
            line 1: Ran
              idle 0
                R 3
                R 4
                CX 0 3
                CX 1 3
                CX 1 4
                CX 2 4
                M 3 => Z_0=0
                M 4 => Z_0=0
            line 2: Ran
              measure_z 0 => Z_0=0
                M 0 => Z_0=0
                M 1 => Z_0=0
                M 2 => Z_0=0
        "#]],
    );
}

#[test]
fn a_pinned_bottom_bit_changes_the_decoded_result_above() {
    let mut outcomes = Outcomes::default();
    // The first bit of line 1: `M 0` of `measure_z`.
    outcomes.pins.insert(BitId { line: 1, k: 0 }, true);
    outcomes.pins.insert(BitId { line: 1, k: 1 }, true);
    outcomes.pins.insert(BitId { line: 1, k: 2 }, true);
    check(
        &repetition3(),
        &["prepare_z 0", "measure_z 0"],
        &outcomes,
        &expect![[r#"
            line 0: Ran
              prepare_z 0
                R 0
                R 1
                R 2
            line 1: Ran
              measure_z 0 => Z_0=1
                M 0 => Z_0=1
                M 1 => Z_0=1
                M 2 => Z_0=1
        "#]],
    );
}

#[test]
fn a_refused_line_runs_nothing_and_the_shot_goes_on() {
    check(
        &repetition3(),
        &["measure_z 0", "prepare_z 0", "bogus", "# a comment", ""],
        &Outcomes::default(),
        &expect![[r#"
            line 0: Rejected("layer 0: invalid instruction: block 0 is not live")
            line 1: Ran
              prepare_z 0
                R 0
                R 1
                R 2
            line 2: Unparsed("`bogus` is not an instruction of `repetition3`")
            line 3: Blank
            line 4: Blank
        "#]],
    );
}

#[test]
fn a_middle_layer_shows_between_top_and_bottom() {
    let engine = Engine::new(&example("c4c6/qodec.yaml")).expect("the qodec plans");
    let shot = engine.run(
        &program(&["prepare_z_all 0", "measure_z_all 0"]),
        &Outcomes::default(),
    );
    assert!(shot.lines.iter().all(|l| l.outcome == Outcome::Ran));
    let per_boundary: Vec<usize> = shot.timelines.iter().map(Vec::len).collect();
    expect![[r#"
        [
            2,
            26,
            244,
        ]
    "#]]
    .assert_debug_eq(&per_boundary);
    // Every middle and bottom batch descends from a top batch of its line.
    for (id, node) in shot.nodes.iter().enumerate() {
        let root = shot.lines[node.line].root.expect("the line ran");
        let mut ancestor = id;
        while let Some(parent) = shot.nodes[ancestor].parent {
            ancestor = parent;
        }
        assert_eq!(ancestor, root);
        assert_eq!(shot.nodes[root].boundary, 0);
    }
}

#[test]
fn random_bits_belong_to_their_line_not_to_their_position() {
    let engine = Engine::new(&repetition3()).expect("the qodec plans");
    let outcomes = Outcomes {
        policy: Policy::Random(7),
        ..Outcomes::default()
    };
    let bits = |program: &[Line], id: u64| -> Vec<bool> {
        let shot = engine.run(program, &outcomes);
        let index = program.iter().position(|l| l.id == id).expect("the line");
        shot.nodes
            .iter()
            .filter(|n| n.line == index)
            .flat_map(|n| &n.calls)
            .filter_map(|t| t.readout.as_ref())
            .flat_map(|r| r.outcomes.clone())
            .map(|b| b.expect("bottom bits are known"))
            .collect()
    };
    let before = program(&["prepare_z 0", "idle 0", "idle 0"]);
    let mut after = before.clone();
    after.insert(
        1,
        Line {
            id: 9,
            text: "idle 0".to_string(),
        },
    );
    assert_eq!(bits(&before, 2), bits(&after, 2));
}

// ── The interface ───────────────────────────────────────────────────────────

/// An interface on `qodec` with the program `texts`.
fn app(qodec: &Qodec, texts: &[&str]) -> App {
    let engine = Engine::new(qodec).expect("the qodec plans");
    let texts: Vec<String> = texts.iter().map(|t| (*t).to_string()).collect();
    App::new(engine, &texts, None)
}

fn press(app: &mut App, keys: &str) {
    for c in keys.chars() {
        let code = match c {
            '\t' => KeyCode::Tab,
            '\n' => KeyCode::Enter,
            '\x1b' => KeyCode::Esc,
            '←' => KeyCode::Left,
            '→' => KeyCode::Right,
            '↑' => KeyCode::Up,
            '↓' => KeyCode::Down,
            '⌫' => KeyCode::Backspace,
            c => KeyCode::Char(c),
        };
        app.key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

/// The screen, `width` by `height`, as text.
fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| ui::render(frame, app))
        .expect("drawing succeeds");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..height {
        let mut row = String::new();
        for x in 0..width {
            row.push_str(buffer[(x, y)].symbol());
        }
        writeln!(text, "{}", row.trim_end()).expect("writing to a string succeeds");
    }
    text
}

fn texts(app: &App) -> Vec<&str> {
    app.program.iter().map(|l| l.text.as_str()).collect()
}

fn outcomes(app: &App) -> Vec<&Outcome> {
    app.shot.lines.iter().map(|l| &l.outcome).collect()
}

#[test]
fn parts_add_lines_that_run_on_the_blocks_live_there() {
    let mut app = app(&repetition3(), &[]);
    assert_eq!(app.focus, Focus::Parts);
    // prepare_z, then idle and measure_z: each on the block prepare_z gave.
    press(&mut app, "\n↓\n↓\n");
    assert_eq!(texts(&app), ["prepare_z 0", "idle 0", "measure_z 0"]);
    assert!(outcomes(&app).iter().all(|o| **o == Outcome::Ran));
    // `measure_z` took block 0, so the next `prepare_z` gives block 0 again;
    // with 0 live, the one after gives block 1.
    press(&mut app, "↑↑\n");
    assert_eq!(texts(&app)[3], "prepare_z 0");
    press(&mut app, "\n");
    assert_eq!(texts(&app)[4], "prepare_z 1");
    expect![[r#"
         qodec-hi  repetition3 · repetition3 ▸ stim+rz · bits: zero · ? help
        ┌Map──────────────────┐┌Parts · repetition3───────┐┌Program─────────────────────────┐┌Circuit 1: stim+rz (bottom) · 1┐
        │repetition3          ││prepare_z <out:repetition3││1  prepare_z 0                  ││  1 2                 3   4 5  │
        │0 repetition3 (top)  ││idle <repetition3>        ││2  idle 0                       ││0 R───CX──────────────M→0 R─── │
        │  ! prepare_z        ││measure_z <in:repetition3>││3  measure_z 0  ⟹ Z_0=0         ││1 R───┼──CX─CX────────M→0 R─── │
        │  ✓ idle             ││rotate_z <repetition3> the││4  prepare_z 0                  ││2 R───┼──┼──┼──CX─────M→0 R─── │
        │  ✓ measure_z        ││                          ││5  prepare_z 1                  ││3   R─CX─CX─┼──┼──M→0       R─ │
        │  ~ rotate_z         ││                          ││                                ││4   R───────CX─CX─M→0       R─ │
        └─────────────────────┘└──────────────────────────┘└────────────────────────────────┘└───────────────────────────────┘
         Live at stim+rz after call 18: qubit 0–3
        ┌────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
        │prepare_z <out:repetition3>  Prepare the logical |0> state.                                                         │
        │Gadget () -> (repetition3): 3 stim+rz calls, 0 checks.                                                              │
        │Enter adds prepare_z 2 after line 5; e adds it and edits it.                                                        │
        └────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
         ↑↓  choose   Enter  add   e  add+edit   Tab  next pane  o bits  q quit
    "#]]
    .assert_eq(&screen(&mut app, 118, 16));
}

#[test]
fn a_line_runs_as_it_is_typed() {
    let mut app = app(&repetition3(), &["prepare_z 0"]);
    assert_eq!(app.focus, Focus::Program);
    press(&mut app, "nmeasure_z");
    assert!(matches!(outcomes(&app)[1], Outcome::Rejected(_)));
    press(&mut app, " 0");
    assert_eq!(outcomes(&app)[1], &Outcome::Ran);
    press(&mut app, "\n");
    assert_eq!(app.editor, None);
    assert_eq!(texts(&app), ["prepare_z 0", "measure_z 0"]);
    // Cancelling an edit puts the line back, and a new line goes away.
    press(&mut app, "\ny\x1b");
    assert_eq!(texts(&app), ["prepare_z 0", "measure_z 0"]);
    press(&mut app, "n\x1b");
    assert_eq!(texts(&app), ["prepare_z 0", "measure_z 0"]);
    // Tab completes a mnemonic, then a parameter.
    press(&mut app, "nrot\t0 th\t1.5\n");
    assert_eq!(texts(&app)[2], "rotate_z 0 theta=1.5");
}

#[test]
fn flipping_a_bottom_bit_changes_every_result_above_it() {
    let mut app = app(&repetition3(), &["prepare_z 0", "measure_z 0"]);
    press(&mut app, "\t");
    assert_eq!(app.focus, Focus::View);
    app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    expect![[r#"
        M 0 → Z_0=0  Destructive Z-basis measurement.
        From line 2 measure_z 0 ▸ stim+rz batch 1 (3 calls)
        bits made up as zero. Space flips it, x unpins; every result above it follows."#]]
    .assert_eq(&explain(&mut app));
    press(&mut app, " ");
    expect![[r#"
        M 0 → Z_0=1  Destructive Z-basis measurement.
        From line 2 measure_z 0 ▸ stim+rz batch 1 (3 calls)
        1 bit pinned. Space flips it, x unpins; every result above it follows."#]]
    .assert_eq(&explain(&mut app));
    expect![[r#"
        line 0: Ran
          prepare_z 0
            R 0
            R 1
            R 2
        line 1: Ran
          measure_z 0 => Z_0=1
            M 0 => Z_0=1
            M 1 => Z_0=0
            M 2 => Z_0=0
    "#]]
    .assert_eq(&outline(&app.engine, &app.shot));
    // Undo, unpin and the outcome policy all go back to zero.
    press(&mut app, "u");
    assert!(app.outcomes.pins.is_empty());
    press(&mut app, " x");
    assert!(app.outcomes.pins.is_empty());
    press(&mut app, "o");
    assert_eq!(app.outcomes.policy, Policy::One);
}

/// The text of the explain pane.
fn explain(app: &mut App) -> String {
    let text = screen(app, 100, 16);
    let lines: Vec<&str> = text.lines().collect();
    lines[11..14]
        .iter()
        .map(|l| l.trim_start_matches('│').trim_end_matches('│').trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn brackets_go_down_to_what_a_call_became_and_back_up() {
    let mut app = app_c4c6();
    press(&mut app, "\t");
    assert_eq!(app.boundary, 2);
    press(&mut app, "[[");
    assert_eq!(app.boundary, 0);
    press(&mut app, "→");
    let top = app.event().expect("a selected call");
    assert_eq!(app.line, 1);
    press(&mut app, "]");
    let middle = app.event().expect("a selected call");
    assert_eq!(app.shot.nodes[middle.node].parent, Some(top.node));
    press(&mut app, "]");
    let bottom = app.event().expect("a selected call");
    assert_eq!(app.shot.nodes[bottom.node].parent, Some(middle.node));
    press(&mut app, "[[");
    assert_eq!(app.event(), Some(top));
}

#[test]
fn the_flow_shows_a_line_through_every_layer() {
    let mut app = app_c4c6();
    press(&mut app, "v");
    assert_eq!(app.mode, Mode::Flow);
    let text = screen(&mut app, 120, 16);
    expect![[r#"
        │ line 1 prepare_z_all 0        │
        │ 0: C4C6 · 1 call              │
        │   prepare_z_all 0 → reject=0  │
        │ ╎ 1: C4C6 · 17 calls          │
        │ ╎   prepare_x_all 0 → reject=0│"#]]
    .assert_eq(
        &text
            .lines()
            .skip(2)
            .take(5)
            .map(|l| &l[l.rfind("││").map_or(0, |i| i + "│".len())..])
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn a_refused_line_says_what_would_make_it_run() {
    let mut app = app(&repetition3(), &["measure_z 0", "prepare_z 0"]);
    expect![[r#"
        Refused: layer 0: invalid instruction: block 0 is not live
        Nothing ran, and the shot goes on. A block is live once an instruction gives it, such as
        prepare_z."#]]
    .assert_eq(&explain(&mut app));
}

/// Every example, by the path of its qodec.
const EXAMPLES: [&str; 12] = [
    "bacon-shor/bacon-shor.qodec.yaml",
    "bb-72/bb-72.qodec.yaml",
    "c422-c832-arch/qodec.yaml",
    "c4c6/qodec.yaml",
    "distillation-15/distillation-15.qodec.yaml",
    "five_qubit/five_qubit.qodec.yaml",
    "honeycomb/honeycomb.qodec.yaml",
    "iceberg/iceberg.qodec.yaml",
    "reed-muller-15/reed-muller-15.qodec.yaml",
    "repetition3/repetition3.qodec.yaml",
    "steane/steane.qodec.yaml",
    "surface/surface.qodec.yaml",
];

#[test]
fn every_example_runs_and_draws_at_every_boundary() {
    for path in EXAMPLES {
        let mut app = app(&example(path), &[]);
        // The first few parts, each on what the earlier ones left live.
        let parts = app.engine.top().instructions.len().min(4);
        for part in 0..parts {
            app.insert_part(part);
        }
        assert!(
            outcomes(&app).iter().any(|o| **o == Outcome::Ran),
            "{path}: nothing ran: {:?}",
            outcomes(&app)
        );
        for b in 0..app.engine.boundaries() {
            app.boundary = b;
            app.cursor = None;
            for mode in [Mode::Circuit, Mode::Flow] {
                app.mode = mode;
                screen(&mut app, 120, 30);
            }
        }
    }
}

fn app_c4c6() -> App {
    app(
        &example("c4c6/qodec.yaml"),
        &["prepare_z_all 0", "measure_z_all 0"],
    )
}

#[test]
fn cancelling_an_edit_keeps_what_redo_would_bring_back() {
    let mut app = app(&repetition3(), &["prepare_z 0", "idle 0"]);
    press(&mut app, "du");
    assert_eq!(texts(&app), ["prepare_z 0", "idle 0"]);
    press(&mut app, "\n\x1bn\x1bU");
    assert_eq!(texts(&app), ["idle 0"]);
}

#[test]
fn completing_after_a_wide_space_does_not_panic() {
    let mut app = app(&repetition3(), &[]);
    press(&mut app, "\tnfoo\u{a0}\t");
    assert_eq!(
        app.editor.as_ref().map(|e| e.text.as_str()),
        Some("foo\u{a0}")
    );
}

// ── Gadget analysis ─────────────────────────────────────────────────────────

/// Every gadget of `qodec`, by (layer, mnemonic), with its lower ISA.
fn gadgets(qodec: &Qodec) -> Vec<(usize, String, qodec::Gadget, qodec::InstructionSet)> {
    let layers = qodec.layers();
    let mut out = Vec::new();
    for (k, layer) in layers
        .iter()
        .enumerate()
        .take(layers.len().saturating_sub(1))
    {
        for (m, g) in &layer.gadgets {
            out.push((
                k,
                m.clone(),
                g.clone(),
                (*layers[k + 1].instruction_set).clone(),
            ));
        }
    }
    out
}

#[test]
fn gadgets_of_well_built_examples_verify() {
    for path in [
        "steane/steane.qodec.yaml",
        "five_qubit/five_qubit.qodec.yaml",
        "surface/surface.qodec.yaml",
        "iceberg/iceberg.qodec.yaml",
        "bacon-shor/bacon-shor.qodec.yaml",
        "reed-muller-15/reed-muller-15.qodec.yaml",
        "bb-72/bb-72.qodec.yaml",
        "distillation-15/distillation-15.qodec.yaml",
    ] {
        for (k, m, g, lower) in gadgets(&example(path)) {
            let a = crate::analysis::analyze(&g, &lower).expect("the circuit reads");
            assert_eq!(
                a.problems(),
                0,
                "{path} layer {k} `{m}`: {:?}",
                a.action.mismatches
            );
        }
    }
}

#[test]
fn the_analysis_catches_a_missing_sign_in_the_honeycomb_example() {
    // Y⊗6 = -(X⊗6)(Z⊗6): the green edges multiply to the plaquettes with a
    // minus sign, so the detector needs a literal 1.
    let (_, _, g, lower) = gadgets(&example("honeycomb/honeycomb.qodec.yaml"))
        .into_iter()
        .find(|(_, m, _, _)| m == "round_green")
        .expect("round_green");
    let a = crate::analysis::analyze(&g, &lower).expect("the circuit reads");
    let detector = &a.checks[4];
    assert_eq!(detector.residual, Ok(crate::gf2::Form::constant(true)));
}

#[test]
fn adopting_the_suggestions_makes_a_bare_gadget_verify() {
    let (_, _, mut g, lower) = gadgets(&repetition3())
        .into_iter()
        .find(|(_, m, _, _)| m == "idle")
        .expect("idle");
    g.checks.clear();
    let a = crate::analysis::analyze(&g, &lower).expect("the circuit reads");
    // Two stabilizers measured, each carried to the output: four checks.
    let suggested: Vec<String> = a
        .suggestions
        .checks
        .iter()
        .map(|c| crate::analysis::equation_text(c))
        .collect();
    expect![[r#"
        [
            "[\"circuit.readouts[0]\", \"in[0].stabilizers[0]\"]",
            "[\"circuit.readouts[1]\", \"in[0].stabilizers[1]\"]",
            "[\"circuit.readouts[0]\", \"out[0].stabilizers[0]\"]",
            "[\"circuit.readouts[1]\", \"out[0].stabilizers[1]\"]",
        ]
    "#]]
    .assert_debug_eq(&suggested);

    let (_, _, mut m, lower) = gadgets(&repetition3())
        .into_iter()
        .find(|(_, m, _, _)| m == "measure_z")
        .expect("measure_z");
    m.readouts.clear();
    m.checks.clear();
    let a = crate::analysis::analyze(&m, &lower).expect("the circuit reads");
    let readout = a
        .suggestions
        .readouts
        .first()
        .cloned()
        .flatten()
        .expect("a readout");
    expect![[r#"["circuit.readouts[0]", "in[0].z[0]"]"#]]
        .assert_eq(&crate::analysis::equation_text(&readout));
}
// ── The workbench ───────────────────────────────────────────────────────────

/// A workbench on the example at `path`, after `edit` changes its documents.
fn workbench_with(path: &str, edit: impl FnOnce(&mut Vec<crate::bundle::Doc>)) -> App {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(path);
    let (mut docs, source) = crate::bundle::load(&path).expect("the example reads");
    edit(&mut docs);
    App::open(docs, Some(source), &[], None).expect("the example opens")
}

fn workbench(path: &str) -> App {
    workbench_with(path, |_| {})
}

/// The document of `app` named `name`.
fn doc<'a>(app: &'a App, name: &str) -> &'a crate::bundle::Doc {
    app.docs
        .iter()
        .find(|d| d.name == name)
        .expect("the document")
}

#[test]
fn a_gadget_shows_its_circuit_its_state_and_what_the_tool_found() {
    let mut app = workbench("repetition3/repetition3.qodec.yaml");
    app.select(&Item::Instruction(0, 1));
    app.focus = Focus::Circuit;
    press(&mut app, "→→→");
    expect![[r#"
         qodec-hi  repetition3 · repetition3 ▸ stim+rz · bits: zero · ? help
        ┌Map───────────────────┐┌Circuit · idle · 3/8────────────────────────────────────┐┌State · after 3 · 6 generators──────┐
        │repetition3           ││0 in0───CX──────────────out0                            ││01234r sign                         │
        │0 repetition3 (top)   ││1 in0───┼──CX─CX────────out0                            ││ZZ···· (−1)^(in[0].stabilizers[0])  │
        │  ! prepare_z         ││2 in0───┼──┼──┼──CX─────out0                            ││·ZZ··· (−1)^(in[0].stabilizers[1])  │
        │  ✓ idle              ││3     R─CX─CX─┼──┼──M→0                                 ││XXXX·X (−1)^(in[0].x[0])            │
        │  ✓ measure_z         ││4     R───────CX─CX─M→1                                 ││Z····Z (−1)^(in[0].z[0])            │
        │  ~ rotate_z          ││                                                        ││Z··Z·· +                            │
        │  ◇ repetition3: repet││                                                        ││····Z· +                            │
        │1 stim+rz (bottom)    ││                                                        ││                                    │
        │    R                 ││                                                        ││                                    │
        │    M                 ││                                                        ││                                    │
        │    CX                ││                                                        ││                                    │
        │    rotate_z          │└────────────────────────────────────────────────────────┘└────────────────────────────────────┘
        │Program               │┌Parts · stim+rz──┐┌Text · idle.gadget.yaml──┐┌Findings────────────────────────────────────────┐
        │                      ││R <out:qubit>    ││ 1 circuit:              ││What each measurement measures                  │
        │                      ││M <in:qubit>     ││ 2   source:             ││  readouts[0] M 3 (Z_0) = in[0].stabilizers[0]  │
        │                      ││CX <qubit> <qubit││ 3     - R: [3]          ││  readouts[1] M 4 (Z_0) = in[0].stabilizers[1]  │
        │                      ││rotate_z <qubit> ││ 4     - R: [4]          ││Checks                                          │
        │                      ││                 ││ 5     - CX: [0, 3]      ││✓ ["circuit.readouts[0]", "in[0].stabilizers[0]"│
        │                      ││                 ││ 6     - CX: [1, 3]      ││✓ ["circuit.readouts[1]", "in[0].stabilizers[1]"│
        │                      ││                 ││ 7     - CX: [1, 4]      ││✓ ["circuit.readouts[0]", "out[0].stabilizers[0]│
        │                      ││                 ││ 8     - CX: [2, 4]      ││✓ ["circuit.readouts[1]", "out[0].stabilizers[1]│
        │                      ││                 ││ 9     - M: [3]          ││Output stabilizers                              │
        │                      ││                 ││10     - M: [4]          ││✓ out[0].stabilizers[0] = in[0].stabilizers[0]  │
        │                      ││                 ││11 checks:               ││✓ out[0].stabilizers[1] = in[0].stabilizers[1]  │
        └──────────────────────┘└─────────────────┘└─────────────────────────┘└────────────────────────────────────────────────┘
         State after call 3 of 8: CX 0 3
        ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
        │CX 0 3  takes 0 3 gives 0 3                                                                                           │
        │                                                                                                                      │
        │                                                                                                                      │
        └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
         ←→  step   Home/End  start/end   Tab  next pane  o bits  q quit
    "#]].assert_eq(&screen(&mut app, 120, 34));
}

/// The findings of the focused gadget, a row per line.
fn findings(app: &mut App) -> String {
    app.findings()
        .into_iter()
        .map(|f| {
            let mut row: String = f.tokens.into_iter().map(|(_, t)| t).collect();
            row.push('\n');
            row
        })
        .collect()
}

/// The map's mark on instruction `i` of layer `k`.
fn mark(app: &mut App, item: &Item) -> Mark {
    app.rows()
        .into_iter()
        .find(|r| r.item == *item)
        .expect("the item is on the map")
        .mark
}

#[test]
fn adopting_what_the_tool_derives_makes_a_gadget_verify() {
    let mut app = workbench("repetition3/repetition3.qodec.yaml");
    // The map: the qodec, layer 0, then its first instruction.
    press(&mut app, "↓↓");
    let prepare = Item::Instruction(0, 0);
    assert_eq!(app.item, prepare);
    // Nothing tells the decoder the signs of the stabilizers it prepares.
    assert_eq!(mark(&mut app, &prepare), Mark::Warn);
    expect![[r#"
        What each measurement measures
          The circuit reports no bits.
        Checks
        + ["out[0].stabilizers[0]"]  derived; Enter adopts
        + ["out[0].stabilizers[1]"]  derived; Enter adopts
        Output stabilizers
        ! out[0].stabilizers[0] = 0  no check determines it
        ! out[0].stabilizers[1] = 0  no check determines it
        Does it implement the instruction?
        ✓ it implements `prepare_z`, with these readouts and frames
    "#]]
    .assert_eq(&findings(&mut app));

    press(&mut app, "\t\t\t\tA");
    assert_eq!(app.focus, Focus::Findings);
    assert_eq!(mark(&mut app, &prepare), Mark::Ok);
    expect![[r#"
        circuit:
          source:
            - R: [0]
            - R: [1]
            - R: [2]
        checks:
          - ["out[0].stabilizers[0]"]
          - ["out[0].stabilizers[1]"]
    "#]]
    .assert_eq(&doc(&app, "prepare_z.gadget.yaml").body);
    assert!(app.dirty());

    press(&mut app, "u");
    assert_eq!(mark(&mut app, &prepare), Mark::Warn);
}

#[test]
fn a_new_gadget_grows_from_the_parts_below() {
    let mut app = workbench_with("repetition3/repetition3.qodec.yaml", |docs| {
        let lines: Vec<String> = docs[0]
            .lines()
            .into_iter()
            .filter(|l| !l.contains("prepare_z.gadget.yaml"))
            .collect();
        docs[0].set_lines(&lines);
        docs.retain(|d| d.name != "prepare_z.gadget.yaml");
    });
    let prepare = Item::Instruction(0, 0);
    press(&mut app, "↓↓");
    assert_eq!(mark(&mut app, &prepare), Mark::Missing);
    press(&mut app, "g");
    assert_eq!(app.focus, Focus::Parts);
    // R, three times: each fills the next output label that has no state.
    press(&mut app, "\n\n\n");
    expect![[r#"
        circuit:
          source:
            - R: [0]
            - R: [1]
            - R: [2]
    "#]]
    .assert_eq(&doc(&app, "prepare_z.gadget.yaml").body);
    expect![[r#"
        What each measurement measures
          The circuit reports no bits.
        Checks
        + ["out[0].stabilizers[0]"]  derived; Enter adopts
        + ["out[0].stabilizers[1]"]  derived; Enter adopts
        Output stabilizers
        ! out[0].stabilizers[0] = 0  no check determines it
        ! out[0].stabilizers[1] = 0  no check determines it
        Does it implement the instruction?
        ✓ it implements `prepare_z`, with these readouts and frames
    "#]]
    .assert_eq(&findings(&mut app));
    press(&mut app, "\t\tA");
    assert_eq!(app.focus, Focus::Findings);
    assert_eq!(mark(&mut app, &prepare), Mark::Ok);
}

#[test]
fn editing_a_code_shows_what_no_longer_fits() {
    let mut app = workbench("repetition3/repetition3.qodec.yaml");
    let code = Item::Code(0, "repetition3".to_string());
    app.select(&code);
    assert_eq!(mark(&mut app, &code), Mark::Ok);
    app.focus = Focus::Text;
    let line = doc(&app, "repetition3.code.yaml")
        .lines()
        .iter()
        .position(|l| l.contains("stabilizers"))
        .expect("the stabilizers");
    app.text_line = line;
    // As it is typed: every keystroke is a new code.
    press(&mut app, "\n⌫⌫⌫⌫⌫⌫⌫⌫X_1 X_2]");
    assert_eq!(mark(&mut app, &code), Mark::Error);
    expect![[r#"
         qodec-hi  repetition3 · repetition3 ▸ stim+rz · bits: zero · unsaved · ? help
        ┌Map───────────────────┐┌repetition3: repetition3 [[3,1]]───────────────────┐┌What holds───────────────────────────────┐
        │repetition3           ││   012                                             ││✓ 1 logical X and 1 logical Z operators. │
        │0 repetition3 (top)   ││S0 ZZ·                                             ││✗ Stabilizers that anticommute: S0·S1.   │
        │  ✗ prepare_z         ││S1 ·XX                                             ││✓ The logical operators commute with the │
        │  ✗ idle              ││X0 XXX                                             ││stabilizers.                             │
        │  ✗ measure_z         ││Z0 Z··                                             ││✓ Each X_i anticommutes with Z_i only;   │
        │  ~ rotate_z          ││                                                   ││the rest commute.                        │
        │  ◇ repetition3: repet││                                                   ││✓ 2 independent stabilizers.             │
        │1 stim+rz (bottom)    ││                                                   ││✓ The logical operators are independent  │
        │    R                 ││                                                   ││of the stabilizers.                      │
        │    M                 ││                                                   ││✓ [[3,1]]: 3 qubits less 2 independent   │
        │    CX                │└───────────────────────────────────────────────────┘└─────────────────────────────────────────┘
        │    rotate_z          │┌Text · repetition3.code.yaml──────────────────────────────────────────────────────────────────┐
        │Program               ││1 name: repetition3                                                                           │
        │                      ││2 description: 3-qubit bit-flip repetition code (Z-basis).                                    │
        │                      ││3 stabilizers: [Z_0 Z_1, X_1 X_2]                                                             │
        │                      ││4 x: [X_0 X_1 X_2]                                                                            │
        │                      ││5 z: [Z_0]                                                                                    │
        │                      ││                                                                                              │
        │                      ││                                                                                              │
        │                      ││                                                                                              │
        └──────────────────────┘└──────────────────────────────────────────────────────────────────────────────────────────────┘
         2 stabilizers, 1 logical qubits on 3 physical qubits
        ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
        │repetition3.code.yaml line 3. Everything is computed again as you type; Enter keeps the line, Esc puts it back.       │
        │                                                                                                                      │
        │                                                                                                                      │
        └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
         Enter  keep   Esc  cancel   ←→  move  o bits  q quit
    "#]].assert_eq(&screen(&mut app, 120, 30));
    press(&mut app, "\x1b");
    assert_eq!(mark(&mut app, &code), Mark::Ok);
}

#[test]
fn text_that_does_not_load_keeps_the_last_qodec_that_did() {
    let mut app = workbench("repetition3/repetition3.qodec.yaml");
    app.select(&Item::Instruction(0, 1));
    app.focus = Focus::Text;
    app.text_line = 4;
    // `- CX: [0, 3]` without its bracket is not YAML.
    press(&mut app, "\n⌫");
    assert!(app.load_error.is_some());
    // The map and the gadget still show the qodec as it last loaded.
    assert_eq!(mark(&mut app, &Item::Instruction(0, 1)), Mark::Ok);
    expect![[r#"
         qodec-hi  repetition3 · repetition3 ▸ stim+rz · bits: zero · the text does not load · unsaved · ? help
        ┌Map───────────────────┐┌Circuit · idle · 8 calls────────────────────────────────┐┌State · inputs · 4 generators───────┐
        │repetition3 ✗         ││0 in0───CX──────────────out0                            ││012r sign                           │
        │0 repetition3 (top)   ││1 in0───┼──CX─CX────────out0                            ││ZZ·· (−1)^(in[0].stabilizers[0])    │
        │  ! prepare_z         ││2 in0───┼──┼──┼──CX─────out0                            ││·ZZ· (−1)^(in[0].stabilizers[1])    │
        │  ✓ idle              ││3     R─CX─CX─┼──┼──M→0                                 ││XXXX (−1)^(in[0].x[0])              │
        │  ✓ measure_z         ││4     R───────CX─CX─M→1                                 ││Z··Z (−1)^(in[0].z[0])              │
        │  ~ rotate_z          ││                                                        ││                                    │
        │  ◇ repetition3: repet││                                                        ││                                    │
        │1 stim+rz (bottom)    ││                                                        ││                                    │
        │    R                 ││                                                        ││                                    │
        │    M                 │└────────────────────────────────────────────────────────┘└────────────────────────────────────┘
        │    CX                │┌Parts · stim+rz──┐┌Text · idle.gadget.yaml──┐┌Findings────────────────────────────────────────┐
        │    rotate_z          ││R <out:qubit>    ││ 1 circuit:              ││What each measurement measures                  │
        │Program               ││M <in:qubit>     ││ 2   source:             ││  readouts[0] M 3 (Z_0) = in[0].stabilizers[0]  │
        │                      ││CX <qubit> <qubit││ 3     - R: [3]          ││  readouts[1] M 4 (Z_0) = in[0].stabilizers[1]  │
        │                      ││rotate_z <qubit> ││ 4     - R: [4]          ││Checks                                          │
        │                      ││                 ││ 5     - CX: [0, 3       ││✓ ["circuit.readouts[0]", "in[0].stabilizers[0]"│
        │                      ││                 ││ 6     - CX: [1, 3]      ││✓ ["circuit.readouts[1]", "in[0].stabilizers[1]"│
        │                      ││                 ││ 7     - CX: [1, 4]      ││✓ ["circuit.readouts[0]", "out[0].stabilizers[0]│
        │                      ││                 ││ 8     - CX: [2, 4]      ││✓ ["circuit.readouts[1]", "out[0].stabilizers[1]│
        │                      ││                 ││ 9     - M: [3]          ││Output stabilizers                              │
        └──────────────────────┘└─────────────────┘└─────────────────────────┘└────────────────────────────────────────────────┘
         ✗ The text does not load: YAML error in <bundle>: did not find expected ',' or ']' at idle.gadget.yaml line 6 column 9,
        ┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
        │YAML error in <bundle>: did not find expected ',' or ']' at idle.gadget.yaml line 6 column 9, while parsing a flow    │
        │sequence at idle.gadget.yaml line 5 column 11                                                                         │
        │idle.gadget.yaml line 5. Everything is computed again as you type; Enter keeps the line, Esc puts it back.            │
        └──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
         Enter  keep   Esc  cancel   ←→  move  o bits  q quit
    "#]].assert_eq(&screen(&mut app, 120, 30));
    press(&mut app, "⌫9]\n");
    assert!(app.load_error.is_none());
    expect![[r#"
        What each measurement measures
          readouts[0] M 3 (Z_0) is random
          readouts[1] M 4 (Z_0) = in[0].stabilizers[1]
        Checks
        ✗ ["circuit.readouts[0]", "in[0].stabilizers[0]"]  sums to in[0].stabilizers[0] ⊕ random@6, not 0
        ✓ ["circuit.readouts[1]", "in[0].stabilizers[1]"]
        ✗ ["circuit.readouts[0]", "out[0].stabilizers[0]"]  sums to in[0].stabilizers[0] ⊕ random@6, not 0
        ✓ ["circuit.readouts[1]", "out[0].stabilizers[1]"]
        Output stabilizers
        ✓ out[0].stabilizers[0] = in[0].stabilizers[0]
        ✓ out[0].stabilizers[1] = in[0].stabilizers[1]
        Does it implement the instruction?
        ✗ the output does not hold `X_0 X_ref0`
        ! call 2: `9` is used before any call gives it a state
    "#]].assert_eq(&findings(&mut app));
}

#[test]
fn saving_writes_the_text_back_with_its_comments() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/repetition3/repetition3.qodec.yaml");
    let dir = std::env::temp_dir().join(format!("qodec-hi-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let path = dir.join("repetition3.qodec.yaml");
    std::fs::copy(&source, &path).expect("copying the example");
    let (docs, source) = crate::bundle::load(&path).expect("the copy reads");
    let mut app = App::open(docs, Some(source), &[], None).expect("the copy opens");
    app.select(&Item::Instruction(0, 0));
    app.focus = Focus::Findings;
    press(&mut app, "A");
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(!app.dirty());
    let saved = std::fs::read_to_string(&path).expect("the saved file");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(saved.starts_with("# repetition3 — single-file qodec bundle."));
    assert!(saved.contains("out[0].stabilizers[1]"));
    let again = Qodec::from_bundle_str(&saved).expect("the saved qodec loads");
    let (gadget, lower) = crate::workbench::gadget_of(&again, 0, 0).expect("prepare_z");
    let a = crate::analysis::analyze(gadget, lower).expect("the circuit reads");
    assert_eq!(a.problems(), 0);
}

#[test]
fn every_part_of_every_example_draws_in_the_workbench() {
    for path in EXAMPLES {
        let mut app = workbench(path);
        let rows = app.rows();
        for row in rows {
            app.select(&row.item);
            for focus in app.panes() {
                app.focus = focus;
                screen(&mut app, 120, 34);
            }
            if let Item::Instruction(k, i) = row.item
                && app.has_gadget(k, i)
            {
                app.focus = crate::app::Focus::Circuit;
                press(&mut app, "→→→→→→→→→→→→");
                screen(&mut app, 120, 34);
                screen(&mut app, 60, 20);
            }
        }
    }
}

#[test]
fn a_shared_file_is_edited_in_place_and_saved_to_itself() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let dir = std::env::temp_dir().join(format!("qodec-hi-shared-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("surface")).expect("a temporary directory");
    let bundle = dir.join("surface/surface.qodec.yaml");
    let shared = dir.join("stim.isa.yaml");
    std::fs::copy(examples.join("surface/surface.qodec.yaml"), &bundle).expect("copying");
    std::fs::copy(examples.join("stim.isa.yaml"), &shared).expect("copying");
    let bundle_before = std::fs::read_to_string(&bundle).expect("the bundle");

    let (docs, source) = crate::bundle::load(&bundle).expect("the copy reads");
    let stim = docs
        .iter()
        .find(|d| d.name == "../stim.isa.yaml")
        .expect("the shared file is a document");
    assert!(stim.external);
    let mut app = App::open(docs, Some(source), &[], None).expect("the copy opens");
    // The bottom layer's text is the shared file.
    app.select(&Item::Layer(1));
    app.focus = Focus::Text;
    app.text_line = 0;
    press(&mut app, "\n (edited)\n");
    assert!(app.load_error.is_none(), "{:?}", app.load_error);
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let message = app.message.clone().unwrap_or_default();
    let bundle_after = std::fs::read_to_string(&bundle).expect("the bundle");
    let shared_after = std::fs::read_to_string(&shared).expect("the shared file");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(message.starts_with("Saved `"), "{message}");
    assert_eq!(bundle_before, bundle_after);
    assert!(
        shared_after
            .lines()
            .next()
            .is_some_and(|l| l.ends_with(" (edited)"))
    );
}
