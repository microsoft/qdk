// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod ast_context;
mod fields;
mod global_items;
mod locals;
mod openqasm;
mod qsharp;
#[cfg(test)]
mod tests;
pub(crate) mod text_edits;

use crate::{
    compilation::{Compilation, CompilationKind, source_position_to_package_offset},
    protocol::{CompletionItem, CompletionItemKind, CompletionList, TextEdit},
};
use ast_context::AstContext;
use fields::Fields;
use global_items::Globals;
use locals::Locals;
use log::{Level::Trace, log_enabled, trace};
use qsc::{
    line_column::{Encoding, Position},
    parse::completion::PathKind,
};

use rustc_hash::FxHashSet;
use text_edits::TextEditRange;

type SortPriority = u32;

pub(crate) fn get_completions(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> CompletionList {
    let unit = &compilation.user_unit();
    let package_offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);

    let source = unit
        .sources
        .find_by_offset(package_offset)
        .expect("source should exist");
    let source_offset: u32 = package_offset - source.offset;

    // The parser uses the relative source name to figure out the implicit namespace.
    let source_name_relative = unit.sources.relative_name(&source.name);

    if log_enabled!(Trace) {
        let last_char = if source_offset > 0 {
            source.contents[(package_offset as usize - 1)..]
                .chars()
                .next()
        } else {
            None
        };
        trace!("the character before the cursor is: {last_char:?}");
    }

    match compilation.kind {
        CompilationKind::OpenProject { .. } | CompilationKind::Notebook { .. } => {
            qsharp::completions(
                compilation,
                position_encoding,
                package_offset,
                &source.contents,
                source_offset,
                source_name_relative,
            )
        }
        CompilationKind::OpenQASM { .. } => {
            openqasm::completions(compilation, &source.contents, source_offset)
        }
    }
}

struct Completion {
    item: CompletionItem,
    sort_priority: SortPriority,
}

impl Completion {
    fn new(label: String, kind: CompletionItemKind) -> Self {
        Self::with_detail(label, kind, None)
    }

    fn with_detail(label: String, kind: CompletionItemKind, detail: Option<String>) -> Self {
        Self::with_text_edits(label, kind, detail, None, 0)
    }

    fn with_text_edits(
        label: String,
        kind: CompletionItemKind,
        detail: Option<String>,
        additional_text_edits: Option<Vec<TextEdit>>,
        sort_priority: u32,
    ) -> Completion {
        Completion {
            item: CompletionItem {
                label,
                kind,
                // This will be populated from sort_priority when the list gets built
                sort_text: None,
                detail,
                additional_text_edits,
            },
            sort_priority,
        }
    }
}

/// Collects path segments that are applicable at the current cursor offset.
/// Assumes that the cursor is in the middle of a path, either immediately
/// following a `.` or in the middle of a path segment that follows a `.` .
///
/// Narrows down the list based on the qualifier before the `.` as well
/// as the name kind expected at that syntax node. For example,
/// `let x : Microsoft.Quantum.Math.↘`  should include `Complex` (a type) while
/// `let x = Microsoft.Quantum.Math.↘` should include `PI` (a callable).
fn collect_path_segments(
    ast_context: &AstContext,
    globals: &Globals,
    fields: &Fields,
) -> Vec<Vec<Completion>> {
    let Some((path_kind, qualifier)) = ast_context.path_segment_context() else {
        return Vec::new();
    };

    match path_kind {
        PathKind::Namespace => globals.namespaces_in(&qualifier),
        PathKind::Expr => {
            // First try treating the path as a field access, then
            // as a global.
            let fields = fields.fields();
            if fields.is_empty() {
                globals.expr_names_in(&qualifier)
            } else {
                vec![fields]
            }
        }
        PathKind::Ty | PathKind::Struct => globals.type_names_in(&qualifier),
        PathKind::Import => globals.importable_names_in(&qualifier),
    }
}

/// Builds the `CompletionList` from the ordered groups of completion items.
fn into_completion_list(groups: impl Iterator<Item = Vec<Completion>>) -> CompletionList {
    // The HashSet serves to eliminate duplicates.
    let mut items = FxHashSet::default();

    // Build list one group at a time. The sort order
    // is determined by the order in which the groups are pushed.
    // Within each group, items are sorted by sort_priority.
    for (current_sort_group, group) in groups.enumerate() {
        items.extend(group.into_iter().map(
            |Completion {
                 item,
                 sort_priority,
             }| CompletionItem {
                // The sort_text is what the editor will ultimately use to
                // sort the items, so we're using the sort priority as a prefix.
                sort_text: Some(format!(
                    "{:02}{:02}{}",
                    current_sort_group, sort_priority, item.label
                )),
                ..item
            },
        ));
    }

    CompletionList {
        items: items.into_iter().collect(),
    }
}
