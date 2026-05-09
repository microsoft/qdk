// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use regex_lite::{Captures, Regex};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write;

use crate::{
    Circuit, Operation,
    circuit::{ComponentGrid, Ket, Measurement, Qubit, Register, SourceLocation, Unitary},
    json_to_circuit::json_to_circuits,
};

pub fn circuits_to_qsharp(file_name: &str, circuits_json: &str) -> Result<String, String> {
    let safe_name = sanitize_qsharp_identifier(file_name);
    json_to_circuits(circuits_json).map(|circuits| build_circuits(&safe_name, &circuits.circuits))
}

/// Coerce an arbitrary string (typically a `.qsc` file basename) into a
/// valid Q# identifier suitable for use as an operation name.
///
/// The caller is the host's circuit-to-Q# bridge, which in normal use
/// passes a stripped file basename. Filenames are essentially unconstrained
/// — they can contain `.`, spaces, leading digits, unicode, etc. — but the
/// emitter uses this string verbatim as the operation name (`operation
/// <name>(qs : Qubit[]) : ...`), and Q# requires a valid identifier there.
/// Without sanitization, perfectly reasonable filenames like
/// `GroupSplittingTest.Main.qsc` blow up downstream with a syntax error.
///
/// Rules applied (deliberately conservative — preserves ASCII letters,
/// digits and underscore as-is, replaces everything else with `_`):
/// * Each char is kept if it is ASCII alphanumeric or `_`, otherwise `_`.
/// * If the result is empty or starts with a digit, a `_` is prefixed.
///
/// Stable across calls so two circuits that map to the same sanitized name
/// will collide in the same way every time, which is the right behaviour
/// for a deterministic emitter — the alternative (hashing the original
/// name into the result) would silently change `operation` names every
/// release if our hash impl ever changed.
fn sanitize_qsharp_identifier(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn build_circuits(file_name: &str, circuits: &[Circuit]) -> String {
    // Names of operations the caller already plans to emit. We must not let
    // an extracted custom-gate definition collide with one of these — the
    // user's `Foo` circuit and an extracted `Foo` body would each compile
    // to a duplicate definition.
    let mut emitted_names: FxHashSet<String> = FxHashSet::default();

    // Queue of custom-gate definitions discovered while walking the bodies.
    // First-occurrence wins; duplicates are silently dropped via the
    // companion `seen` set so even cyclic-looking trace data terminates.
    let mut extraction_queue: Vec<ExtractedDef> = vec![];
    let mut seen_extractions: FxHashSet<String> = FxHashSet::default();

    let mut qsharp_str = String::new();

    // Pre-compute the global set of custom-gate names. Every call site of
    // these names (anywhere in any circuit, including transitively inside
    // extracted bodies) must use the array-wrap calling convention because
    // every extracted operation has signature `(qs : Qubit[])`. We compute
    // this set ONCE up front so `operation_call` has consistent information
    // when emitting the very first body — by the time we drain the
    // extraction queue, the top-level body has already been emitted, so a
    // progressively-built set wouldn't help calls in the main op that
    // refer to gates only discovered later in the walk.
    let mut custom_gates: FxHashSet<String> = FxHashSet::default();

    if circuits.len() == 1 {
        emitted_names.insert(file_name.to_string());
        // Reserve the main name *before* walking so an in-body call to a
        // gate with the same name (rare, but legal in tests/snapshots)
        // doesn't try to extract the main op as one of its own callees.
        seen_extractions.insert(file_name.to_string());
        let working = unwrap_trace_entry_point_wrapper(&circuits[0]);
        collect_custom_gate_names(&working.component_grid, &mut custom_gates);
        qsharp_str.push_str(&build_operation_def(file_name, &working, &custom_gates));
        collect_custom_gate_defs(
            &working.component_grid,
            &mut extraction_queue,
            &mut seen_extractions,
        );
    } else {
        for index in 0..circuits.len() {
            let name = format!("{file_name}{index}");
            emitted_names.insert(name.clone());
            seen_extractions.insert(name);
        }
        // Walk every circuit first to assemble the union of all custom
        // gates. Only then do we start emitting — by that point the set
        // covers every name `operation_call` could encounter.
        let workings: Vec<Circuit> = circuits
            .iter()
            .map(unwrap_trace_entry_point_wrapper)
            .collect();
        for working in &workings {
            collect_custom_gate_names(&working.component_grid, &mut custom_gates);
        }
        for (index, working) in workings.iter().enumerate() {
            let circuit_name = format!("{file_name}{index}");
            qsharp_str.push_str(&build_operation_def(&circuit_name, working, &custom_gates));
            collect_custom_gate_defs(
                &working.component_grid,
                &mut extraction_queue,
                &mut seen_extractions,
            );
        }
    }

    // Drain the queue. Each extracted definition's own body is walked for
    // further custom-gate calls (transitive closure: if Foo calls Bar, both
    // Foo and Bar end up emitted exactly once). The `seen_extractions` set
    // doubles as the dedup guard, so this terminates in O(distinct gates).
    let mut i = 0;
    while i < extraction_queue.len() {
        let def = &extraction_queue[i];
        let name = def.name.clone();
        let synth = synthesize_circuit_for_extraction(&def.children, &def.targets);

        // The synthesized body may itself reference custom gates we haven't
        // seen yet (the trace recorded them only inside this extracted
        // body, never at the top level). Fold them into the global set
        // before emitting so the resulting Q# uses array-wrap calls
        // uniformly.
        collect_custom_gate_names(&synth.component_grid, &mut custom_gates);

        // Note: build_operation_def does its own divergence-banner pass on
        // the synthesized body, so any opaque conditionals or non-uniform
        // loops *inside* a custom gate get their own banner above its
        // declaration.
        qsharp_str.push_str(&build_operation_def(&name, &synth, &custom_gates));
        emitted_names.insert(name);

        // Walk this extracted body for further nested gates. We pass the
        // outgoing queue/seen set so any new finds are appended to the same
        // queue and processed in subsequent loop iterations.
        collect_custom_gate_defs(
            &synth.component_grid,
            &mut extraction_queue,
            &mut seen_extractions,
        );

        i += 1;
    }

    let _ = emitted_names; // currently informational; reserved for future collision diagnostics
    qsharp_str
}

/// Strip the trace-derived "entry-point wrapper" if present, returning a
/// `Circuit` whose top-level grid is the wrapper's body.
///
/// A trace always wraps the entire body in a single outer `Unitary` call to
/// the entry-point operation (e.g. `Main`). That wrapper is a trace artifact
/// — there's no Q# operation by that name in the user's saved file — so
/// emitting it as a call would produce a reference to a non-existent
/// operation and skip the body entirely. Editor-authored circuits never
/// produce this shape (custom-gate calls in editor circuits don't carry
/// their body as `children`).
///
/// This unwrap fires *exactly once* at the top of `build_circuits`, before
/// custom-gate extraction. Crucially, it does NOT recurse into extracted
/// callees: when extraction synthesizes `operation Foo` from a custom-gate
/// call's `children`, that body is real user code (the operation's actual
/// body as recorded by the trace) and must not be unwrapped further. If we
/// applied this heuristic recursively, a `Foo` whose body is just a single
/// call to `Bar` would have its `Bar` call eaten — a real bug we'd hit
/// every time the user defines a thin wrapper operation.
///
/// Detection: a top-level grid with exactly one column, exactly one
/// component, where that component is a `Unitary` with non-empty `children`
/// AND a name that does NOT identify a structural group (loops,
/// conditionals, anonymous scopes — those are real circuit structure that
/// must be preserved). Any deviation from this exact shape and we leave
/// the circuit untouched.
fn unwrap_trace_entry_point_wrapper(circuit: &Circuit) -> Circuit {
    let grid = &circuit.component_grid;
    let single_col = grid.len() == 1;
    let only = single_col
        .then(|| grid.first().and_then(|c| c.components.first()))
        .flatten();

    let inner = only.and_then(|op| match op {
        Operation::Unitary(u)
            if grid.first().map(|c| c.components.len()) == Some(1)
                && !u.children.is_empty()
                && structural_group_kind(&u.gate).is_none() =>
        {
            Some(u.children.clone())
        }
        _ => None,
    });

    match inner {
        Some(component_grid) => Circuit {
            component_grid,
            qubits: circuit.qubits.clone(),
        },
        None => circuit.clone(),
    }
}

/// One custom-gate definition harvested from a `Unitary` op's `children`.
/// Owns its data so the queue can be drained without juggling lifetimes
/// against the original `Circuit`.
struct ExtractedDef {
    name: String,
    /// The body of the custom gate, exactly as it appeared in the trace.
    children: ComponentGrid,
    /// The targets of the *call* — these define the parameter order of the
    /// extracted operation. Within `children`, qubit IDs equal to
    /// `targets[i].qubit` map to local parameter slot `i`.
    targets: Vec<Register>,
}

/// Walk `grid` and append any custom-gate definitions it carries to
/// `queue`. `seen` deduplicates by gate name (first occurrence wins) and
/// also lets the caller pre-reserve names that should never be extracted
/// (e.g. the main operation's name).
///
/// We recurse into every `Unitary`'s children regardless of whether we
/// already emitted that gate, so a custom `Foo` whose body calls `Bar` is
/// guaranteed to surface `Bar` even if `Foo` itself is being skipped.
fn collect_custom_gate_defs(
    grid: &ComponentGrid,
    queue: &mut Vec<ExtractedDef>,
    seen: &mut FxHashSet<String>,
) {
    for col in grid {
        for op in &col.components {
            // Only `Unitary` ops can carry a `children` body that represents
            // a Q# operation definition. Measurements with children are
            // structural (e.g. measurement-based scopes), not user gates.
            let Operation::Unitary(u) = op else { continue };
            let has_body = !u.children.is_empty();
            if !has_body {
                continue;
            }
            // Structural groups (`loop:`, `if:`, `(N)`, `<lambda>`,
            // `<scope>`) are not user-defined operations — their children
            // get inlined by the emitter. Don't try to extract them; do
            // recurse to find any user gates buried inside.
            let is_structural = structural_group_kind(&u.gate).is_some();
            if !is_structural && seen.insert(u.gate.clone()) {
                queue.push(ExtractedDef {
                    name: u.gate.clone(),
                    children: u.children.clone(),
                    targets: u.targets.clone(),
                });
            }
            collect_custom_gate_defs(&u.children, queue, seen);
        }
    }
}

/// Walk `grid` and insert into `names` every gate name that appears as a
/// custom-operation definition site (i.e. a `Unitary` carrying a non-empty
/// `children` body whose name is not a structural-group marker). Recurses
/// into all child grids unconditionally so transitively-nested custom
/// gates are captured.
///
/// Used by `build_circuits` to assemble — *before any emission* — the
/// global set of gate names that `operation_call` must render with the
/// array-wrap calling convention. Every extracted operation has signature
/// `(qs : Qubit[])`, so any call to such a name (including bare references
/// without children, e.g. the second iteration of a loop) must wrap its
/// targets in a single array literal.
fn collect_custom_gate_names(grid: &ComponentGrid, names: &mut FxHashSet<String>) {
    for col in grid {
        for op in &col.components {
            match op {
                Operation::Unitary(u) => {
                    if !u.children.is_empty() && structural_group_kind(&u.gate).is_none() {
                        names.insert(u.gate.clone());
                    }
                    collect_custom_gate_names(&u.children, names);
                }
                Operation::Measurement(m) => {
                    collect_custom_gate_names(&m.children, names);
                }
                Operation::Ket(k) => {
                    collect_custom_gate_names(&k.children, names);
                }
            }
        }
    }
}

/// Build a synthetic `Circuit` suitable for handing to `build_operation_def`
/// when emitting an extracted custom-gate definition.
///
/// Qubit list = `call_targets` first (so positional parameter order matches
/// the call site `Foo(qs[0], qs[1], …)`), followed by any other qubit IDs
/// the body references but the call didn't pass as a target (controls,
/// internally-allocated qubits captured by the trace, …). Adding those
/// extras keeps `get_qubit_name` from panicking on out-of-range IDs at the
/// cost of a slightly inflated parameter list — acceptable because the
/// alternative would be a hard panic in the live preview.
///
/// `num_results` per qubit is computed by counting `Measurement.results`
/// entries attached to that qubit in the body, so the synthesized
/// operation's return type matches what its body actually produces.
fn synthesize_circuit_for_extraction(
    children: &ComponentGrid,
    call_targets: &[Register],
) -> Circuit {
    let mut qubit_ids: Vec<usize> = vec![];
    let mut seen: FxHashSet<usize> = FxHashSet::default();

    for r in call_targets {
        if seen.insert(r.qubit) {
            qubit_ids.push(r.qubit);
        }
    }

    collect_qubit_ids(children, &mut qubit_ids, &mut seen);

    let mut result_counts: FxHashMap<usize, usize> = FxHashMap::default();
    count_results_per_qubit(children, &mut result_counts);

    let qubits = qubit_ids
        .into_iter()
        .map(|id| Qubit {
            id,
            num_results: result_counts.get(&id).copied().unwrap_or(0),
            declarations: vec![],
        })
        .collect();

    Circuit {
        qubits,
        component_grid: children.clone(),
    }
}

/// Recursively gather every distinct qubit ID referenced anywhere in `grid`
/// (targets, controls, measurement qubits, measurement-result wires).
fn collect_qubit_ids(grid: &ComponentGrid, ids: &mut Vec<usize>, seen: &mut FxHashSet<usize>) {
    for col in grid {
        for op in &col.components {
            match op {
                Operation::Measurement(m) => {
                    for r in &m.qubits {
                        if seen.insert(r.qubit) {
                            ids.push(r.qubit);
                        }
                    }
                    for r in &m.results {
                        if seen.insert(r.qubit) {
                            ids.push(r.qubit);
                        }
                    }
                    collect_qubit_ids(&m.children, ids, seen);
                }
                Operation::Unitary(u) => {
                    for r in &u.targets {
                        if seen.insert(r.qubit) {
                            ids.push(r.qubit);
                        }
                    }
                    for r in &u.controls {
                        if seen.insert(r.qubit) {
                            ids.push(r.qubit);
                        }
                    }
                    collect_qubit_ids(&u.children, ids, seen);
                }
                Operation::Ket(k) => {
                    for r in &k.targets {
                        if seen.insert(r.qubit) {
                            ids.push(r.qubit);
                        }
                    }
                    collect_qubit_ids(&k.children, ids, seen);
                }
            }
        }
    }
}

/// Count measurement-result wires attached to each qubit. Used to size the
/// `num_results` field of synthesized qubits so the extracted operation's
/// return type (`Unit` / `Result` / `Result[]`) is consistent with what
/// `process_components` actually emits inside the body.
fn count_results_per_qubit(grid: &ComponentGrid, counts: &mut FxHashMap<usize, usize>) {
    for col in grid {
        for op in &col.components {
            match op {
                Operation::Measurement(m) => {
                    for r in &m.results {
                        *counts.entry(r.qubit).or_insert(0) += 1;
                    }
                    count_results_per_qubit(&m.children, counts);
                }
                Operation::Unitary(u) => {
                    count_results_per_qubit(&u.children, counts);
                }
                Operation::Ket(k) => {
                    count_results_per_qubit(&k.children, counts);
                }
            }
        }
    }
}

fn build_operation_def(
    circuit_name: &str,
    circuit: &Circuit,
    custom_gates: &FxHashSet<String>,
) -> String {
    let mut indentation_level = 0;
    let qubits = circuit
        .qubits
        .iter()
        .enumerate()
        .map(|(i, q)| (q.id, format!("qs[{i}]")))
        .collect::<FxHashMap<_, _>>();

    let parameter = if qubits.is_empty() {
        String::new()
    } else {
        "qs : Qubit[]".to_string()
    };

    // The return type is determined by the number of qubits "children".
    // However, the actual return statement is determined by the variables storing measurements.
    // If there is an inconsistency between these, which would happen if there was a mismatch between
    // the number of qubit children specified on the circuit and the number of qubit children specified
    // on the measurements, incorrect Q# could be generated.
    let return_type = match circuit.qubits.iter().fold(0, |sum, q| sum + q.num_results) {
        0 => "Unit",
        1 => "Result",
        _ => "Result[]",
    };

    // Check if all operations (recursively) are unitaries — only then can the
    // emitted operation declare `is Ctl + Adj`. We have to descend into
    // structural groups (loops, conditionals) because a measurement nested
    // inside a loop disqualifies the operation just as much as a measurement
    // at the top level.
    let is_ctl_adj = grid_is_all_unitary(&circuit.component_grid);

    let characteristics = if is_ctl_adj { "is Ctl + Adj " } else { "" };
    let summary = if qubits.is_empty() {
        String::new()
    } else {
        format!(
            "/// Expects a qubit register of at least {} qubits.\n",
            qubits.len()
        )
    };

    let mut qsharp_str = format!(
        "{summary}operation {circuit_name}({parameter}) : {return_type} {characteristics}{{\n"
    );
    indentation_level += 1;

    let mut measure_results = vec![];
    let indent = "    ".repeat(indentation_level);

    // If there are operation, add an assert for the number of qubits
    if !circuit.component_grid.is_empty()
        && circuit
            .component_grid
            .iter()
            .any(|col| !col.components.is_empty())
    {
        qsharp_str.push_str(&generate_qubit_validation(
            circuit_name,
            &qubits,
            indentation_level,
        ));
    }

    let mut body_str = String::new();
    let mut should_add_pi = false;

    let body_grid = &circuit.component_grid;

    // Scan the body for trace-only patterns the emitter can't faithfully
    // recreate as Q# (e.g. loops with structurally divergent iterations,
    // conditionals with opaque expressions). Any findings become a banner
    // above the operation declaration so the reader knows the preview is
    // approximate. Editor-authored circuits never trigger this banner
    // because they only contain shapes the emitter can recreate exactly.
    let divergence_banner = format_divergence_banner(&detect_divergences(body_grid));

    process_components(
        body_grid,
        &qubits,
        indentation_level,
        &mut measure_results,
        &mut should_add_pi,
        &mut body_str,
        custom_gates,
    );

    if should_add_pi {
        // Add the π constant
        writeln!(qsharp_str, "{indent}let π = Std.Math.PI();")
            .expect("could not write to qsharp_str");
    }

    qsharp_str.push_str(body_str.as_str());
    qsharp_str.push_str(&generate_return_statement(&mut measure_results, &indent));
    qsharp_str.push_str("}\n\n");
    // Prepend the divergence banner (if any) so it sits above the doc-comment
    // and operation declaration. Computed earlier from the same body grid we
    // emitted, so its line references stay consistent with what the user sees.
    if divergence_banner.is_empty() {
        qsharp_str
    } else {
        format!("{divergence_banner}{qsharp_str}")
    }
}

/// Recursively emits Q# for the given grid of components into `out`.
///
/// Most operations emit a single call. The exception is structural groups
/// (loops, conditionals, anonymous scopes, loop-iteration wrappers) — these
/// don't correspond to real Q# operations, so calling them by name would
/// produce code that doesn't compile. Instead we recurse into their children
/// and surface the structure as Q# comments. As the editor learns to author
/// these constructs natively (loops, conditionals, …), each case here will
/// graduate from a `// loop: …` comment to a real `for` / `if` block.
///
/// Custom-gate groups (e.g. `Foo` with a `children` expansion of its body)
/// are *not* treated as structural — the call to `Foo` is what we want to
/// preserve, and the user's project supplies the body.
#[allow(clippy::too_many_arguments)]
fn process_components(
    grid: &ComponentGrid,
    qubits: &FxHashMap<usize, String>,
    indentation_level: usize,
    measure_results: &mut Vec<(String, (usize, usize))>,
    should_add_pi: &mut bool,
    out: &mut String,
    custom_gates: &FxHashSet<String>,
) {
    let indent = "    ".repeat(indentation_level);
    for col in grid {
        for op in &col.components {
            // Structural groups are inlined rather than emitted as a call.
            if let Operation::Unitary(u) = op
                && !u.children.is_empty()
                && let Some(kind) = structural_group_kind(&u.gate)
            {
                emit_structural_group(
                    kind,
                    &u.gate,
                    &u.children,
                    qubits,
                    indentation_level,
                    measure_results,
                    should_add_pi,
                    out,
                    custom_gates,
                );
                continue;
            }

            match &op {
                Operation::Measurement(measurement) => {
                    out.push_str(&generate_measurement_call(
                        measurement,
                        qubits,
                        &indent,
                        measure_results,
                    ));
                }
                Operation::Unitary(unitary) => {
                    out.push_str(&generate_unitary_call(
                        unitary,
                        qubits,
                        &indent,
                        custom_gates,
                    ));
                }
                Operation::Ket(ket) => {
                    out.push_str(&generate_ket_call(ket, qubits, &indent));
                }
            }

            // Look for a "π" in the args
            let args = op.args();
            if !*should_add_pi && !args.is_empty() {
                *should_add_pi = args.iter().any(|arg| arg.contains("π"));
            }
        }
    }
}

/// Categorization of structural group names produced by the circuit tracer.
/// Any variant other than [`StructuralGroupKind::Iteration`] gets a
/// human-readable comment header in the emitted Q#.
#[derive(Clone, Copy)]
enum StructuralGroupKind {
    Loop,
    Conditional,
    /// A loop-iteration wrapper such as `(0)`, `(1)`. Its children are the
    /// iteration body and we recurse silently — adding visible markers for
    /// every iteration would dwarf the actual Q#.
    Iteration,
    /// `<lambda>`, `<scope>`, or any other compiler-synthesized scope label
    /// that doesn't map to a callable.
    AnonymousScope,
}

fn structural_group_kind(name: &str) -> Option<StructuralGroupKind> {
    if name.starts_with("loop:") {
        Some(StructuralGroupKind::Loop)
    } else if name.starts_with("if:") {
        Some(StructuralGroupKind::Conditional)
    } else if is_iteration_marker(name) {
        Some(StructuralGroupKind::Iteration)
    } else if name == "<lambda>" || name == "<scope>" {
        Some(StructuralGroupKind::AnonymousScope)
    } else {
        None
    }
}

/// Matches a loop-iteration wrapper name like `(0)`, `(12)`. We deliberately
/// require ASCII digits and the literal parens so we don't accidentally
/// classify a custom gate named e.g. `(Reset)` as an iteration wrapper.
fn is_iteration_marker(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 3
        && bytes[0] == b'('
        && bytes[bytes.len() - 1] == b')'
        && bytes[1..bytes.len() - 1].iter().all(u8::is_ascii_digit)
}

#[allow(clippy::too_many_arguments)]
fn emit_structural_group(
    kind: StructuralGroupKind,
    name: &str,
    children: &ComponentGrid,
    qubits: &FxHashMap<usize, String>,
    indentation_level: usize,
    measure_results: &mut Vec<(String, (usize, usize))>,
    should_add_pi: &mut bool,
    out: &mut String,
    custom_gates: &FxHashSet<String>,
) {
    let indent = "    ".repeat(indentation_level);

    // Iteration markers emit a single header comment with no closing
    // marker — the next iteration (or the enclosing `// end loop`) closes
    // the visual scope. Keeping them visible (rather than transparent) is
    // important when loop iterations are structurally different: without
    // these markers there's no way for the reader to tell which gates
    // belong to which iteration.
    if matches!(kind, StructuralGroupKind::Iteration) {
        writeln!(out, "{indent}// iteration {name}").expect("could not write to out");
        process_components(
            children,
            qubits,
            indentation_level,
            measure_results,
            should_add_pi,
            out,
            custom_gates,
        );
        return;
    }

    let footer = match kind {
        StructuralGroupKind::Loop => "// end loop",
        StructuralGroupKind::Conditional => "// end if",
        StructuralGroupKind::AnonymousScope => "// end scope",
        StructuralGroupKind::Iteration => unreachable!("handled above"),
    };
    writeln!(out, "{indent}// {name}").expect("could not write to out");
    process_components(
        children,
        qubits,
        indentation_level,
        measure_results,
        should_add_pi,
        out,
        custom_gates,
    );
    writeln!(out, "{indent}{footer}").expect("could not write to out");
}

/// Returns true iff every operation in `grid` (and recursively in any
/// children) is a [`Operation::Unitary`]. Used to decide whether the emitted
/// operation can declare `is Ctl + Adj`.
fn grid_is_all_unitary(grid: &ComponentGrid) -> bool {
    grid.iter().all(|col| {
        col.components
            .iter()
            .all(|op| matches!(op, Operation::Unitary(_)) && grid_is_all_unitary(op.children()))
    })
}

// ---------------------------------------------------------------------------
// Trace-divergence detection
//
// The trace-derived form of a circuit can contain shapes that the editor
// (and therefore the emitter) can't recreate exactly as Q#:
//
//   * `loop:` groups whose iterations are structurally different — produced
//     when partial evaluation specialises iterations differently (e.g. an
//     `if` body gets eliminated in iteration 0 but appears in iteration 2).
//     A `for` loop has one body, so no `for` we emit could reproduce these
//     iterations as-is. The recursive emitter already prints them as
//     unrolled `// iteration (N)` blocks; the banner just calibrates the
//     reader's expectation.
//
//   * `if:` groups whose label is an opaque expression (e.g.
//     `(f(c_0, c_1)) > (2)`) rather than a literal `c_N == One` / `Zero`
//     comparison. The trace summarises arbitrary classical conditions
//     opaquely; the original Q# expression is lost.
//
// Detection runs on the same grid the emitter prints. Findings are
// surfaced as a single banner above the operation declaration, naming each
// issue and (when available) its source line.
// ---------------------------------------------------------------------------

/// One actionable finding from the divergence detector. Each becomes a
/// bullet line in the banner above the emitted operation.
struct DivergenceFinding {
    kind: DivergenceKind,
    /// The structural group's label as written in the circuit (e.g.
    /// `"loop: 0..3"`, `"if: (f(c_0)) > (2)"`). Surfaced verbatim so the
    /// reader can correlate it with the inline `// loop: ...` / `// if: ...`
    /// comments in the body.
    label: String,
    location: Option<SourceLocation>,
}

#[derive(Clone, Copy)]
enum DivergenceKind {
    DivergentLoopIterations,
    OpaqueConditional,
}

fn detect_divergences(grid: &ComponentGrid) -> Vec<DivergenceFinding> {
    let mut findings = vec![];
    walk_for_divergences(grid, &mut findings);
    findings
}

fn walk_for_divergences(grid: &ComponentGrid, findings: &mut Vec<DivergenceFinding>) {
    for col in grid {
        for op in &col.components {
            if let Operation::Unitary(u) = op
                && !u.children.is_empty()
            {
                match structural_group_kind(&u.gate) {
                    Some(StructuralGroupKind::Loop) => check_loop(u, findings),
                    Some(StructuralGroupKind::Conditional) => check_conditional(u, findings),
                    _ => {}
                }
            }
            // Recurse into structural-group children (loops, conditionals,
            // anonymous scopes, iteration markers) and into measurement/ket
            // children — those get inlined into the current operation's
            // body, so any divergences in them belong on this operation's
            // banner.
            //
            // Skip recursion into non-structural Unitary children (custom
            // gate calls). Those get extracted into their own operation by
            // `collect_custom_gate_defs`, and `build_operation_def` will
            // run divergence detection on the extracted body separately.
            // Recursing here would double-report divergences on both the
            // caller and the callee.
            match op {
                Operation::Unitary(u) if structural_group_kind(&u.gate).is_none() => {}
                _ => walk_for_divergences(op.children(), findings),
            }
        }
    }
}

fn check_loop(loop_op: &Unitary, findings: &mut Vec<DivergenceFinding>) {
    let iterations: Vec<&Unitary> = loop_op
        .children
        .iter()
        .flat_map(|col| col.components.iter())
        .filter_map(|op| match op {
            Operation::Unitary(u) if is_iteration_marker(&u.gate) => Some(u),
            _ => None,
        })
        .collect();

    if iterations.len() < 2 {
        return;
    }

    let first_body = &iterations[0].children;
    let all_equiv = iterations
        .iter()
        .skip(1)
        .all(|it| grids_skeleton_equal(&it.children, first_body));

    if !all_equiv {
        findings.push(DivergenceFinding {
            kind: DivergenceKind::DivergentLoopIterations,
            label: loop_op.gate.clone(),
            location: location_from_metadata(loop_op),
        });
    }
}

fn check_conditional(if_op: &Unitary, findings: &mut Vec<DivergenceFinding>) {
    let label = if_op.gate.trim_start_matches("if:").trim();
    if !is_simple_conditional(label) {
        findings.push(DivergenceFinding {
            kind: DivergenceKind::OpaqueConditional,
            label: if_op.gate.clone(),
            location: location_from_metadata(if_op),
        });
    }
}

/// True iff `label` is a comparison the emitter could reproduce literally:
/// `<identifier> == One` or `<identifier> == Zero`. Anything more complex
/// (function calls, numeric comparisons, conjunctions) is opaque.
fn is_simple_conditional(label: &str) -> bool {
    let Some((lhs, rhs)) = label.split_once("==") else {
        return false;
    };
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    let lhs_is_ident =
        !lhs.is_empty() && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let rhs_is_result = rhs == "One" || rhs == "Zero";
    lhs_is_ident && rhs_is_result
}

/// Two grids are skeleton-equal if their structure matches when register
/// indices are ignored. This is the equivalence we care about for loop
/// iterations: a uniform `for i in 0..N { H(qs[i]); }` produces N
/// iterations whose bodies share the same shape but reference different
/// qubits, and we must consider those equivalent. A divergent loop changes
/// the *shape* — different gates, an extra `if:` group, missing operations.
fn grids_skeleton_equal(a: &ComponentGrid, b: &ComponentGrid) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(ca, cb)| {
        ca.components.len() == cb.components.len()
            && ca
                .components
                .iter()
                .zip(cb.components.iter())
                .all(|(oa, ob)| operations_skeleton_equal(oa, ob))
    })
}

