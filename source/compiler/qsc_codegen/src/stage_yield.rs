// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What each optimization stage removes from the same programs.
//!
//! The FIR stage's rationale is that a redundancy removed before inlining and
//! unrolling is never copied, so removal there is multiplicative rather than
//! additive. That is an assertion until someone counts, and this module counts
//! it: every program below is compiled to QIR three ways, with no stage, with
//! the FIR stage, and with the post-partial-evaluation stage, and the gates in
//! the emitted QIR are the one currency all three share.
//!
//! Both stages are driven from here rather than from the Python harness that
//! carries the circuit pipeline's baseline. That harness names two stages,
//! `graph` and `circuit`, and both are measurements of a single circuit run; it
//! reaches neither compiler stage. Separate baselines keep compiler-stage
//! measurements distinct from circuit-pipeline measurements.
//!
//! # What the multiplicative claim actually predicts
//!
//! Not a bigger reduction. Both stages remove the same gates when both accept,
//! because the post-partial-evaluation stage sees every copy inlining made and
//! removes every one of them. What differs is the price. The claim is about the
//! exchange rate between edits and gates, so the column that carries it is
//! `per edit`: gates removed divided by statements the FIR return leg removed.
//! A body optimized once and instantiated ten times pays one edit for ten
//! gates; the same reduction below costs ten edits. At the other stage that
//! ratio is one by construction, since its return leg subtracts exactly the
//! instructions its rule retired.
//!
//! # Two bounds the figures carry
//!
//! The stage runs after monomorphization and defunctionalization, which
//! duplicate bodies before any invariant level the pipeline checks, so whatever
//! those two copied is already lost when the producer arrives. Every ratio here
//! is a floor on the claim rather than the claim itself.
//!
//! Producer refusals and loop-interface guards also bound the benefit. The
//! corpus measures both an interior loop-body pair that can be retired and an
//! edge pair that the FIR stage leaves alone to preserve the carried binding.

use expect_test::expect;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_fir::fir::{PackageId, PackageStore};
use qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize;
use qsc_fir_transforms::test_utils::{assert_no_pipeline_errors, compile_to_fir};
use qsc_partial_eval::ProgramEntry;
use qsc_qhdg::declares_backedge;
use qsc_qhdg::verify::binds_loop_interface;

use crate::fir_optimize::{StageReport, optimize};
use crate::fir_qhdg::lower_callable_with_census;
use crate::pass_pipeline::OptimizationLevel;
use crate::qir::fir_to_qir_at;

/// The Base profile, where partial evaluation unrolls every loop and inlines
/// every call. Multiplicativity is a claim about that expansion, so measuring
/// it anywhere the expansion does not happen would measure something else.
const CAPABILITIES: TargetCapabilityFlags = TargetCapabilityFlags::empty();

/// Generates QIR from a store the transform pipeline has finished with.
///
/// This is the codegen path's own sequence — resource analysis, then the entry
/// expression with its execution graph, then partial evaluation — run over a
/// store the stage may have edited.
fn qir_of(store: &PackageStore, package: PackageId, level: OptimizationLevel) -> String {
    let compute_properties = qsc_rca::Analyzer::init(store, CAPABILITIES).analyze_all();
    let fir_package = store.get(package);
    let entry = ProgramEntry {
        exec_graph: fir_package.entry_exec_graph.clone(),
        expr: (
            package,
            fir_package.entry.expect("the program has an entry"),
        )
            .into(),
    };
    fir_to_qir_at(store, CAPABILITIES, &compute_properties, &entry, level)
        .expect("the program should generate QIR")
}

/// Quantum instructions in emitted QIR.
///
/// Count gates rather than characters so renaming a symbol cannot change the
/// measured reduction.
fn gates(qir: &str) -> usize {
    qir.lines()
        .filter(|line| line.trim_start().starts_with("call void @__quantum__qis__"))
        .count()
}

/// What the FIR stage did with one program.
#[derive(Default)]
struct FirStage {
    /// Statements the return leg removed.
    edits: usize,
    /// Bodies the producer or the return leg declined.
    declined: usize,
}

