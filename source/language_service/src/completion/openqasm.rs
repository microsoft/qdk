// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::iter::once;
use std::str::FromStr;

use qsc::openqasm::compiler::{
    PragmaKind, SUPPORTED_QDK_ANNOTATIONS, annotation_configures_profile, valid_box_pragma_targets,
};
use qsc::openqasm::completion::word_kinds::NameKind;
use qsc::openqasm::completion::{CompletionContext, CompletionDirective};
use qsc::openqasm::semantic::symbols::SymbolTable;
use qsc::target::Profile;

use crate::{
    Compilation,
    completion::{AstContext, Fields, Globals, collect_path_segments},
    protocol::{CompletionItemKind, CompletionList},
};

use super::{Completion, Locals, into_completion_list};

/// Gives parser-classified directive positions exclusive candidate sets so
/// ordinary OpenQASM completions cannot leak into directive names or values.
pub(super) fn completions(
    compilation: &Compilation,
    source_contents: &str,
    cursor_offset: u32,
) -> CompletionList {
    let parser_completion =
        qsc::openqasm::completion::completion_at_offset_in_source(source_contents, cursor_offset);

    let directive_completions =
        match parser_completion.context {
            Some(CompletionContext::AnnotationName) => Some(annotation_name_completions()),
            Some(CompletionContext::PragmaName) => Some(pragma_name_completions()),
            Some(CompletionContext::DirectiveValue) => Some(directive_value_completions(
                source_contents,
                parser_completion.directive.as_ref(),
            )),
            None => incomplete_directive_context(source_contents, cursor_offset).map(|context| {
                match context {
                    CompletionContext::AnnotationName => annotation_name_completions(),
                    CompletionContext::PragmaName => pragma_name_completions(),
                    CompletionContext::DirectiveValue => Vec::new(),
                }
            }),
        };
    if let Some(scoped) = directive_completions {
        return into_completion_list(once(scoped));
    }

    let expected_words_at_cursor = parser_completion.words;

    // Now that we have the information from the parser about what kinds of
    // words are expected, gather the actual words (identifiers, keywords, etc) for each kind.

    // Keywords and other hardcoded words
    let hardcoded_completions = collect_hardcoded_words(expected_words_at_cursor);

    // The tricky bit: locals, names we need to gather from the compilation.
    let name_completions = collect_names_qasm(expected_words_at_cursor, cursor_offset, compilation);

    // We have all the data, put everything into a completion list.
    into_completion_list(once(hardcoded_completions).chain(name_completions))
}

/// Preserves completion for editor prefixes too incomplete for parser context.
/// The narrow boundary keeps the parser authoritative for all other input.
fn incomplete_directive_context(
    source_contents: &str,
    cursor_offset: u32,
) -> Option<qsc::openqasm::completion::CompletionContext> {
    let offset = (cursor_offset as usize).min(source_contents.len());
    let line_start = source_contents[..offset]
        .rfind(['\r', '\n'])
        .map_or(0, |index| index + 1);
    let prefix = source_contents[line_start..offset].trim_start();

    if prefix == "@" {
        return Some(qsc::openqasm::completion::CompletionContext::AnnotationName);
    }

    if !prefix.is_empty() && prefix.starts_with('#') && "#pragma".starts_with(prefix) {
        return Some(qsc::openqasm::completion::CompletionContext::PragmaName);
    }

    None
}

/// Scopes value candidates to the enclosing directive so opaque or unsupported
/// payloads do not receive unrelated suggestions.
fn directive_value_completions(
    source_contents: &str,
    directive: Option<&CompletionDirective>,
) -> Vec<Completion> {
    match directive {
        Some(CompletionDirective::Annotation(name)) if annotation_configures_profile(name) => {
            profile_completions()
        }
        Some(CompletionDirective::Pragma(name)) => match PragmaKind::from_str(name) {
            Ok(PragmaKind::QdkQirProfile) => profile_completions(),
            Ok(PragmaKind::QdkBoxOpen | PragmaKind::QdkBoxClose) => {
                box_target_completions_from_source(source_contents)
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Reparses semantically because valid box targets depend on resolved callable
/// signatures rather than parser word kinds.
fn box_target_completions_from_source(source_contents: &str) -> Vec<Completion> {
    let result = qsc::openqasm::semantic::parse(source_contents, "<completions>");
    box_target_completions(&result.symbols)
}

/// Derives pragma names from the compiler's supported set to keep completion
/// aligned with directive handling.
fn pragma_name_completions() -> Vec<Completion> {
    PragmaKind::all()
        .into_iter()
        .map(|kind| Completion::new(kind.as_str().to_string(), CompletionItemKind::Keyword))
        .collect()
}

/// Limits annotation suggestions to names supported by the QDK compiler.
fn annotation_name_completions() -> Vec<Completion> {
    SUPPORTED_QDK_ANNOTATIONS
        .into_iter()
        .map(|name| Completion::new(name.to_string(), CompletionItemKind::Interface))
        .collect()
}

/// Limits profile values to target profiles understood by the compiler.
fn profile_completions() -> Vec<Completion> {
    Profile::all()
        .into_iter()
        .map(|profile| Completion::new(profile.to_str().to_string(), CompletionItemKind::Keyword))
        .collect()
}

/// Uses the compiler's semantic predicate so box completion and validation
/// agree on which callable signatures are valid targets.
fn box_target_completions(symbols: &SymbolTable) -> Vec<Completion> {
    valid_box_pragma_targets(symbols)
        .into_iter()
        .map(|name| Completion::new(name, CompletionItemKind::Function))
        .collect()
}

/// Converts parser-owned hardcoded expectations while withholding annotation
/// names from generic statement starts, where they would be too broad.
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

/// Bridges parser expression-path expectations to scope-aware local and
/// builtin candidates from the compilation.
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

/// Routes parser name categories into the corresponding compilation scopes so
/// symbol lookup does not broaden the parser's expected domains.
fn collect_names_qasm(
    expected: qsc::openqasm::completion::word_kinds::WordKinds,
    cursor_offset: u32,
    compilation: &Compilation,
) -> Vec<Vec<Completion>> {
    let mut groups = Vec::new();
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