fn operations_skeleton_equal(a: &Operation, b: &Operation) -> bool {
    let same_kind = matches!(
        (a, b),
        (Operation::Measurement(_), Operation::Measurement(_))
            | (Operation::Unitary(_), Operation::Unitary(_))
            | (Operation::Ket(_), Operation::Ket(_))
    );
    if !same_kind {
        return false;
    }
    if a.gate() != b.gate() {
        return false;
    }
    grids_skeleton_equal(a.children(), b.children())
}

/// Pulls the most useful source location off a structural group's metadata.
/// Prefers `scope_location` (the `for`/`if` keyword) over `source` (often
/// the first gate inside the body).
fn location_from_metadata(u: &Unitary) -> Option<SourceLocation> {
    let md = u.metadata.as_ref()?;
    md.scope_location.clone().or_else(|| md.source.clone())
}

fn format_divergence_banner(findings: &[DivergenceFinding]) -> String {
    if findings.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(
        "// NOTE: This Q# preview was reconstructed from a circuit trace and is approximate.\n",
    );
    out.push_str("// The original Q# source is the authoritative version.\n");

    for finding in findings {
        let location_suffix = finding
            .location
            .as_ref()
            // Source locations are 0-indexed; editors display 1-indexed.
            .map(|loc| format!(" (line {})", loc.line + 1))
            .unwrap_or_default();
        let descr = match finding.kind {
            DivergenceKind::DivergentLoopIterations => "loop has structurally divergent iterations",
            DivergenceKind::OpaqueConditional => "conditional uses an opaque expression",
        };
        let _ = writeln!(out, "//   - {descr}{location_suffix}: {}", finding.label);
    }

    out
}