/// Runs the FIR stage over every callable in the package, from inside the
/// pipeline's own optimization point.
///
/// The assembly is `fir_optimize::optimize` rather than a copy of it. A second
/// wiring here would be a second definition of what the stage is, and this
/// measurement would then be pricing something other than what a caller runs.
fn run_fir_stage(store: &mut PackageStore, package: PackageId) -> FirStage {
    // This table measures removals, not the number of bodies left unchanged.
    let StageReport {
        edits, declined, ..
    } = optimize(store, package, OptimizationLevel::Default);
    FirStage { edits, declined }
}

/// One program measured at both stages.
struct Row {
    label: &'static str,
    /// Gates emitted with neither stage running.
    baseline: usize,
    /// Gates emitted after the FIR stage ran.
    at_fir: usize,
    /// Gates emitted after the post-partial-evaluation stage ran.
    at_rir: usize,
    /// Statements the FIR return leg removed.
    fir_edits: usize,
    /// Bodies the FIR stage declined.
    fir_declined: usize,
}

fn measure(label: &'static str, source: &str) -> Row {
    // Compiled twice rather than reusing one store, so the untouched figure
    // comes from a run that never entered the stage rather than from a
    // snapshot taken before it.
    let (mut untouched, untouched_package) = compile_to_fir(source);
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut untouched, untouched_package, |_, _| {});
    assert_no_pipeline_errors("stage_yield::untouched", &result.errors);

    let (mut edited, edited_package) = compile_to_fir(source);
    let mut stage = FirStage::default();
    let result = run_pipeline_with_diagnostics_and_optimize(
        &mut edited,
        edited_package,
        |store, package| {
            stage = run_fir_stage(store, package);
        },
    );
    assert_no_pipeline_errors("stage_yield::edited", &result.errors);

    Row {
        label,
        baseline: gates(&qir_of(
            &untouched,
            untouched_package,
            OptimizationLevel::None,
        )),
        at_fir: gates(&qir_of(&edited, edited_package, OptimizationLevel::None)),
        at_rir: gates(&qir_of(
            &untouched,
            untouched_package,
            OptimizationLevel::Default,
        )),
        fir_edits: stage.edits,
        fir_declined: stage.declined,
    }
}

fn table(rows: &[Row]) -> String {
    use std::fmt::Write as _;

    let mut rendered = format!(
        "{:<38} {:>5} {:>5} {:>5} {:>5} {:>8} {:>8}\n",
        "program", "gates", "FIR", "RIR", "edits", "per edit", "declined"
    );
    for row in rows {
        let removed = row.baseline - row.at_fir;
        // Tenths in integer arithmetic. A ratio of counts has no business
        // arriving through a float.
        let per_edit = (removed * 10).checked_div(row.fir_edits).map_or_else(
            || "-".to_string(),
            |tenths| format!("{}.{}", tenths / 10, tenths % 10),
        );
        writeln!(
            rendered,
            "{:<38} {:>5} {:>5} {:>5} {:>5} {:>8} {:>8}",
            row.label,
            row.baseline,
            row.at_fir,
            row.at_rir,
            row.fir_edits,
            per_edit,
            row.fir_declined,
        )
        .expect("writing to a string cannot fail");
    }
    rendered
}

/// A pair in a body, with a gate beside it that does not cancel.
///
/// The surviving `T` is load-bearing for the measurement rather than decoration.
/// Without it every node of the calling statement would be retired at once and
/// the whole call would be removed, which is sound but would price a call
/// removal rather than a gate pair.
const CALLED_ONCE: &str = r"
    operation Body(q : Qubit) : Unit {
        H(q);
        H(q);
        T(q);
    }
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        Body(q);
    }
    ";

/// The same body, reached ten times. This is the claim, stated as a program.
const CALLED_TEN_TIMES: &str = r"
    operation Body(q : Qubit) : Unit {
        H(q);
        H(q);
        T(q);
    }
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        for _ in 1..10 {
            Body(q);
        }
    }
    ";

/// A pair in the entry body itself, reached once and copied by nothing.
const IN_THE_ENTRY: &str = r"
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        H(q);
        H(q);
        T(q);
    }
    ";

