// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::iter::once;
use std::str::FromStr;

use qsc::openqasm::compiler::{
    PragmaKind, SUPPORTED_QDK_ANNOTATIONS, annotation_configures_profile, valid_box_pragma_targets,
};
use qsc::openqasm::parser::ast::PathKind;
use qsc::openqasm::semantic::ast::{Annotation, Pragma};
use qsc::openqasm::semantic::symbols::SymbolTable;
use qsc::openqasm::span::Span;
use qsc::target::Profile;

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
    // Pragmas and annotations are lexed as single whole-line tokens, so the generic word-kind
    // collector cannot distinguish the name position from the value position within them.
    // When the cursor is inside such a line, re-parse the source and use the resulting AST
    // to offer precisely scoped completions. Pragma names are only offered inside a pragma, and
    // annotation names are only offered after the `@` that begins an annotation.
    //
    // We should update the lexer to produce separate tokens for the directive keyword,
    // name, and value, so that the generic word-kind collector can handle them without
    // needing to re-parse the source in a follow-up.
    if let Some(line_kind) = classify_pragma_or_annotation_line(source_contents, cursor_offset) {
        let scoped = pragma_or_annotation_completions(source_contents, cursor_offset, line_kind);
        return into_completion_list(once(scoped));
    }

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

/// The kind of directive line the cursor is on, used to scope completions to
/// pragma names / annotation names / profile values.
#[derive(Clone, Copy)]
enum DirectiveLine {
    Pragma,
    Annotation,
}

/// Classifies the line the cursor is on as a pragma line, an annotation line,
/// or neither. Pragma names are only relevant inside a `#pragma`/`pragma` line,
/// and annotation names only after the `@` that begins an annotation, so this
/// keeps those completions out of ordinary statement positions.
fn classify_pragma_or_annotation_line(
    source_contents: &str,
    cursor_offset: u32,
) -> Option<DirectiveLine> {
    let offset = (cursor_offset as usize).min(source_contents.len());
    let line_start = source_contents[..offset]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line = source_contents[line_start..].trim_start();

    if line.starts_with('@') {
        return Some(DirectiveLine::Annotation);
    }

    // Require a word boundary after the keyword so identifiers such as
    // `pragmatic` are not mistaken for a pragma directive.
    for keyword in ["#pragma", "pragma"] {
        if let Some(rest) = line.strip_prefix(keyword)
            && (rest.is_empty() || rest.starts_with(char::is_whitespace))
        {
            return Some(DirectiveLine::Pragma);
        }
    }

    None
}

/// Returns the completions scoped to the cursor's position inside a `#pragma`
/// or `@annotation` line. Re-parses the source so the pragma/annotation AST
/// node spans can be used to tell the name position from the value position.
/// Falls back to offering names when no node is available yet.
fn pragma_or_annotation_completions(
    source_contents: &str,
    cursor_offset: u32,
    line_kind: DirectiveLine,
) -> Vec<Completion> {
    let res = qsc::openqasm::semantic::parse(source_contents, "<completions>");
    let program = &res.program;

    match line_kind {
        DirectiveLine::Pragma => {
            for pragma in &program.pragmas {
                if span_contains(pragma.span, cursor_offset) {
                    return pragma_completions(pragma, cursor_offset, &res.symbols);
                }
            }
            pragma_name_completions()
        }
        DirectiveLine::Annotation => {
            for stmt in &program.statements {
                for annotation in &stmt.annotations {
                    if span_contains(annotation.span, cursor_offset) {
                        return annotation_completions(annotation, cursor_offset);
                    }
                }
            }
            annotation_name_completions()
        }
    }
}

fn span_contains(span: Span, offset: u32) -> bool {
    span.lo <= offset && offset <= span.hi
}

/// Completions for a position inside a `#pragma` line: the supported pragma
/// names while the cursor is on the name, or value completions once the cursor
/// is in the value position. The value completions are target profiles for a
/// `qdk.qir.profile` pragma, or valid target function names for a
/// `qdk.box.open`/`qdk.box.close` pragma.
fn pragma_completions(
    pragma: &Pragma,
    cursor_offset: u32,
    symbols: &SymbolTable,
) -> Vec<Completion> {
    let in_name_position = match pragma.identifier.as_ref().and_then(PathKind::span) {
        Some(span) => cursor_offset <= span.hi,
        None => true,
    };

    if in_name_position {
        return pragma_name_completions();
    }

    let name = pragma
        .identifier
        .as_ref()
        .map(PathKind::as_string)
        .unwrap_or_default();
    match PragmaKind::from_str(&name) {
        Ok(PragmaKind::QdkQirProfile) => profile_completions(),
        Ok(PragmaKind::QdkBoxOpen | PragmaKind::QdkBoxClose) => box_target_completions(symbols),
        _ => Vec::new(),
    }
}

/// Completions for a position inside an `@annotation` line: the supported QDK
/// annotation names while the cursor is on the name, or target profile values
/// once the cursor is in the value position of a profile annotation.
fn annotation_completions(annotation: &Annotation, cursor_offset: u32) -> Vec<Completion> {
    let in_name_position = match annotation.identifier.span() {
        Some(span) => cursor_offset <= span.hi,
        None => true,
    };

    if in_name_position {
        return annotation_name_completions();
    }

    if annotation_configures_profile(&annotation.identifier.as_string()) {
        profile_completions()
    } else {
        Vec::new()
    }
}

fn pragma_name_completions() -> Vec<Completion> {
    PragmaKind::all()
        .into_iter()
        .map(|kind| Completion::new(kind.as_str().to_string(), CompletionItemKind::Keyword))
        .collect()
}

fn annotation_name_completions() -> Vec<Completion> {
    SUPPORTED_QDK_ANNOTATIONS
        .into_iter()
        .map(|name| Completion::new(name.to_string(), CompletionItemKind::Interface))
        .collect()
}

fn profile_completions() -> Vec<Completion> {
    Profile::all()
        .into_iter()
        .map(|profile| Completion::new(profile.to_str().to_string(), CompletionItemKind::Keyword))
        .collect()
}

/// Completions for the value of a `qdk.box.open`/`qdk.box.close` pragma: the
/// names of functions that are valid box targets (parameterless and returning
/// void), as defined by the compiler.
fn box_target_completions(symbols: &SymbolTable) -> Vec<Completion> {
    valid_box_pragma_targets(symbols)
        .into_iter()
        .map(|name| Completion::new(name, CompletionItemKind::Function))
        .collect()
}

#[allow(clippy::items_after_statements)]
fn collect_hardcoded_words(
    expected: qsc::openqasm::completion::word_kinds::WordKinds,
) -> Vec<Completion> {
    let mut completions = Vec::new();
    for word_kind in expected.iter_hardcoded_ident_kinds() {
        match word_kind {
            qsc::openqasm::completion::word_kinds::HardcodedIdentKind::Annotation => {
                // Annotation names are offered only after the `@` that begins an
                // annotation (handled by the directive-line path), not at every
                // statement-start position where an annotation would be valid.
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