fn generate_qubit_validation(
    circuit_name: &str,
    qubits: &FxHashMap<usize, String>,
    indentation_level: usize,
) -> String {
    if qubits.is_empty() {
        return String::new();
    }

    let indent = "    ".repeat(indentation_level);
    let inner_indent = "    ".repeat(indentation_level + 1);
    format!(
        "{indent}if Length(qs) < {} {{\n\
        {inner_indent}fail \"Invalid number of qubits. Operation {circuit_name} expects a qubit register of at least {} qubits.\";\n\
        {indent}}}\n",
        qubits.len(),
        qubits.len()
    )
}

fn generate_measurement_call(
    measurement: &Measurement,
    qubits: &FxHashMap<usize, String>,
    indent: &str,
    measure_results: &mut Vec<(String, (usize, usize))>,
) -> String {
    let operation_str = measurement_call(measurement, qubits);
    let mut op_results = vec![];
    for reg in &measurement.results {
        if let Some(c_id) = reg.result {
            let result = (format!("c{}_{}", reg.qubit, c_id), (reg.qubit, c_id));
            op_results.push(result.clone());
        }
    }

    // Sort first by q_id, then by c_id
    op_results.sort_by_key(|(_, (q_id, c_id))| (*q_id, *c_id));
    let result = op_results
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    match op_results.len() {
        0 => {
            format!("{indent}{operation_str};\n")
        }
        1 => {
            measure_results.extend(op_results);
            format!("{indent}let {result} = {operation_str};\n")
        }
        _ => {
            measure_results.extend(op_results);
            format!("{indent}let ({result}) = {operation_str};\n")
        }
    }
}

