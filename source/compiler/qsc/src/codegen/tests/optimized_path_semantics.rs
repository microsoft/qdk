// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A semantic oracle for the post-partial-evaluation optimization stage.
//!
//! The `.out` behavior goldens do not cover this stage, and the reason they
//! look reassuring is the reason they are not evidence: the default path never
//! enters the stage, so a golden that has not moved says nothing about what the
//! stage does. Every other check on this path compares QIR text, a stage
//! report, or a gate count, all of which a semantically wrong pass would
//! satisfy.
//!
//! This runs each program twice at the same seed, once through `get_qir` and
//! once through `get_qir_at` at `Default`, and compares what the two emitted
//! programs actually do. Cancelling a self-inverse pair must not change a
//! measurement outcome, and at a fixed seed the outcomes are drawn from
//! identical amplitudes, so the comparison is exact equality rather than a
//! distribution test.
//!
//! The evaluator understands straight-line QIR and refuses everything else by
//! name. What it cannot evaluate is reported as a row rather than dropped, so
//! the oracle's reach is a figure in the snapshot instead of a silent filter.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use expect_test::expect;
use qsc_data_structures::{
    language_features::LanguageFeatures,
    source::SourceMap,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_eval::backend::{Backend, SparseSim};

const SEED: u64 = 42;

fn assert_fir_preserves_failure(body: &str, before_entry: bool, nested: bool, reason: &str) {
    let wrapper = format!("operation Wrapper(q : Qubit, n : Int) : Unit {{ {body} H(q); H(q); }}");
    let call = if nested {
        "Outer(q, 0);"
    } else {
        "Wrapper(q, 0);"
    };
    let entry =
        format!("@EntryPoint() operation Main() : Unit {{ use q = Qubit(); {call} Reset(q); }}");
    let outer = if nested {
        "operation Outer(q : Qubit, n : Int) : Unit { Wrapper(q, n); }"
    } else {
        ""
    };
    let source = if before_entry {
        format!("{wrapper} {outer} {entry}")
    } else {
        format!("{entry} {outer} {wrapper}")
    };
    assert_fir_source_failure(&source, reason);
}

fn assert_fir_source_failure(source: &str, reason: &str) {
    use qsc_codegen::pass_pipeline::{OptimizationLevel, OptimizationLevels};
    for levels in [
        OptimizationLevels::OFF,
        OptimizationLevels::at_fir(OptimizationLevel::Default),
    ] {
        let capabilities = Profile::AdaptiveRI.into();
        let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
        let errors = crate::codegen::qir::get_qir_reporting(
            SourceMap::new([("trap.qs".into(), source.into())], None),
            LanguageFeatures::default(),
            capabilities,
            store,
            &[(std_id, None)],
            levels,
        )
        .expect_err("optimization must preserve the evaluation failure");
        let rendered = format!("{errors:?}");
        assert!(
            rendered.contains("EvaluationFailed"),
            "{levels:?}: {rendered}"
        );
        assert!(rendered.contains(reason), "{levels:?}: {rendered}");
    }
}

#[test]
fn fir_cancellation_preserves_failure_in_an_unused_call_argument() {
    assert_fir_source_failure(
        r"
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let n = 0;
            Wrapper(q, 1 / n);
            Reset(q);
        }
        operation Wrapper(q : Qubit, unused : Int) : Unit { H(q); H(q); }
    ",
        "division by zero",
    );
}

#[test]
fn fir_cancellation_preserves_division_failure_after_entry_declaration() {
    assert_fir_preserves_failure("let _ = 1 / n;", false, false, "division by zero");
}

#[test]
fn fir_cancellation_preserves_division_failure_before_entry_declaration() {
    assert_fir_preserves_failure("let _ = 1 / n;", true, false, "division by zero");
}

#[test]
fn fir_cancellation_preserves_nested_division_failure() {
    assert_fir_preserves_failure("let _ = 1 / n;", false, true, "division by zero");
}

#[test]
fn fir_cancellation_preserves_index_failure() {
    assert_fir_preserves_failure("let _ = [1][n + 2];", false, false, "index out of range: 2");
}

#[test]
fn fir_cancellation_preserves_modulus_failure() {
    assert_fir_preserves_failure("let _ = 1 % n;", false, false, "division by zero");
}

#[test]
fn fir_cancellation_preserves_negative_array_length_failure() {
    assert_fir_preserves_failure(
        "let _ = [1, size = n - 1];",
        false,
        false,
        "invalid array length: -1",
    );
}

#[test]
fn fir_cancellation_preserves_negative_precision_failure() {
    assert_fir_preserves_failure(
        "let _ = Microsoft.Quantum.Convert.DoubleAsStringWithPrecision(1.0, n - 1);",
        false,
        false,
        "negative integers cannot be used here: -1",
    );
}

