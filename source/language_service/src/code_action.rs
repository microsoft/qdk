// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc::{
    Span,
    compile::ErrorKind,
    error::WithSource,
    line_column::{Encoding, Range},
};

use crate::{
    compilation::Compilation,
    protocol::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit},
};
use qsc::hir::{
    CallableKind, ItemKind, PatKind,
    ty::{Prim, Ty},
};

pub(crate) fn get_code_actions(
    compilation: &Compilation,
    source_name: &str,
    range: Range,
    position_encoding: Encoding,
) -> Vec<CodeAction> {
    // Compute quick fixes (lint-based) and refactor actions and merge.
    let span = compilation.source_range_to_package_span(source_name, range, position_encoding);
    let mut actions = quick_fixes(compilation, source_name, span, position_encoding);
    actions.extend(operation_refactors(
        compilation,
        source_name,
        span,
        position_encoding,
    ));
    actions
}

fn quick_fixes(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let mut code_actions = Vec::new();

    // get relevant diagnostics
    let diagnostics = compilation
        .compile_errors
        .iter()
        .filter(|error| is_error_relevant(error, span));

    // For all diagnostics that are lints, we extract the code action edits from them.
    for diagnostic in diagnostics {
        if let ErrorKind::Lint(lint) = diagnostic.error() {
            if !lint.code_action_edits.is_empty() {
                let source = compilation
                    .user_unit()
                    .sources
                    .find_by_name(source_name)
                    .expect("source should exist");
                let text_edits: Vec<TextEdit> = lint
                    .code_action_edits
                    .iter()
                    .map(|(new_text, span)| TextEdit {
                        new_text: new_text.clone(),
                        range: qsc::line_column::Range::from_span(encoding, &source.contents, span),
                    })
                    .collect();
                code_actions.push(CodeAction {
                    title: diagnostic.to_string(),
                    edit: Some(WorkspaceEdit {
                        changes: vec![(source_name.to_string(), text_edits)],
                    }),
                    kind: Some(CodeActionKind::QuickFix),
                    is_preferred: None,
                });
            }
        }
    }

    code_actions
}

// Refactor code actions (non-lint) for operations.
// Currently adds a placeholder refactor for any operation with a non-empty parameter list
// that intersects the requested span. The edit is left empty per initial feature request.
fn operation_refactors(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let mut code_actions = Vec::new();
    let user_unit = compilation.user_unit();
    let package = &user_unit.package;
    let source_map = &user_unit.sources;
    let source = source_map
        .find_by_name(source_name)
        .expect("source should exist");
    let source_span = compilation.package_span_of_source(source_name);

    for (_, item) in package.items.iter() {
        if !source_span.contains(item.span.lo) || span.intersection(&item.span).is_none() {
            continue;
        }
        if let ItemKind::Callable(decl) = &item.kind {
            if decl.kind != CallableKind::Operation || decl.input.ty == Ty::UNIT {
                continue; // only operations with non-empty params
            }

            // Determine indentation using source-local offset (package offset minus source base).
            let local_lo = item.span.lo - source.offset;
            let indent = line_indentation(&source.contents, local_lo);
            let body_indent = if indent.contains('\t') {
                format!("{indent}\t")
            } else {
                format!("{indent}    ")
            };

            let original_name = decl.name.name.as_ref();
            let wrapper_name = generate_unique_wrapper_name(package, original_name);

            let (decl_lines, call_args) = build_param_decls_and_call_args(&decl.input);

            let call_args_joined = if call_args.is_empty() {
                String::new()
            } else {
                call_args.join(", ")
            };

            let return_ty = decl.output.display();
            let return_is_unit = decl.output == Ty::UNIT;

            let call_line = if return_is_unit {
                format!("{body_indent}{original_name}({call_args_joined});")
            } else {
                format!("{body_indent}return {original_name}({call_args_joined});")
            };

            let mut body_lines = Vec::new();
            if !decl_lines.is_empty() {
                body_lines.push(format!(
                    "{body_indent}// TODO: Fill out the values for the parameters"
                ));
                body_lines.extend(decl_lines.iter().map(|decl| format!("{body_indent}{decl}")));
                body_lines.push(String::new()); // blank line
            }
            body_lines.push(format!("{body_indent}// Call original operation"));
            body_lines.push(call_line);

            // We intentionally do NOT prefix the first line with `indent` because the insertion point
            // inherits the existing line's leading whitespace. We DO append `{indent}` after the blank line
            // so that the original operation keeps its indentation after the inserted block.
            let wrapper_text = format!(
                "operation {wrapper_name}() : {return_ty} {{\n{}\n{indent}}}\n\n{indent}",
                &body_lines.join("\n")
            );

            // Insert immediately above the original operation: use zero-length span at item.span.lo
            let insert_span = Span {
                lo: local_lo,
                hi: local_lo,
            };
            let edit_range =
                qsc::line_column::Range::from_span(encoding, &source.contents, &insert_span);

            code_actions.push(CodeAction {
                title: format!("Generate wrapper for {original_name}"),
                edit: Some(WorkspaceEdit {
                    changes: vec![(
                        source_name.to_string(),
                        vec![TextEdit {
                            new_text: wrapper_text,
                            range: edit_range,
                        }],
                    )],
                }),
                kind: Some(CodeActionKind::Refactor),
                is_preferred: None,
            });
        }
    }
    code_actions
}

