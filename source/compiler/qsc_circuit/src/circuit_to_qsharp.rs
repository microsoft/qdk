// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use regex_lite::{Captures, Regex};
use rustc_hash::FxHashMap;
use std::fmt::Write;

use crate::{
    Circuit, Operation,
    circuit::{ComponentGrid, Ket, Measurement, SourceLocation, Unitary},
    json_to_circuit::json_to_circuits,
};

pub fn circuits_to_qsharp(file_name: &str, circuits_json: &str) -> Result<String, String> {
    json_to_circuits(circuits_json).map(|circuits| build_circuits(file_name, &circuits.circuits))
}

fn build_circuits(file_name: &str, circuits: &[Circuit]) -> String {
    if circuits.len() == 1 {
        build_operation_def(file_name, &circuits[0])
    } else {
        let mut qsharp_str = String::new();
        for (index, circuit) in circuits.iter().enumerate() {
            let circuit_name = format!("{file_name}{index}");
            let circuit_str = build_operation_def(&circuit_name, circuit);
            qsharp_str.push_str(&circuit_str);
        }
        qsharp_str
    }
}

fn build_operation_def(circuit_name: &str, circuit: &Circuit) -> String {
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

    // The trace-derived form of a circuit wraps the entire body in a single
    // outer call to the entry-point operation (e.g. `Main` with the whole
    // body in `children`). Calling that name here would emit a call to a
    // non-existent operation and skip the body entirely, so we unwrap one
    // level when we see that shape. Editor-authored circuits never produce
    // this shape — custom-gate calls don't carry their body as children.
    let body_grid = match unwrap_entry_point_wrapper(&circuit.component_grid) {
        Some(inner) => inner,
        None => &circuit.component_grid,
    };

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
fn process_components(
    grid: &ComponentGrid,
    qubits: &FxHashMap<usize, String>,
    indentation_level: usize,
    measure_results: &mut Vec<(String, (usize, usize))>,
    should_add_pi: &mut bool,
    out: &mut String,
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
                    out.push_str(&generate_unitary_call(unitary, qubits, &indent));
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

/// Detects the trace-derived "entry-point wrapper" shape: a top-level grid
/// containing exactly one column with exactly one unitary that has children
/// AND whose name does *not* identify a structural group (loops,
/// conditionals, scopes — those are real circuit structure that we must
/// preserve, not wrappers to unwrap). The wrapper's name is the entry-point
/// operation that was traced (e.g. `Main`). Emitting it as a call would
/// point at an operation that doesn't exist in the user's preview and would
/// skip the body entirely.
///
/// Returns the inner grid to use as the body, or `None` if the grid is not
/// in entry-point-wrapper shape.
fn unwrap_entry_point_wrapper(grid: &ComponentGrid) -> Option<&ComponentGrid> {
    if grid.len() != 1 {
        return None;
    }
    let col = grid.first()?;
    if col.components.len() != 1 {
        return None;
    }
    let only = col.components.first()?;
    match only {
        Operation::Unitary(u)
            if !u.children.is_empty() && structural_group_kind(&u.gate).is_none() =>
        {
            Some(&u.children)
        }
        _ => None,
    }
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
// Detection runs after `unwrap_entry_point_wrapper` so it walks the same
// grid the emitter prints. Findings are surfaced as a single banner above
// the operation declaration, naming each issue and (when available) its
// source line.
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
            // Recurse into all children — divergences can be nested arbitrarily,
            // and we want to surface every one in the banner.
            walk_for_divergences(op.children(), findings);
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
            DivergenceKind::DivergentLoopIterations => {
                "loop has structurally divergent iterations"
            }
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
) -> String {
    let operation_str = operation_call(unitary, qubits);
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

fn operation_call(unitary: &Unitary, qubits: &FxHashMap<usize, String>) -> String {
    let gate = unitary.gate.as_str();

    let is_controlled = !unitary.controls.is_empty();

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
    args.extend(targets);

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
