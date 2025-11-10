// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{iter::once, sync::Arc};

use qsc::{
    LanguageFeatures,
    line_column::Encoding,
    parse::completion::{
        HardcodedIdentKind, NameKind, PathKind, WordKinds, possible_words_at_offset_in_fragments,
        possible_words_at_offset_in_source,
    },
};
use qsc_project::ProjectType;

use crate::{
    Compilation,
    compilation::CompilationKind,
    completion::{AstContext, Fields, Globals},
    protocol::{CompletionItem, CompletionItemKind, CompletionList},
};

use super::{Completion, Locals, TextEditRange, collect_path_segments, into_completion_list};

pub(super) fn completions(
    compilation: &Compilation,
    position_encoding: Encoding,
    package_offset: u32,
    contents: &Arc<str>,
    source_offset: u32,
    source_name_relative: &str,
) -> CompletionList {
    // Special case: no completions in attribute arguments, even when the
    // parser expects an expression.
    let ast_context = AstContext::init(source_offset, &compilation.user_unit().ast.package);
    if let Some(name) = ast_context.get_name_of_attr_for_attr_arg() {
        if name.as_ref() == "EntryPoint" {
            return CompletionList {
                items: vec![
                    CompletionItem::new("Unrestricted".to_string(), CompletionItemKind::Keyword),
                    CompletionItem::new("Base".to_string(), CompletionItemKind::Keyword),
                    CompletionItem::new("Adaptive_RI".to_string(), CompletionItemKind::Keyword),
                    CompletionItem::new("Adaptive_RIF".to_string(), CompletionItemKind::Keyword),
                ],
            };
        }
        // No completions in attribute expressions, they're misleading.
        return CompletionList::default();
    }

    // What kinds of words are expected at the cursor location?
    let expected_words_at_cursor =
        expected_word_kinds(compilation, source_name_relative, contents, source_offset);

    // Now that we have the information from the parser about what kinds of
    // words are expected, gather the actual words (identifiers, keywords, etc) for each kind.

    // Keywords and other hardcoded words
    let hardcoded_completions = collect_hardcoded_words(expected_words_at_cursor);

    // The tricky bit: globals, locals, names we need to gather from the compilation.
    let name_completions = collect_names(
        expected_words_at_cursor,
        package_offset,
        compilation,
        position_encoding,
    );

    // We have all the data, put everything into a completion list.
    into_completion_list(once(hardcoded_completions).chain(name_completions))
}

/// Invokes the parser to determine what kinds of words are expected at the cursor location.
fn expected_word_kinds(
    compilation: &Compilation,
    source_name_relative: &str,
    source_contents: &str,
    cursor_offset: u32,
) -> WordKinds {
    // We should not return any completions in comments.
    // This compensates for a bug in [`possible_words_at_offset_in_source`] .
    // Ideally, that function would be aware of the comment context and not
    // return any completions, however this is difficult to do today because
    // of the parser's unawareness of comment tokens.
    // So we do a simple check here where we have access to the full source contents.
    if in_comment(source_contents, cursor_offset) {
        return WordKinds::empty();
    }

    match &compilation.kind {
        CompilationKind::OpenProject {
            package_graph_sources,
            ..
        } => possible_words_at_offset_in_source(
            source_contents,
            Some(source_name_relative),
            package_graph_sources.root.language_features,
            cursor_offset,
        ),
        CompilationKind::Notebook { project } => possible_words_at_offset_in_fragments(
            source_contents,
            project.as_ref().map_or(LanguageFeatures::default(), |p| {
                let ProjectType::QSharp(sources) = &p.project_type else {
                    unreachable!("Project type should be Q#")
                };
                sources.root.language_features
            }),
            cursor_offset,
        ),
        CompilationKind::OpenQASM { .. } => {
            unreachable!("OpenQASM compilations shouldn't request Q# completions")
        }
    }
}

fn in_comment(source_contents: &str, cursor_offset: u32) -> bool {
    // find the last newline before the cursor
    let last_line_start = source_contents[..cursor_offset as usize]
        .rfind('\n')
        .unwrap_or(0);
    // find the last comment start before the cursor
    let last_comment_start = source_contents[last_line_start..cursor_offset as usize].rfind("//");
    last_comment_start.is_some()
}

/// Collects hardcoded completions from the given set of parser predictions.
///
/// Hardcoded words are actual keywords (`let`, etc) as well as other words that are
/// hardcoded into the language (`Qubit`, `EntryPoint`, etc)
fn collect_hardcoded_words(expected: WordKinds) -> Vec<Completion> {
    let mut completions = Vec::new();
    for word_kind in expected.iter_hardcoded_ident_kinds() {
        match word_kind {
            HardcodedIdentKind::Qubit => {
                completions.push(Completion::new(
                    "Qubit".to_string(),
                    CompletionItemKind::Interface,
                ));
            }
            HardcodedIdentKind::Attr => {
                completions.extend([
                    Completion::new("EntryPoint".to_string(), CompletionItemKind::Interface),
                    Completion::new("Config".to_string(), CompletionItemKind::Interface),
                    Completion::new(
                        "SimulatableIntrinsic".to_string(),
                        CompletionItemKind::Interface,
                    ),
                    Completion::new("Measurement".to_string(), CompletionItemKind::Interface),
                    Completion::new("Reset".to_string(), CompletionItemKind::Interface),
                    Completion::new("Test".to_string(), CompletionItemKind::Interface),
                ]);
            }
            HardcodedIdentKind::Size => {
                completions.push(Completion::new(
                    "size".to_string(),
                    CompletionItemKind::Keyword,
                ));
            }
        }
    }

    for keyword in expected.iter_keywords() {
        let keyword = keyword.to_string();
        // Skip adding the underscore keyword to the list, it's more confusing than helpful.
        if keyword != "_" {
            completions.push(Completion::new(keyword, CompletionItemKind::Keyword));
        }
    }

    completions
}