// Generate a wrapper name that does not clash with existing items in the same package (simple heuristic).
fn generate_unique_wrapper_name(package: &qsc::hir::Package, base: &str) -> String {
    let mut candidate = format!("{base}_Wrapper");
    let mut counter = 2;
    while package.items.iter().any(|(_, item)| match &item.kind {
        ItemKind::Callable(decl) => decl.name.name.as_ref() == candidate,
        _ => false,
    }) {
        candidate = format!("{base}_Wrapper{counter}");
        counter += 1;
    }
    candidate
}

// Build declarations and call arguments preserving tuple structure.
// Returns (declaration lines, call argument expressions list at top-level)
fn build_param_decls_and_call_args(pat: &qsc::hir::Pat) -> (Vec<String>, Vec<String>) {
    let mut decls = Vec::new();
    let call_args = match &pat.kind {
        PatKind::Tuple(items) => {
            let mut args = Vec::new();
            for item in items {
                args.push(build_pattern_expr(item, &mut decls));
            }
            args
        }
        _ => vec![build_pattern_expr(pat, &mut decls)],
    };
    (decls, call_args)
}

// Recursively build an expression for a pattern, pushing any needed declarations (let/use) into decls.
// parent_name is used when synthesizing component variable names for unnamed tuple components within a single bound identifier.
fn build_pattern_expr(pat: &qsc::hir::Pat, decls: &mut Vec<String>) -> String {
    match &pat.kind {
        PatKind::Err | PatKind::Discard => "_".to_string(),
        PatKind::Tuple(items) => {
            // Nested tuple pattern: build each component expression.
            let parts: Vec<String> = items.iter().map(|p| build_pattern_expr(p, decls)).collect();
            format!("({})", parts.join(", "))
        }
        PatKind::Bind(ident) => build_binding_expr(ident.name.as_ref(), &pat.ty, decls),
    }
}

fn build_binding_expr(name: &str, ty: &Ty, decls: &mut Vec<String>) -> String {
    match ty {
        Ty::Prim(Prim::Qubit) => {
            decls.push(format!("use {name} = Qubit();"));
            name.to_string()
        }
        Ty::Array(inner) if matches!(**inner, Ty::Prim(Prim::Qubit)) => {
            decls.push(format!("use {name} = Qubit[1];"));
            name.to_string()
        }
        Ty::Tuple(items) => {
            // Single binding to a tuple type: synthesize tuple literal with defaults, allocating qubits as needed.
            let mut qubit_counter = 0u32;
            let mut qubit_reg_counter = 0u32;
            let tuple_expr = build_tuple_literal(
                name,
                items,
                decls,
                &mut qubit_counter,
                &mut qubit_reg_counter,
            );
            decls.push(format!("let {name} = {tuple_expr};"));
            name.to_string()
        }
        _ => {
            let (default_expr, comment) = default_value_for_type(ty);
            if let Some(expr) = default_expr {
                decls.push(format!("let {name} = {expr};"));
                name.to_string()
            } else {
                decls.push(format!("// TODO: provide value for {name} ({comment})"));
                "_".to_string()
            }
        }
    }
}