#[test]
fn fir_cancellation_preserves_negative_exponent_failure() {
    assert_fir_preserves_failure(
        "let _ = 2 ^ (n - 1);",
        false,
        false,
        "negative integers cannot be used here: -1",
    );
}

#[test]
fn fir_safe_cancellation_still_removes_a_wrapper_with_total_evaluation() {
    use qsc_codegen::pass_pipeline::{OptimizationLevel, OptimizationLevels};
    let source = r"
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Wrapper(q, 1);
            Reset(q);
        }
        operation Wrapper(q : Qubit, n : Int) : Unit {
            let _ = n + 1;
            H(q); H(q);
        }
    ";
    let capabilities = Profile::AdaptiveRI.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    let (qir, fir, rir) = crate::codegen::qir::get_qir_reporting(
        SourceMap::new([("total.qs".into(), source.into())], None),
        LanguageFeatures::default(),
        capabilities,
        store,
        &[(std_id, None)],
        OptimizationLevels::at_fir(OptimizationLevel::Default),
    )
    .expect("total wrapper should still compile and optimize");
    assert!(fir.edits > 0, "{fir:?}");
    assert!(!rir.changed, "{rir:?}");
    assert!(!qir.contains("__quantum__qis__h__body"), "{qir}");
}

/// The programs this oracle runs. Chosen because the stage accepts all four and
/// removes a pair from each, so a semantic difference would have somewhere to
/// hide.
const CORPUS: [&str; 4] = [
    "BernsteinVaziraniNISQ",
    "DeutschJozsaNISQ",
    "Functors",
    "MeasureAndReuse",
];

fn corpus_source(name: &str) -> String {
    std::fs::read_to_string(corpus_path("input", name, "qs"))
        .expect("corpus input should be readable")
}

fn corpus_path(leaf: &str, name: &str, extension: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../qdk_package/tests-integration/resources/adaptive_ri")
        .join(leaf)
        .join(format!("{name}.{extension}"))
}

/// The committed seed-42 behavior golden, in the same form this evaluator
/// records.
///
/// The goldens are produced by a different simulator through a different
/// runner, so agreeing with them is the one check that says this evaluator is
/// reading the program rather than agreeing with itself.
fn behavior_golden(name: &str) -> String {
    let text = std::fs::read_to_string(corpus_path("output", name, "out"))
        .expect("every corpus input should have a behavior golden");
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != "OUTPUT" {
                return None;
            }
            match fields.next()? {
                "TUPLE" | "ARRAY" => Some(';'),
                "RESULT" => Some(if fields.next()? == "1" { '1' } else { '0' }),
                _ => None,
            }
        })
        .collect()
}

fn compile(source: &str, levels: qsc_codegen::pass_pipeline::OptimizationLevels) -> String {
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    crate::codegen::qir::get_qir_at(
        sources,
        LanguageFeatures::default(),
        capabilities,
        store,
        &[(std_id, None)],
        levels,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "failed to generate QIR:\n{}",
            super::format_interpret_errors(errors)
        )
    })
}

