// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::iter::once;

use crate::{
    Compilation,
    completion::{AstContext, Fields, Globals, collect_path_segments},
    protocol::{CompletionItemKind, CompletionList},
};

use super::{Completion, Locals, into_completion_list};

pub(super) fn completions(
    compilation: &Compilation,
    source_contents: &str,
    cursor_offset: u32,
) -> CompletionList {
    let expected_words_at_cursor = qsc::openqasm::completion::possible_words_at_offset_in_source(
        source_contents,
        cursor_offset,
    );

    // Now that we have the information from the parser about what kinds of
    // words are expected, gather the actual words (identifiers, keywords, etc) for each kind.

    // Keywords and other hardcoded words
    let hardcoded_completions = collect_hardcoded_words(expected_words_at_cursor);

    // The tricky bit: locals, names we need to gather from the compilation.
    let name_completions = collect_names_qasm(expected_words_at_cursor, cursor_offset, compilation);

    // We have all the data, put everything into a completion list.
    into_completion_list(once(hardcoded_completions).chain(name_completions))
}

#[allow(clippy::items_after_statements)]
fn collect_hardcoded_words(
    expected: qsc::openqasm::completion::word_kinds::WordKinds,
) -> Vec<Completion> {
    let mut completions = Vec::new();
    for word_kind in expected.iter_hardcoded_ident_kinds() {
        match word_kind {
            qsc::openqasm::completion::word_kinds::HardcodedIdentKind::Annotation => {
                completions.extend([Completion::new(
                    "SimulatableIntrinsic".to_string(),
                    CompletionItemKind::Interface,
                )]);
            }
        }
    }

    for keyword in expected.iter_keywords() {
        completions.push(Completion::new(
            keyword.to_string(),
            CompletionItemKind::Keyword,
        ));
    }

    completions
}

#[allow(clippy::items_after_statements)]
fn collect_paths(
    expected: qsc::openqasm::completion::word_kinds::PathKind,
    locals_at_cursor: &Locals,
) -> Vec<Vec<Completion>> {
    let mut locals_and_builtins = Vec::new();
    match expected {
        qsc::openqasm::completion::word_kinds::PathKind::Expr => {
            locals_and_builtins.push(locals_at_cursor.expr_names());
        }
    }
    locals_and_builtins
}

#[allow(clippy::items_after_statements)]
fn collect_names_qasm(
    expected: qsc::openqasm::completion::word_kinds::WordKinds,
    cursor_offset: u32,
    compilation: &Compilation,
) -> Vec<Vec<Completion>> {
    let mut groups = Vec::new();
    use qsc::openqasm::completion::word_kinds::NameKind;
    for name_kind in expected.iter_name_kinds() {
        match name_kind {
            NameKind::Path(path_kind) => {
                let locals = Locals::new(cursor_offset, compilation);
                groups.extend(collect_paths(path_kind, &locals));
            }
            NameKind::PathSegment => {
                let globals = Globals::init(cursor_offset, compilation);
                let ast_context =
                    AstContext::init(cursor_offset, &compilation.user_unit().ast.package);
                let fields = Fields::new(compilation, &ast_context);

                groups.extend(collect_path_segments(&ast_context, &globals, &fields));
            }
        }
    }
    groups
}