// Build a tuple literal expression for a list of types, adding declarations for qubits / complex components.
fn build_tuple_literal(
    base: &str,
    items: &[Ty],
    decls: &mut Vec<String>,
    qubit_counter: &mut u32,
    qubit_reg_counter: &mut u32,
) -> String {
    if items.is_empty() {
        return "()".to_string();
    }
    let mut parts = Vec::new();
    for ty in items {
        match ty {
            Ty::Prim(Prim::Qubit) => {
                let v = format!("{base}_q{qubit_counter}");
                *qubit_counter += 1;
                decls.push(format!("use {v} = Qubit();"));
                parts.push(v);
            }
            Ty::Array(inner) if matches!(**inner, Ty::Prim(Prim::Qubit)) => {
                let v = format!("{base}_qs{qubit_reg_counter}");
                *qubit_reg_counter += 1;
                decls.push(format!("use {v} = Qubit[1];"));
                parts.push(v);
            }
            Ty::Tuple(sub) => {
                // Recurse: nested tuple inside bound tuple; build literal inline.
                let nested =
                    build_tuple_literal(base, sub, decls, qubit_counter, qubit_reg_counter);
                parts.push(nested);
            }
            _ => {
                let (default_expr, comment) = default_value_for_type(ty);
                if let Some(expr) = default_expr {
                    parts.push(expr);
                } else {
                    // Can't synthesize; emit comment and use underscore placeholder.
                    decls.push(format!(
                        "// TODO: provide value for tuple component of {base} ({comment})"
                    ));
                    parts.push("_".to_string());
                }
            }
        }
    }
    if parts.len() == 1 {
        // Q# requires a trailing comma to preserve a single-element tuple; otherwise it is unwrapped.
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

fn default_value_for_type(ty: &Ty) -> (Option<String>, String) {
    match ty {
        Ty::Prim(p) => match p {
            Prim::Int => (Some("0".to_string()), "Int".to_string()),
            Prim::Bool => (Some("false".to_string()), "Bool".to_string()),
            Prim::Double => (Some("0.0".to_string()), "Double".to_string()),
            Prim::Result => (Some("Zero".to_string()), "Result".to_string()),
            Prim::Pauli => (Some("PauliI".to_string()), "Pauli".to_string()),
            Prim::BigInt => (Some("0L".to_string()), "BigInt".to_string()),
            Prim::String => (Some("\"\"".to_string()), "String".to_string()),
            Prim::Qubit => (None, "Qubit - allocate with 'use'".to_string()),
            Prim::Range | Prim::RangeTo | Prim::RangeFrom | Prim::RangeFull => {
                (Some("0..1".to_string()), "Range".to_string())
            }
        },
        Ty::Array(_) => (Some("[]".to_string()), "Array".to_string()),
        Ty::Tuple(_) => (None, "Tuple".to_string()),
        Ty::Param { name, .. } => (None, format!("Generic parameter {name}")),
        Ty::Udt(name, _) => (None, format!("UDT {name}")),
        Ty::Arrow(_) => (None, "Callable type".to_string()),
        Ty::Infer(_) | Ty::Err => (None, "Unknown".to_string()),
    }
}

fn line_indentation(contents: &str, offset: u32) -> String {
    let offset_usize = offset as usize;
    let line_start = contents[..offset_usize]
        .rfind('\n')
        .map_or(0, |idx| idx + 1);
    contents[line_start..offset_usize]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Returns true if the error has a `Range` and it overlaps
/// with the code action's range.
fn is_error_relevant(error: &WithSource<ErrorKind>, span: Span) -> bool {
    let Some(error_span) = resolve_span(error) else {
        return false;
    };
    span.intersection(&error_span).is_some()
}

/// Extracts the uri and `Span` from an error.
fn resolve_span(e: &WithSource<ErrorKind>) -> Option<Span> {
    e.labels()
        .into_iter()
        .flatten()
        .map(|labeled_span| {
            let start = u32::try_from(labeled_span.offset()).expect("offset should fit in u32");
            let len = u32::try_from(labeled_span.len()).expect("length should fit in u32");
            qsc::Span {
                lo: start,
                hi: start + len,
            }
        })
        .next()
}