/// Runs one emitted QIR program and returns what it recorded.
///
/// Returns the construct's name rather than a guess when the program leaves
/// what this understands. A silently skipped instruction would make two
/// different programs agree, which is the one failure an oracle may not have.
fn run_qir(qir: &str, seed: u64) -> Result<String, String> {
    let mut sim = SparseSim::new();
    sim.set_seed(Some(seed));
    let mut qubits: BTreeMap<u64, usize> = BTreeMap::new();
    let mut results: BTreeMap<u64, bool> = BTreeMap::new();
    let mut recorded = String::new();
    let mut in_entry = false;

    for line in qir.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("define ") {
            in_entry = trimmed.contains("@ENTRYPOINT__main");
            continue;
        }
        if !in_entry {
            continue;
        }
        if trimmed == "}" {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with("ret ") || trimmed.ends_with(':') {
            continue;
        }
        if !trimmed.starts_with("call ") {
            return Err(format!("not straight-line: {trimmed}"));
        }
        let Some(at) = trimmed.find('@') else {
            return Err(format!("unparsed call: {trimmed}"));
        };
        let Some(open) = trimmed[at..].find('(').map(|i| i + at) else {
            return Err(format!("unparsed call: {trimmed}"));
        };
        let symbol = &trimmed[at + 1..open];
        let args = &trimmed[open + 1..];
        let qs = indices(args, "%Qubit*");
        let rs = indices(args, "%Result*");

        match symbol {
            "__quantum__rt__initialize" => {}
            "__quantum__rt__tuple_record_output" | "__quantum__rt__array_record_output" => {
                recorded.push(';');
            }
            "__quantum__rt__result_record_output" => {
                let value = results
                    .get(&rs[0])
                    .ok_or_else(|| format!("result {} recorded before it was measured", rs[0]))?;
                recorded.push(if *value { '1' } else { '0' });
            }
            _ => {
                let Some(rest) = symbol.strip_prefix("__quantum__qis__") else {
                    return Err(format!("unknown runtime call: {symbol}"));
                };
                let split = rest
                    .rfind("__")
                    .ok_or_else(|| format!("unnamed: {symbol}"))?;
                let (gate, variant) = (&rest[..split], &rest[split + 2..]);
                let q: Vec<usize> = qs
                    .iter()
                    .map(|index| allocate(&mut sim, &mut qubits, *index))
                    .collect();
                match (gate, variant) {
                    ("h", "body") => sim.h(q[0]),
                    ("x", "body") => sim.x(q[0]),
                    ("y", "body") => sim.y(q[0]),
                    ("z", "body") => sim.z(q[0]),
                    ("s", "body") => sim.s(q[0]),
                    ("s", "adj") => sim.sadj(q[0]),
                    ("t", "body") => sim.t(q[0]),
                    ("t", "adj") => sim.tadj(q[0]),
                    ("sx", "body") => sim.sx(q[0]),
                    ("cx", "body") => sim.cx(q[0], q[1]),
                    ("cy", "body") => sim.cy(q[0], q[1]),
                    ("cz", "body") => sim.cz(q[0], q[1]),
                    ("swap", "body") => sim.swap(q[0], q[1]),
                    ("ccx", "body") => sim.ccx(q[0], q[1], q[2]),
                    ("reset", "body") => sim.reset(q[0]),
                    ("m" | "mresetz", "body") => {
                        let outcome = if gate == "m" {
                            sim.m(q[0])
                        } else {
                            sim.mresetz(q[0])
                        }
                        .map_err(|e| format!("measurement failed: {e}"))?;
                        results.insert(rs[0], outcome.unwrap_bool());
                        Ok(())
                    }
                    _ => return Err(format!("unsupported intrinsic: {symbol}")),
                }
                .map_err(|e| format!("{symbol} failed: {e}"))?;
            }
        }
    }
    Ok(recorded)
}

/// Maps a QIR qubit index onto a simulator qubit, allocating on first use.
fn allocate(sim: &mut SparseSim, qubits: &mut BTreeMap<u64, usize>, index: u64) -> usize {
    *qubits.entry(index).or_insert_with(|| {
        sim.qubit_allocate()
            .expect("the simulator allocates on demand")
    })
}

/// The constant indices an argument list names under one pointer type.
fn indices(args: &str, pointer: &str) -> Vec<u64> {
    let needle = format!("{pointer} inttoptr (i64 ");
    let mut found = Vec::new();
    let mut rest = args;
    while let Some(start) = rest.find(&needle) {
        let tail = &rest[start + needle.len()..];
        let end = tail.find(' ').unwrap_or(tail.len());
        if let Ok(index) = tail[..end].parse::<u64>() {
            found.push(index);
        }
        rest = &tail[end..];
    }
    found
}

/// Compiles every corpus program twice, once with nothing running and once
/// with `levels`, and reports whether the two agree at seed 42.
///
/// `changes_every_program` is what separates an oracle from a comparison of a
/// program with itself. The post-partial-evaluation stage changes all four, so
/// its report demands that; the FIR stage does not, so its report records which
/// programs it reached instead of failing on the ones it declined.
fn differential_report(
    levels: qsc_codegen::pass_pipeline::OptimizationLevels,
    changes_every_program: bool,
) -> String {
    let mut report = String::new();
    for name in CORPUS {
        let source = corpus_source(name);
        let plain = compile(&source, qsc_codegen::pass_pipeline::OptimizationLevels::OFF);
        let optimized = compile(&source, levels);
        if changes_every_program {
            assert_ne!(
                plain, optimized,
                "{name} is meant to be a program the stage changes, and the two paths emitted the same text"
            );
        } else if plain == optimized {
            writeln!(report, "{name}: unchanged, so nothing to compare")
                .expect("writing to a string does not fail");
            continue;
        }

        let outcome = match (run_qir(&plain, SEED), run_qir(&optimized, SEED)) {
            (Ok(before), Ok(after)) => {
                assert_eq!(
                    before,
                    behavior_golden(name),
                    "{name}: this evaluator disagrees with the committed seed-42 behavior golden, so it is not an oracle"
                );
                if before == after {
                    format!("agree on {before}")
                } else {
                    format!("DISAGREE: {before} then {after}")
                }
            }
            (Err(why), _) | (_, Err(why)) => format!("not evaluated: {why}"),
        };
        writeln!(report, "{name}: {outcome}").expect("writing to a string does not fail");
    }
    report
}

