// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod wrapper_refactor;

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc::{
    Span,
    compile::ErrorKind,
    error::WithSource,
    line_column::{Encoding, Range},
    resolve::NameKind,
};
use rustc_hash::FxHashSet;

use crate::{
    compilation::Compilation,
    completion::text_edits::TextEditRange,
    protocol::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit},
};

/// Diagnostic code emitted by the resolver for an unresolved name.
const RESOLVE_NOT_FOUND_CODE: &str = "Qsc.Resolve.NotFound";

pub(crate) fn get_code_actions(
    compilation: &Compilation,
    source_name: &str,
    range: Range,
    position_encoding: Encoding,
) -> Vec<CodeAction> {
    // Compute quick fixes (lint-based) and refactor actions and merge.
    let span = compilation.source_range_to_package_span(source_name, range, position_encoding);
    let mut actions = quick_fixes(compilation, source_name, span, position_encoding);
    // Add auto-import quick fixes for unresolved names (e.g. `DumpMachine` -> `import Std.Diagnostics.DumpMachine;`).
    actions.extend(auto_import_fixes(
        compilation,
        source_name,
        span,
        position_encoding,
    ));
    // Add operation refactor actions (wrapper generation, etc.). Additional refactor providers
    // should be added here, each returning their own Vec<CodeAction>.
    actions.extend(wrapper_refactor::operation_refactors(
        compilation,
        source_name,
        span,
        position_encoding,
    ));
    actions
}

/// Produces auto-import quick fixes for unresolved names within `span`.
///
/// For each `Qsc.Resolve.NotFound` diagnostic that overlaps the requested range, the
/// unresolved (unqualified) name is looked up in the global term and type tables. Every
/// namespace that exports a matching name yields a separate `QuickFix` code action that
/// inserts an `import {namespace}.{name};` statement at the start of the enclosing namespace.
fn auto_import_fixes(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let source = compilation
        .user_unit()
        .sources
        .find_by_name(source_name)
        .expect("source should exist");

    let mut code_actions = Vec::new();
    // Dedupe by title, since the same name may be unresolved at multiple offsets in range.
    let mut seen = FxHashSet::default();

    let not_found_errors = compilation
        .compile_errors
        .iter()
        .filter(|error| is_error_relevant(error, span))
        .filter(|error| {
            error
                .code()
                .is_some_and(|code| code.to_string() == RESOLVE_NOT_FOUND_CODE)
        });

    for error in not_found_errors {
        let Some(error_span) = resolve_span(error) else {
            continue;
        };

        // Extract the unresolved name from the source text at the error's span.
        let lo = (error_span.lo - source.offset) as usize;
        let hi = (error_span.hi - source.offset) as usize;
        let Some(name) = source.contents.get(lo..hi) else {
            continue;
        };

        // v1 only handles unqualified names; partial paths are deferred.
        if name.is_empty() || name.contains('.') {
            continue;
        }

        // Determine where an import would be inserted for the enclosing namespace.
        let edit_range = TextEditRange::init(error_span.lo, compilation, encoding);
        let Some(insert_at) = edit_range.insert_import_at else {
            continue;
        };

        for namespace_name in matching_namespaces(compilation, name) {
            let title = format!("Import {namespace_name}.{name}");
            if !seen.insert(title.clone()) {
                continue;
            }

            let new_text = format!("import {namespace_name}.{name};{}", edit_range.indent);
            code_actions.push(CodeAction {
                title,
                edit: Some(WorkspaceEdit {
                    changes: vec![(
                        source_name.to_string(),
                        vec![TextEdit {
                            new_text,
                            range: insert_at,
                        }],
                    )],
                }),
                kind: Some(CodeActionKind::QuickFix),
                is_preferred: None,
            });
        }
    }

    code_actions
}

/// Returns the fully-qualified names of all namespaces that export an item named `name`
/// in either an expression (term) or type context. Results are sorted for determinism.
fn matching_namespaces(compilation: &Compilation, name: &str) -> Vec<String> {
    let global_scope = &compilation.user_unit().ast.globals;
    let mut namespaces = FxHashSet::default();

    for name_kind in [NameKind::Term, NameKind::Ty] {
        for (namespace_id, names) in global_scope.table(name_kind).iter() {
            if names.contains_key(name) {
                let namespace_name = global_scope.format_namespace_name(namespace_id);
                // Don't suggest auto-imports for OpenQASM namespaces (mirrors completions).
                if !namespace_name.starts_with("Std.OpenQASM") {
                    namespaces.insert(namespace_name);
                }
            }
        }
    }

    let mut namespaces: Vec<String> = namespaces.into_iter().collect();
    namespaces.sort();
    namespaces
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
        if let ErrorKind::Lint(lint) = diagnostic.error()
            && let Some(code_action) = &lint.code_action
            && !code_action.edits.is_empty()
        {
            let source = compilation
                .user_unit()
                .sources
                .find_by_name(source_name)
                .expect("source should exist");
            let text_edits: Vec<TextEdit> = code_action
                .edits
                .iter()
                .map(|(new_text, span)| TextEdit {
                    new_text: new_text.clone(),
                    range: qsc::line_column::Range::from_span(encoding, &source.contents, span),
                })
                .collect();
            let title = code_action.title.clone();
            code_actions.push(CodeAction {
                title,
                edit: Some(WorkspaceEdit {
                    changes: vec![(source_name.to_string(), text_edits)],
                }),
                kind: Some(CodeActionKind::QuickFix),
                is_preferred: None,
            });
        }
    }

    code_actions
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
