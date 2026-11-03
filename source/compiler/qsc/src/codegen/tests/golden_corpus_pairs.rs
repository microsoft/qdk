// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The other half of the golden-corpus pre-commitment.
//!
//! Before the cancellation rule was enabled, the adjacent self-inverse pairs in
//! the `adaptive_ri` corpus were enumerated and fixed: 27 candidates the rule
//! is allowed to take, and a larger excluded set that must survive untouched.
//! Pre-committing an expected set is only half a measurement. This is the
//! comparison afterward, and it is the reason the pre-commitment was stated as
//! a candidate set rather than a prediction: which candidates actually go
//! depends on which programs the producer accepts, and that could not be known
//! until the stage was wired in.
//!
//! One scanner reads both sides, so a difference between them is a difference
//! in the program rather than in how it was counted. Adjacency follows the
//! delivered rule rather than the text: two calls to the same operation with
//! identical qubit operands, in the same basic block, with no instruction
//! between them touching any of those operands.
//!
//! Two further classes are counted here, and neither is a pre-commitment. They
//! are measurements taken to settle what a wider rule would be worth, and the
//! stage is expected to leave both alone because nothing delivers either one.
//! An adjoint pair is a gate applied and then undone, `s` followed by `s_adj`
//! on the same operands. A commuting-separated pair is a self-inverse pair on
//! one wire with only operations that commute with it in between, where
//! commuting means both the pair and everything between it are declared
//! diagonal in the computational basis. That basis is borrowed from the
//! compiler rather than stated here, and it cannot see commutation that
//! depends on operand role, such as `x q1` around `cx(q0,q1)`, so a zero is a
//! statement about this basis and not about commutation in general.
//!
//! Each class reports a population beside its pairable
//! count, because a bare zero cannot be told apart from a class the corpus
//! never exercised, and the two answer different questions: a zero pairable
//! count beside a nonzero population closes the option for this corpus, while
//! a zero population means the corpus never put it to the test.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::PathBuf,
};

use expect_test::expect;
use qsc_data_structures::{
    language_features::LanguageFeatures,
    source::SourceMap,
    target::{Profile, TargetCapabilityFlags},
};

/// One call the scanner cares about, reduced to what adjacency is defined over.
struct Call {
    gate: String,
    suffix: String,
    qubits: Vec<u64>,
    /// Whether the call carries a classical argument, which `qubits` cannot see.
    ///
    /// Read from the argument list rather than from a list of gate names, so a
    /// gate that gains an angle later is covered without editing this file.
    angled: bool,
}

/// The pairs found in one program, split by whether the rule may take them.
///
/// The last two classes are measurements rather than pre-commitments: nothing
/// in the compiler delivers either one, so the stage is expected to leave them
/// alone. Each is a population beside a pairable count, because a bare zero
/// cannot be told apart from a class the corpus never exercised.
#[derive(Default)]
struct Pairs {
    candidates: Vec<String>,
    excluded: Vec<String>,
    adjoint: Vec<String>,
    adjoint_calls: usize,
    commuting: Vec<String>,
    commuting_separated: usize,
}

fn corpus_dir(leaf: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../qdk_package/tests-integration/resources/adaptive_ri")
        .join(leaf)
}

/// Splits QIR into basic blocks, keeping only the calls that name a qubit.
///
/// A label, a `define`, and a closing brace all end a block. Anything else that
/// is not a call is dropped, because adjacency is defined over what touches an
/// operand and a non-call touches none.
fn blocks(qir: &str) -> Vec<Vec<Call>> {
    let mut all = Vec::new();
    let mut current: Vec<Call> = Vec::new();
    for line in qir.lines() {
        let trimmed = line.trim();
        let ends_block = trimmed == "}"
            || trimmed.starts_with("define ")
            || (line.starts_with(|c: char| c.is_alphanumeric() || c == '_')
                && trimmed.ends_with(':'));
        if ends_block {
            if !current.is_empty() {
                all.push(std::mem::take(&mut current));
            }
            continue;
        }
        let Some(call) = parse_call(trimmed) else {
            continue;
        };
        if !call.qubits.is_empty() {
            current.push(call);
        }
    }
    if !current.is_empty() {
        all.push(current);
    }
    all
}