#[test]
fn the_optimized_path_records_what_the_default_path_records() {
    expect![[r#"
        BernsteinVaziraniNISQ: agree on ;10101
        DeutschJozsaNISQ: agree on ;;1000;0000
        Functors: agree on ;;00;11;00
        MeasureAndReuse: agree on ;01100
    "#]]
    .assert_eq(&differential_report(
        qsc_codegen::pass_pipeline::OptimizationLevels::at_rir(
            qsc_codegen::pass_pipeline::OptimizationLevel::Default,
        ),
        true,
    ));
}

#[test]
fn the_fir_optimized_path_records_what_the_default_path_records() {
    // The same oracle over the same corpus at the same seed, asking for the FIR
    // stage instead of the one below it. The `unchanged` rows are the honest
    // half of the result rather than a skip: the FIR producer accepts
    // straight-line bodies only, so a corpus program whose bodies it declines
    // emits identical text and there is no pair of runs to compare. Recorded
    // here so the reach of this stage over real programs is a checked figure
    // instead of an assumption, and so a body it starts accepting shows up as a
    // change to this snapshot.
    expect![[r#"
        BernsteinVaziraniNISQ: unchanged, so nothing to compare
        DeutschJozsaNISQ: unchanged, so nothing to compare
        Functors: unchanged, so nothing to compare
        MeasureAndReuse: unchanged, so nothing to compare
    "#]]
    .assert_eq(&differential_report(
        qsc_codegen::pass_pipeline::OptimizationLevels::at_fir(
            qsc_codegen::pass_pipeline::OptimizationLevel::Default,
        ),
        false,
    ));
}

/// Programs the FIR stage does reach, for the half of the oracle the golden
/// corpus cannot supply.
///
/// The measurement sits in the entry and the cancelling pair in a called body,
/// because the producer refuses a body holding a measurement: a `Result` at FIR
/// is a value bound to a local, and the vocabulary's store name carries a
/// register index that does not exist until partial evaluation makes one.
///
/// These carry no committed `.out` golden, so they are not anchored the way the
/// corpus programs are. They do not need to be: the evaluator is anchored by
/// the test above, which reproduces all four committed goldens, and what these
/// add is a pair of runs to compare rather than a second anchor.
const FIR_REACHED: [(&str, &str); 4] = [
    (
        "a pair before a gate that stays",
        r"
        namespace Test {
            operation Body(q : Qubit) : Unit {
                H(q);
                H(q);
                X(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Body(q);
                return MResetZ(q);
            }
        }
        ",
    ),
    (
        "a pair after a gate that stays",
        r"
        namespace Test {
            operation Body(q : Qubit) : Unit {
                X(q);
                H(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Body(q);
                return MResetZ(q);
            }
        }
        ",
    ),
    (
        "a pair on one qubit of two",
        r"
        namespace Test {
            operation Body(a : Qubit, b : Qubit) : Unit {
                X(a);
                H(b);
                H(b);
            }
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[2];
                Body(qs[0], qs[1]);
                return [MResetZ(qs[0]), MResetZ(qs[1])];
            }
        }
        ",
    ),
    (
        // The theta path, end to end. The pair sits in the interior of the loop
        // body, with a gate on each side of it, because a pair at either edge
        // holds a port the loop's interface binds and the rule declines it.
        // One edit here removes eight gates from the emitted program.
        "a pair inside a loop body",
        r"
        namespace Test {
            operation Body(q : Qubit) : Unit {
                for _ in 1..4 {
                    T(q);
                    H(q);
                    H(q);
                    S(q);
                }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Body(q);
                return MResetZ(q);
            }
        }
        ",
    ),
];

#[test]
fn the_fir_stage_preserves_behavior_on_the_programs_it_reaches() {
    let mut report = String::new();
    for (label, source) in FIR_REACHED {
        let plain = compile(source, qsc_codegen::pass_pipeline::OptimizationLevels::OFF);
        let optimized = compile(
            source,
            qsc_codegen::pass_pipeline::OptimizationLevels::at_fir(
                qsc_codegen::pass_pipeline::OptimizationLevel::Default,
            ),
        );
        assert_ne!(
            plain, optimized,
            "{label} is here because the FIR stage reaches it, and the two paths emitted the same text"
        );

        let outcome = match (run_qir(&plain, SEED), run_qir(&optimized, SEED)) {
            (Ok(before), Ok(after)) if before == after => format!("agree on {before}"),
            (Ok(before), Ok(after)) => format!("DISAGREE: {before} then {after}"),
            (Err(why), _) | (_, Err(why)) => format!("not evaluated: {why}"),
        };
        writeln!(report, "{label}: {outcome}").expect("writing to a string does not fail");
    }
    expect![[r#"
        a pair before a gate that stays: agree on 1
        a pair after a gate that stays: agree on 1
        a pair on one qubit of two: agree on ;10
        a pair inside a loop body: agree on 0
    "#]]
    .assert_eq(&report);
}
