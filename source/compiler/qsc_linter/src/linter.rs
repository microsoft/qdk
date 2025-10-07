// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(crate) mod ast;
pub(crate) mod hir;

use self::{ast::run_ast_lints, hir::run_hir_lints};
use crate::{
    GroupConfig,
    lints::{ast::AstLint, hir::HirLint},
};
use miette::{Diagnostic, LabeledSpan};
use qsc_data_structures::span::Span;
use qsc_frontend::compile::{CompileUnit, PackageStore};
use qsc_hir::hir::{Item, ItemId};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// The entry point to the linter. It takes a [`qsc_frontend::compile::CompileUnit`]
/// as input and outputs a [`Vec<Lint>`](Lint).
#[must_use]
pub fn run_lints(
    package_store: &PackageStore,
    compile_unit: &CompileUnit,
    config: Option<&[LintOrGroupConfig]>,
) -> Vec<Lint> {
    let mut lints = run_lints_without_deduplication(package_store, compile_unit, config);
    remove_duplicates(&mut lints);
    lints
}

/// This function is used by our unit tests, to make sure lints aren't duplicated under
/// normal circumstances. The `run_lints` functions deduplicates the lints to take care
/// of a few special cases where the same expression (referring to the same span in the
/// source code) appears referenced in multiple places in the HIR.
pub(crate) fn run_lints_without_deduplication(
    package_store: &PackageStore,
    compile_unit: &CompileUnit,
    config: Option<&[LintOrGroupConfig]>,
) -> Vec<Lint> {
    let compilation = Compilation {
        package_store,
        compile_unit,
    };

    let unfolded_config = config.map(unfold_groups);

    let mut ast_lints = run_ast_lints(
        &compile_unit.ast.package,
        unfolded_config.as_deref(),
        compilation,
    );
    let mut hir_lints = run_hir_lints(
        &compile_unit.package,
        unfolded_config.as_deref(),
        compilation,
    );

    let mut lints = Vec::new();
    lints.append(&mut ast_lints);
    lints.append(&mut hir_lints);
    lints
}

/// Unfolds groups into lists of lints. Specific lints override group configs.
pub(crate) fn unfold_groups(config: &[LintOrGroupConfig]) -> Vec<LintConfig> {
    let mut config_map: FxHashMap<LintKind, LintLevel> = FxHashMap::default();

    // Unfold groups in the order they appear.
    for elt in config {
        if let LintOrGroupConfig::Group(group) = elt {
            for lint in group.lint_group.unfold() {
                config_map.insert(lint, group.level);
            }
        }
    }

    // Specific lints override group configs.
    for elt in config {
        if let LintOrGroupConfig::Lint(lint) = elt {
            config_map.insert(lint.kind, lint.level);
        }
    }

    config_map
        .iter()
        .map(|(kind, level)| LintConfig {
            kind: *kind,
            level: *level,
        })
        .collect()
}

pub(crate) fn remove_duplicates<T: Eq + std::hash::Hash + Clone>(vec: &mut Vec<T>) {
    let mut seen = rustc_hash::FxHashSet::default();
    vec.retain(|x| seen.insert(x.clone()));
}

#[derive(Clone, Copy)]
pub(crate) struct Compilation<'a> {
    pub package_store: &'a PackageStore,
    pub compile_unit: &'a CompileUnit,
}

impl Compilation<'_> {
    /// Resolves an item id to an item.
    pub fn resolve_item_id(&self, item_id: &ItemId) -> &Item {
        let package = match item_id.package {
            Some(package_id) => {
                &self
                    .package_store
                    .get(package_id)
                    .expect("package should exist in store")
                    .package
            }
            None => &self.compile_unit.package,
        };
        package
            .items
            .get(item_id.item)
            .expect("item id should exist")
    }

    /// Returns a substring of the user code's `SourceMap` in the range `lo..hi`.
    pub fn get_source_code(&self, span: Span) -> String {
        let source = self
            .compile_unit
            .sources
            .find_by_offset(span.lo)
            .expect("source should exist");

        let lo = (span.lo - source.offset) as usize;
        let hi = (span.hi - source.offset) as usize;
        source.contents[lo..hi].to_string()
    }

    /// Returns the indentation at the given offset.
    pub fn indentation_at_offset(&self, offset: u32) -> u32 {
        let source = self
            .compile_unit
            .sources
            .find_by_offset(offset)
            .expect("source should exist");

        let mut indentation = 0;
        for c in source.contents[..(offset - source.offset) as usize]
            .chars()
            .rev()
        {
            if c == '\n' {
                break;
            } else if c == ' ' {
                indentation += 1;
            } else if c == '\t' {
                indentation += 4;
            } else {
                indentation = 0;
            }
        }
        indentation
    }
}

/// A lint emitted by the linter.
#[derive(Debug, Clone, thiserror::Error)]
pub struct Lint {
    /// A span indicating where the diagnostic is in the source code.
    pub span: Span,
    /// The lint level: allow, warning, error.
    pub level: LintLevel,
    /// The message the user will see in the code editor.
    pub message: &'static str,
    /// The help text the user will see in the code editor.
    pub help: &'static str,
    /// An enum identifying this lint.
    pub kind: LintKind,
    /// The suggested edits to fix the lint.
    pub code_action_edits: Vec<(String, Span)>,
}

impl std::hash::Hash for Lint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.span.hash(state);
        self.kind.hash(state);
    }
}

impl std::cmp::PartialEq for Lint {
    fn eq(&self, other: &Self) -> bool {
        self.span == other.span && self.kind == other.kind
    }
}

impl std::cmp::Eq for Lint {}

impl std::fmt::Display for Lint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Diagnostic for Lint {
    fn severity(&self) -> Option<miette::Severity> {
        match self.level {
            LintLevel::Allow => None,
            LintLevel::Warn | LintLevel::ForceWarn => Some(miette::Severity::Warning),
            LintLevel::Error | LintLevel::ForceError => Some(miette::Severity::Error),
        }
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        let source_span = miette::SourceSpan::from(self.span);
        let labeled_span = LabeledSpan::new_with_span(None, source_span);
        Some(Box::new(vec![labeled_span].into_iter()))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        if self.help.is_empty() {
            None
        } else {
            Some(Box::new(self.help))
        }
    }
}

/// A lint level. This defines if a lint will be treated as a warning or an error,
/// and if the lint level can be overridden by the user.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LintLevel {
    /// The lint is effectively disabled.
    #[default]
    Allow,
    /// The lint will be treated as a warning.
    Warn,
    /// The lint will be treated as a warning and cannot be overridden by the user.
    ForceWarn,
    /// The lint will be treated as an error.
    Error,
    /// The lint will be treated as an error and cannot be overridden by the user.
    ForceError,
}

impl Display for LintLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self {
            LintLevel::Allow => "",
            LintLevel::Warn | LintLevel::ForceWarn => "warning",
            LintLevel::Error | LintLevel::ForceError => "error",
        };

        write!(f, "{level}")
    }
}

/// End-user configuration for a specific lint or a lint group.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LintOrGroupConfig {
    /// An specific lint configuration.
    Lint(LintConfig),
    /// A lint group configuration.
    Group(GroupConfig),
}

/// End-user configuration for a specific lint.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LintConfig {
    #[serde(rename = "lint")]
    /// The lint name.
    pub kind: LintKind,
    /// The lint level.
    pub level: LintLevel,
}

/// Represents a lint name.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum LintKind {
    /// AST lint name.
    Ast(AstLint),
    /// HIR lint name.
    Hir(HirLint),
}