fn parse_call(trimmed: &str) -> Option<Call> {
    if !trimmed.starts_with("call ") {
        return None;
    }
    let at = trimmed.find('@')?;
    let open = trimmed[at..].find('(')? + at;
    let symbol = &trimmed[at + 1..open];
    let args = &trimmed[open + 1..];
    let rest = symbol.strip_prefix("__quantum__qis__")?;
    let split = rest.rfind("__")?;
    Some(Call {
        gate: rest[..split].to_string(),
        suffix: rest[split + 2..].to_string(),
        qubits: qubit_operands(args),
        angled: args.contains("double "),
    })
}

/// The qubit indices an argument list names, in the order it names them.
///
/// Order matters and a set would lose it: `cx(a,b)` then `cx(b,a)` touches the
/// same qubits and is not the identity.
fn qubit_operands(args: &str) -> Vec<u64> {
    let mut found = Vec::new();
    let mut rest = args;
    while let Some(start) = rest.find("%Qubit* inttoptr (i64 ") {
        let tail = &rest[start + "%Qubit* inttoptr (i64 ".len()..];
        let end = tail.find(' ').unwrap_or(tail.len());
        if let Ok(index) = tail[..end].parse::<u64>() {
            found.push(index);
        }
        rest = &tail[end..];
    }
    found
}

/// Whether this position seeds a same-wire repeat with something in between,
/// and if so whether everything in between is declared diagonal.
///
/// `None` means there is no separated repeat, so the position is either an
/// adjacent candidate already counted elsewhere or nothing at all. `Some(true)`
/// means a commutation-aware rule could reach the pair; `Some(false)` means a
/// non-commuting operation blocks it. Both are counted, because a pairable
/// count of zero beside a population of zero would not say whether the case is
/// worth supporting or simply absent from this corpus.
///
/// The walk continues past a blocking operation rather than returning early,
/// because the population figure needs the repeat found either way.
fn separated_repeat(block: &[Call], index: usize, diagonal: &BTreeSet<String>) -> Option<bool> {
    let call = &block[index];
    let mut clear = true;
    let mut separated = false;
    for other in &block[index + 1..] {
        if !other.qubits.iter().any(|q| call.qubits.contains(q)) {
            continue;
        }
        if other.gate == call.gate && other.suffix == call.suffix && other.qubits == call.qubits {
            return separated.then_some(clear);
        }
        clear &= diagonal.contains(&format!("quantum.{}", other.gate));
        separated = true;
    }
    None
}