fn generate_unitary_call(
    unitary: &Unitary,
    qubits: &FxHashMap<usize, String>,
    indent: &str,
    custom_gates: &FxHashSet<String>,
) -> String {
    let operation_str = operation_call(unitary, qubits, custom_gates);
    format!("{indent}{operation_str};\n")
}

fn generate_ket_call(ket: &Ket, qubits: &FxHashMap<usize, String>, indent: &str) -> String {
    // Note: The only supported ket operation is "0"
    if ket.gate == "0" {
        let ket_str = ket_call(ket, qubits);
        format!("{indent}{ket_str};\n")
    } else {
        format!(
            "{indent}fail \"Unsupported ket operation: |{}〉\";\n",
            ket.gate
        )
    }
}

fn generate_return_statement(
    measure_results: &mut [(String, (usize, usize))],
    indent: &str,
) -> String {
    if measure_results.is_empty() {
        return String::new();
    }

    measure_results.sort_by_key(|(_, (q_id, c_id))| (*q_id, *c_id));
    if measure_results.len() == 1 {
        let (name, _) = measure_results[0].clone();
        format!("{indent}return {name};\n")
    } else {
        let results = measure_results
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{indent}return [{results}];\n")
    }
}

fn get_qubit_name(qubits: &FxHashMap<usize, String>, q_id: usize) -> String {
    qubits
        .get(&q_id)
        .unwrap_or_else(|| panic!("Qubit with {q_id} not found"))
        .clone()
}