/// Collects names from the compilation that match the expected word kinds.
fn collect_names(
    expected: WordKinds,
    cursor_offset: u32,
    compilation: &Compilation,
    position_encoding: Encoding,
) -> Vec<Vec<Completion>> {
    let mut groups = Vec::new();

    for name_kind in expected.iter_name_kinds() {
        match name_kind {
            NameKind::Path(path_kind) => {
                let globals = Globals::init(cursor_offset, compilation);
                let edit_range = TextEditRange::init(cursor_offset, compilation, position_encoding);
                let locals = Locals::new(cursor_offset, compilation);

                groups.extend(collect_paths(path_kind, &globals, &locals, &edit_range));
            }
            NameKind::PathSegment => {
                let globals = Globals::init(cursor_offset, compilation);
                let ast_context =
                    AstContext::init(cursor_offset, &compilation.user_unit().ast.package);
                let fields = Fields::new(compilation, &ast_context);

                groups.extend(collect_path_segments(&ast_context, &globals, &fields));
            }
            NameKind::TyParam => {
                let locals = Locals::new(cursor_offset, compilation);
                groups.push(locals.type_names());
            }
            NameKind::Field => {
                let ast_context =
                    AstContext::init(cursor_offset, &compilation.user_unit().ast.package);
                let fields = Fields::new(compilation, &ast_context);

                groups.push(fields.fields());
            }
            NameKind::PrimitiveClass => {
                // we know the types of the primitive classes, so we can just return them
                // hard coded here.
                // If we ever support user-defined primitive classes, we'll need to change this.

                // this is here to force us to update completions if a new primitive class
                // constraint is supported
                use qsc::hir::ty::ClassConstraint::*;
                match Add {
                    Add
                    | Eq
                    | Exp { .. }
                    | Iterable { .. }
                    | NonNativeClass(_)
                    | Integral
                    | Mod
                    | Sub
                    | Mul
                    | Div
                    | Signed
                    | Ord
                    | Show => (),
                }

                groups.push(vec![
                    Completion::new("Add".to_string(), CompletionItemKind::Class),
                    Completion::new("Eq".to_string(), CompletionItemKind::Class),
                    Completion::with_detail(
                        "Exp".to_string(),
                        CompletionItemKind::Class,
                        Some("Exp['Power]".into()),
                    ),
                    Completion::new("Integral".to_string(), CompletionItemKind::Class),
                    Completion::new("Show".to_string(), CompletionItemKind::Class),
                    Completion::new("Signed".to_string(), CompletionItemKind::Class),
                    Completion::new("Ord".to_string(), CompletionItemKind::Class),
                    Completion::new("Mod".to_string(), CompletionItemKind::Class),
                    Completion::new("Sub".to_string(), CompletionItemKind::Class),
                    Completion::new("Mul".to_string(), CompletionItemKind::Class),
                    Completion::new("Div".to_string(), CompletionItemKind::Class),
                ]);
            }
        }
    }
    groups
}

/// Collects paths that are applicable at the current cursor offset,
/// taking into account all the relevant name resolution context,
/// such as scopes and visibility at the cursor location.
///
/// Note that the list will not contain full paths to items. It will typically
/// only include leading qualifier, or the item name along with an auto-import edit.
/// For example, the item `Microsoft.Quantum.Diagnostics.DumpMachine` will contribute
/// two completion items: the leading qualifier (namespace) `Microsoft` and the
/// callable name `DumpMachine` with an auto-import edit to add `Microsoft.Quantum.Diagnostics`.
fn collect_paths(
    expected: PathKind,
    globals: &Globals,
    locals_at_cursor: &Locals,
    text_edit_range: &TextEditRange,
) -> Vec<Vec<Completion>> {
    let mut global_names = Vec::new();
    let mut locals_and_builtins = Vec::new();
    match expected {
        PathKind::Expr => {
            locals_and_builtins.push(locals_at_cursor.expr_names());
            global_names.extend(globals.expr_names(text_edit_range));
        }
        PathKind::Ty => {
            locals_and_builtins.push(locals_at_cursor.type_names());
            locals_and_builtins.push(
                [
                    "Qubit", "Int", "Unit", "Result", "Bool", "BigInt", "Double", "Pauli", "Range",
                    "String",
                ]
                .map(|s| Completion::new(s.to_string(), CompletionItemKind::Interface))
                .into(),
            );

            global_names.extend(globals.type_names(text_edit_range));
        }
        PathKind::Import => {
            global_names.extend(globals.importable_names());
        }
        PathKind::Struct => {
            global_names.extend(globals.type_names(text_edit_range));
        }
        PathKind::Namespace => {
            global_names.push(globals.namespaces());
        }
    }

    // This order ensures that locals and builtins come before globals
    // in the eventual completion list
    locals_and_builtins.extend(global_names);
    locals_and_builtins
}