/// Every adjacent same-operation pair, split by whether the vocabulary calls
/// the operation self-inverse.
///
/// Overlapping runs are counted as the pre-commitment counted them, so four
/// consecutive `t` gates are three pairs rather than two. It makes no
/// difference to the candidate set, because every candidate run in this corpus
/// is exactly two long, and it is what keeps the excluded figure comparable.
fn pairs_in(qir: &str) -> Pairs {
    // The same vocabulary the rule consults, so this cannot drift from it.
    let vocabulary = qsc_codegen::pass_pipeline::circuit_inverses();
    // Commutation is borrowed, not stated here: two operations diagonal in the
    // computational basis commute, and this is the same declaration the
    // delivered diagonal-collapse pass binds. A name that does not resolve is
    // absent from the set and so counts as not commuting, which is the
    // direction that declines a pair rather than inventing one.
    let diagonal = qsc_codegen::pass_pipeline::circuit_diagonal_operations();
    let mut pairs = Pairs::default();
    for block in blocks(qir) {
        for (index, call) in block.iter().enumerate() {
            if call.suffix == "adj" {
                pairs.adjoint_calls += 1;
            }
            let seed = format!("quantum.{}", call.gate);
            // The pair itself must be diagonal, not only what separates it: a
            // diagonal operation does not commute with an `x` pair.
            if call.suffix == "body"
                && vocabulary.are_inverse(&seed, &seed)
                && diagonal.contains(&seed)
                && let Some(clear) = separated_repeat(&block, index, &diagonal)
            {
                pairs.commuting_separated += 1;
                if clear {
                    pairs
                        .commuting
                        .push(format!("{}__{}", call.gate, call.suffix));
                }
            }
            let Some(next) = block[index + 1..]
                .iter()
                .find(|other| other.qubits.iter().any(|q| call.qubits.contains(q)))
            else {
                continue;
            };
            if next.gate != call.gate || next.qubits != call.qubits {
                continue;
            }
            let name = format!("{}__{}", call.gate, call.suffix);
            if next.suffix != call.suffix {
                // A gate applied and then undone. Deliberately ahead of the
                // vocabulary: `circuit_inverses` is built from `SELF_INVERSE`
                // alone, so it denies every adjoint relation and the class has
                // to be stated structurally until delivery widens.
                //
                // An angle-carrying call is not counted, because `qubits`
                // drops every `double` and two rotations by different angles
                // would otherwise compare equal.
                let opposed = matches!(
                    (call.suffix.as_str(), next.suffix.as_str()),
                    ("body", "adj") | ("adj", "body")
                );
                if opposed && !call.angled && !next.angled {
                    pairs.adjoint.push(name);
                }
                continue;
            }
            let term = format!("quantum.{}", call.gate);
            if call.suffix == "body" && vocabulary.are_inverse(&term, &term) {
                pairs.candidates.push(name);
            } else {
                pairs.excluded.push(name);
            }
        }
    }
    pairs
}

/// Compiles one corpus program with the post-partial-evaluation stage asked to
/// optimize, which no production caller does by default, and reports what the
/// stage did with it.
fn optimized_qir(source: &str) -> (String, Option<String>) {
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    let (qir, _, report) = crate::codegen::qir::get_qir_reporting(
        sources,
        LanguageFeatures::default(),
        capabilities,
        store,
        &[(std_id, None)],
        qsc_codegen::pass_pipeline::OptimizationLevels::at_rir(
            qsc_codegen::pass_pipeline::OptimizationLevel::Default,
        ),
    )
    .unwrap_or_else(|errors| {
        panic!(
            "failed to generate QIR:\n{}",
            super::format_interpret_errors(errors)
        )
    });
    (qir, report.declined)
}