fn measurement_call(measurement: &Measurement, qubits: &FxHashMap<usize, String>) -> String {
    let args = measurement
        .qubits
        .iter()
        .map(|q| get_qubit_name(qubits, q.qubit))
        .collect::<Vec<_>>();
    let args_count = args.len();

    let args = args.join(", ");
    if args_count == 1 {
        format!("M({args})")
    } else {
        // This is a joint measurement operation.
        // For now, assume PauliZ measurement basis for all measurements.
        let bases = vec!["PauliZ"; args_count].join(", ");
        format!("Measure([{bases}], [{args}])")
    }
}

fn ket_call(ket: &Ket, qubits: &FxHashMap<usize, String>) -> String {
    // Note: The only supported ket operation is "0" which is a reset operation
    let targets = ket
        .targets
        .iter()
        .map(|q| get_qubit_name(qubits, q.qubit))
        .collect::<Vec<_>>();
    let args = targets.join(", ");
    format!("Reset({args})")
}

fn operation_call(
    unitary: &Unitary,
    qubits: &FxHashMap<usize, String>,
    custom_gates: &FxHashSet<String>,
) -> String {
    let gate = unitary.gate.as_str();

    let is_controlled = !unitary.controls.is_empty();
    // A "custom" gate is one we extracted (or will extract) into its own
    // operation definition. Every such operation has the canonical
    // signature `(qs : Qubit[])`, so its call sites must wrap targets in
    // a single array literal — `Foo([qs[0], qs[1]])`, not the per-qubit
    // positional form `Foo(qs[0], qs[1])` we use for built-in gates like
    // `H` / `X` whose signatures take individual qubits. The set is
    // computed up front in `build_circuits` so that even a bare reference
    // (e.g. the second iteration of a loop, where the trace dropped the
    // body) renders with the array convention.
    let is_custom = custom_gates.contains(gate);

    let functors = if is_controlled && unitary.is_adjoint {
        "Controlled Adjoint "
    } else if is_controlled {
        "Controlled "
    } else if unitary.is_adjoint {
        "Adjoint "
    } else {
        ""
    };

    let mut args = vec![];

    // Create the regex for matching numbers (both integers and doubles)
    let number_regex = Regex::new(r"((\d+(\.\d*)?)|(\.\d+))").expect("Regex should compile");

    // Convert ints to doubles by appending a `.` to the end of the integer
    for arg in &unitary.args {
        // Replace all numbers in the string
        let updated_arg = number_regex
            .replace_all(arg, |caps: &Captures| {
                let number = &caps[0]; // The matched number
                if number.contains('.') {
                    number.to_string() // If it's already a double, leave it unchanged
                } else {
                    format!("{number}.") // If it's an integer, append a `.`
                }
            })
            .to_string();

        args.push(updated_arg);
    }

    let targets = unitary
        .targets
        .iter()
        .map(|t| get_qubit_name(qubits, t.qubit))
        .collect::<Vec<_>>();
    if is_custom {
        // Single Qubit[] arg — even an empty target list emits `[]` so the
        // arity matches the extracted operation's `(qs : Qubit[])`
        // signature. The controlled branch below sees this as a single
        // arg and won't add a stray tuple wrap, yielding the desired
        // `Controlled Foo([c], [qs[0], qs[1]])`.
        args.push(format!("[{}]", targets.join(", ")));
    } else {
        args.extend(targets);
    }

    if is_controlled {
        let controls = unitary
            .controls
            .iter()
            .filter_map(|c| {
                if c.result.is_none() {
                    Some(get_qubit_name(qubits, c.qubit))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let controls = format!("[{controls}]");
        let args_count = args.len();
        let mut inner_args = args.join(", ");
        if args_count != 1 {
            inner_args = format!("({inner_args})");
        }
        args = vec![controls, inner_args];
    }

    let args = args.join(", ");
    format!("{functors}{gate}({args})")
}
