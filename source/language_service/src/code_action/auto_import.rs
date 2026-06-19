// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Auto-import code action logic: for unresolved names, offers quick fixes that insert an
// `import {namespace}.{name};` statement at the start of the enclosing namespace.

#[cfg(test)]
mod tests;

use qsc::{Span, compile::ErrorKind, line_column::Encoding, resolve::NameKind};
use rustc_hash::FxHashSet;

use super::is_error_relevant;
use crate::{
    compilation::Compilation,
    completion::text_edits::TextEditRange,
    protocol::{CodeAction, CodeActionKind, TextEdit, WorkspaceEdit},
};

/// Produces auto-import quick fixes for unresolved names within `span`.
///
/// For each unresolved-name diagnostic that overlaps the requested range, the unresolved
/// (unqualified) name is looked up in the global term and type tables. Every namespace that
/// exports a matching name yields a separate `QuickFix` code action that inserts an
/// `import {namespace}.{name};` statement at the start of the enclosing namespace.
pub(super) fn auto_import_fixes(
    compilation: &Compilation,
    source_name: &str,
    span: Span,
    encoding: Encoding,
) -> Vec<CodeAction> {
    let mut code_actions = Vec::new();
    // Dedupe by title, since the same name may be unresolved at multiple offsets in range.
    let mut seen = FxHashSet::default();

    let unresolved_names = compilation
        .compile_errors
        .iter()
        .filter(|error| is_error_relevant(error, span))
        .filter_map(|error| match error.error() {
            ErrorKind::Frontend(frontend_error) => frontend_error.unresolved_name(),
            _ => None,
        });

    for (name, name_span) in unresolved_names {
        // v1 only handles unqualified names; partial paths are deferred.
        if name.is_empty() || name.contains('.') {
            continue;
        }

        // Determine where an import would be inserted for the enclosing namespace.
        let edit_range = TextEditRange::init(name_span.lo, compilation, encoding);
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