#[test]
fn the_enumerated_pair_set_is_compared_against_what_the_stage_actually_removed() {
    let inputs = corpus_dir("input");
    let goldens = corpus_dir("output");
    let mut names: Vec<String> = std::fs::read_dir(&inputs)
        .expect("corpus input directory should exist")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".qs").map(ToString::to_string)
        })
        .collect();
    names.sort();

    let mut before: BTreeMap<String, usize> = BTreeMap::new();
    let mut after: BTreeMap<String, usize> = BTreeMap::new();
    let mut why: BTreeMap<String, String> = BTreeMap::new();
    let mut adjoint: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut commuting = [0usize; 4];
    let mut excluded_before = 0usize;
    let mut excluded_after = 0usize;

    for name in &names {
        let golden = std::fs::read_to_string(goldens.join(format!("{name}.ll")))
            .expect("every corpus input should have a golden");
        let source = std::fs::read_to_string(inputs.join(format!("{name}.qs")))
            .expect("corpus input should be readable");
        let golden_pairs = pairs_in(&golden);
        let (optimized, declined) = optimized_qir(&source);
        let optimized_pairs = pairs_in(&optimized);

        excluded_before += golden_pairs.excluded.len();
        excluded_after += optimized_pairs.excluded.len();
        commuting[0] += golden_pairs.commuting_separated;
        commuting[1] += optimized_pairs.commuting_separated;
        commuting[2] += golden_pairs.commuting.len();
        commuting[3] += optimized_pairs.commuting.len();
        if golden_pairs.adjoint_calls > 0 || optimized_pairs.adjoint_calls > 0 {
            adjoint.insert(
                name.clone(),
                [
                    golden_pairs.adjoint_calls,
                    optimized_pairs.adjoint_calls,
                    golden_pairs.adjoint.len(),
                    optimized_pairs.adjoint.len(),
                ],
            );
        }
        if !golden_pairs.candidates.is_empty()
            || !optimized_pairs.candidates.is_empty()
            || golden_pairs.adjoint_calls > 0
            || optimized_pairs.adjoint_calls > 0
        {
            before.insert(name.clone(), golden_pairs.candidates.len());
            after.insert(name.clone(), optimized_pairs.candidates.len());
            why.insert(
                name.clone(),
                declined.unwrap_or_else(|| "accepted".to_string()),
            );
        }
    }

    // Every excluded pair must survive. This is the assertion that matters
    // most, because the difference between a correct pass and a silent
    // miscompile is whether `t;t` and `s;s` are left alone.
    assert_eq!(
        excluded_before, excluded_after,
        "the stage took a pair the vocabulary does not call self-inverse"
    );

    let enumerated: usize = before.values().sum();
    let surviving: usize = after.values().sum();
    let mut report = String::new();
    for name in before.keys() {
        writeln!(
            report,
            "{name}: {} -> {} ({})",
            before[name], after[name], why[name]
        )
        .expect("writing to a string does not fail");
    }
    writeln!(
        report,
        "enumerated {enumerated}, removed {}, surviving {surviving}, excluded {excluded_before} unchanged",
        enumerated - surviving
    )
    .expect("writing to a string does not fail");
    for (name, counts) in &adjoint {
        writeln!(
            report,
            "adjoint {name}: {} -> {} calls, {} -> {} paired ({})",
            counts[0], counts[1], counts[2], counts[3], why[name]
        )
        .expect("writing to a string does not fail");
    }
    let adjoint_total = |slot: usize| adjoint.values().map(|counts| counts[slot]).sum::<usize>();
    writeln!(
        report,
        "adjoint total: {} -> {} calls, {} -> {} paired",
        adjoint_total(0),
        adjoint_total(1),
        adjoint_total(2),
        adjoint_total(3)
    )
    .expect("writing to a string does not fail");
    writeln!(
        report,
        "commuting-separated total: {} -> {} separated, {} -> {} all-diagonal",
        commuting[0], commuting[1], commuting[2], commuting[3]
    )
    .expect("writing to a string does not fail");
    // The adjoint figures read `golden -> optimized`. The one that matters is
    // `Functors: 0 -> 1 paired`: the stage creates an adjoint pair that it
    // cannot then take. On qubit 7 the golden interleaves two conjugation
    // blocks as `s__adj, h, s__body`; cancelling the surrounding `h;h` brings
    // `s__adj` and `s__body` together, and delivery stops at `SELF_INVERSE`, so
    // the pair survives. The opportunity is manufactured by the optimizer
    // rather than written by the author, which is why scanning source or
    // goldens alone reports zero.
    //
    // The commuting-separated line reads the other way, and closes rather than
    // opens: the corpus does separate diagonal self-inverse pairs on a wire
    // eight times, and not once is everything in between diagonal. So the
    // population exists and the opportunity does not, which is the evidence a
    // commutation-aware rule would need and does not have.
    expect![[r#"
        BernsteinVaziraniNISQ: 2 -> 0 (accepted)
        DeutschJozsaNISQ: 7 -> 0 (accepted)
        Functors: 5 -> 0 (accepted)
        LoopWithMutableVariableUpdates: 2 -> 0 (accepted)
        MeasureAndReuse: 1 -> 0 (accepted)
        NestedBranching: 12 -> 12 (variable 6 is written inside a branch arm, and nothing merges the arms)
        enumerated 29, removed 17, surviving 12, excluded 92 unchanged
        adjoint Functors: 6 -> 6 calls, 0 -> 1 paired (accepted)
        adjoint MeasureAndReuse: 2 -> 2 calls, 0 -> 0 paired (accepted)
        adjoint total: 8 -> 8 calls, 0 -> 1 paired
        commuting-separated total: 8 -> 8 separated, 0 -> 0 all-diagonal
    "#]]
    .assert_eq(&report);
}