/// A pair neither call holds on its own. The producer walks into a callee, so
/// what it compares is the body as reached rather than the body as written.
const ACROSS_TWO_CALLS: &str = r"
    operation Flip(q : Qubit) : Unit {
        H(q);
    }
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        Flip(q);
        Flip(q);
        T(q);
    }
    ";

/// A pair at the leading edge of a loop body.
///
/// The producer now lowers this into a theta, and the pair still does not go.
/// `H(q)` is the body's first use of the wire, so its input port is the
/// region's carried binding, and `binds_loop_interface` declines a pair that
/// holds a port the loop's contract names. The zero here is a correct guard
/// firing rather than a producer that would not look.
const BEHIND_CONTROL_FLOW: &str = r"
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        for _ in 1..4 {
            H(q);
            H(q);
            T(q);
        }
    }
    ";

/// A pair in the interior of a loop body, with a gate on each side of it.
///
/// The shape that pays. Neither half of the pair is the body's first use of the
/// wire or its last production, so neither guard names it. No sample program in
/// the corpus supplies this shape, which is why it is written here rather than
/// found.
const INTERIOR_PAIR_IN_A_LOOP: &str = r"
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        for _ in 1..4 {
            T(q);
            H(q);
            H(q);
            S(q);
        }
    }
    ";

fn corpus() -> Vec<Row> {
    vec![
        measure("a pair in a body called once", CALLED_ONCE),
        measure("the same body called ten times", CALLED_TEN_TIMES),
        measure("a pair in the entry body", IN_THE_ENTRY),
        measure("a pair only adjacent across two calls", ACROSS_TWO_CALLS),
        measure("a pair at the edge of a loop body", BEHIND_CONTROL_FLOW),
        measure("a pair inside a loop body", INTERIOR_PAIR_IN_A_LOOP),
    ]
}

#[test]
fn what_each_stage_removes_from_the_same_programs() {
    // The measurement, recorded rather than described. `gates` is the untouched
    // program, `FIR` and `RIR` are what each stage left, `edits` is how many
    // statements the FIR return leg removed to get there, and `per edit` is the
    // exchange rate the multiplicative claim is about.
    //
    // The last two rows are the loop half. The edge pair does not go and the
    // interior pair does, at four gates per edit against one for a straight
    // line, which is the multiplicative claim stated over iterations rather
    // than over call sites. Neither zero in the edge row is a refusal: the body
    // lowers, and a guard declines the pair.
    expect![[r#"
        program                                gates   FIR   RIR edits per edit declined
        a pair in a body called once               3     1     1     2      1.0        0
        the same body called ten times            30    10    10     2     10.0        0
        a pair in the entry body                   3     1     1     2      1.0        0
        a pair only adjacent across two calls      3     1     1     2      1.0        0
        a pair at the edge of a loop body         12    12     4     0        -        0
        a pair inside a loop body                 16     8     8     2      4.0        0
    "#]]
    .assert_eq(&table(&corpus()));
}

#[test]
fn the_two_stages_reach_the_same_program_size_wherever_both_accept() {
    // The half of the claim that turns out not to hold. Removal before inlining
    // is not a larger reduction, because the stage below sees every copy the
    // inlining made and removes every one of them. Anywhere both stages accept,
    // the emitted program is the same size.
    for row in corpus() {
        if row.fir_edits == 0 {
            continue;
        }
        assert_eq!(
            row.at_fir, row.at_rir,
            "'{}' came out at {} gates through the FIR stage and {} through the \
             other, so the two stages disagree about what the program reduces to",
            row.label, row.at_fir, row.at_rir
        );
    }
}

#[test]
fn one_edit_to_a_body_removes_a_gate_from_every_instantiation() {
    // The half that does. The same body, the same rule, the same edit count,
    // and ten times the gates removed, which is what "multiplicative" names.
    let once = measure("once", CALLED_ONCE);
    let ten = measure("ten", CALLED_TEN_TIMES);

    assert_eq!(
        once.fir_edits, ten.fir_edits,
        "the body is optimized once either way, so the edit count should not move"
    );
    assert_eq!(
        (ten.baseline - ten.at_fir) / (once.baseline - once.at_fir),
        10,
        "ten instantiations should give ten times the reduction for the same edit"
    );
}

#[test]
fn a_zero_inside_a_loop_body_names_the_guard_that_produced_it() {
    // The bound stated as a check, and attributed rather than asserted. The
    // producer used to refuse this body outright; it now lowers it into a theta
    // and the pair still does not go, so a bare zero here would invite the
    // conclusion that the capability does not work.
    //
    // The rewrite's `find` returns a bare `Ok(None)` at every guard with no
    // reason channel, so the attribution is made by calling the two `pub`
    // predicates on the produced graph. They are `pub` for exactly this: a
    // rewrite that answered the question for itself and a verifier that
    // answered it here would be two answers to one question.
    let edge_pair = measure("edge", BEHIND_CONTROL_FLOW);
    assert_eq!(
        edge_pair.fir_declined, 0,
        "the body lowers now, so the zero below is not a refusal"
    );
    assert_eq!(edge_pair.fir_edits, 0, "and the pair still does not go");
    assert_eq!(
        edge_pair.at_fir, edge_pair.baseline,
        "so nothing is removed"
    );

    let (holds_interface, joined_by_backedge) = pair_attribution(BEHIND_CONTROL_FLOW);
    assert!(
        holds_interface,
        "the pair's first half is the body's first use of the wire, so its \
         input port is the region's carried binding and `binds_loop_interface` \
         declines it"
    );
    assert!(
        !joined_by_backedge,
        "and the two halves are adjacent inside the body rather than across the \
         back edge, which distinguishes the two guards cleanly"
    );

    assert!(
        edge_pair.at_rir < edge_pair.baseline,
        "while the other stage, which sees the loop unrolled, takes it"
    );
}

#[test]
fn an_interior_pair_inside_a_loop_body_is_removed_and_the_body_survives() {
    // The shape that pays, which no sample program in the corpus supplies. One
    // edit against four iterations, with the gates on either side of the pair
    // left alone.
    let interior = measure("interior", INTERIOR_PAIR_IN_A_LOOP);

    assert_eq!(interior.fir_declined, 0, "the body lowers");
    assert!(
        interior.fir_edits > 0,
        "and the interior pair is removed rather than declined"
    );
    assert!(
        interior.at_fir < interior.baseline,
        "which reaches the emitted program: {} gates became {}",
        interior.baseline,
        interior.at_fir
    );

    let (holds_interface, joined_by_backedge) = pair_attribution(INTERIOR_PAIR_IN_A_LOOP);
    assert!(
        !holds_interface,
        "neither half of an interior pair is named by the loop's contract"
    );
    assert!(
        !joined_by_backedge,
        "nor are they joined across the back edge"
    );
}

/// Whether the first adjacent same-operation pair in a lowered body holds a
/// port the loop's contract names, and whether its two halves are joined by a
/// declared back edge.
///
/// The two questions the one registered rule asks before it builds a patch,
/// asked here through the same `pub` predicates so a zero can be attributed to
/// the guard that produced it.
fn pair_attribution(source: &str) -> (bool, bool) {
    let (mut store, package) = compile_to_fir(source);
    let mut attribution = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let item = store
                .get(package)
                .items
                .iter()
                .find_map(|(id, item)| match &item.kind {
                    qsc_fir::fir::ItemKind::Callable(decl) if &*decl.name.name == "Main" => {
                        Some(id)
                    }
                    _ => None,
                })
                .expect("the program has a Main");
            let lowered =
                lower_callable_with_census(store, qsc_fir::fir::StoreItemId { package, item })
                    .0
                    .expect("the body lowers");
            let graph = &lowered.graph;

            // The first `quantum.h` and the next node holding the same wire.
            let first = graph
                .nodes()
                .find(|node| node.operation.0 == "quantum.h")
                .expect("the body holds an h");
            let output = *first.outputs.first().expect("a gate produces a capability");
            let edge = graph
                .edges()
                .iter()
                .find(|edge| edge.source == output)
                .expect("the capability went somewhere");
            let input = *first.inputs.first().expect("a gate consumes a capability");

            attribution = Some((
                binds_loop_interface(graph, input) || binds_loop_interface(graph, edge.target),
                declares_backedge(graph, edge),
            ));
        });
    assert_no_pipeline_errors("stage_yield::pair_attribution", &result.errors);
    attribution.expect("the optimization point should have been reached")
}
