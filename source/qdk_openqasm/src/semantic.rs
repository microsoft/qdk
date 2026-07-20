// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::io::InMemorySourceResolver;
use crate::io::SourceResolver;
use crate::parser::ParseResult;
use crate::parser::QasmSource;
use crate::parser::SourceSnapshot;

use crate::error::WithSource;
use crate::source::SourceMap;
pub(crate) use lowerer::Lowerer;

use std::sync::Arc;

pub mod ast;
pub(crate) mod const_eval;
pub mod error;
mod lowerer;
pub(crate) mod mut_visit;
pub use error::Error;
pub use error::SemanticErrorKind;
pub mod passes;
pub mod symbols;
pub mod types;
pub mod visit;

#[cfg(test)]
pub(crate) mod tests;

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub source: QasmSource,
    pub source_map: SourceMap,
    pub source_snapshot: SourceSnapshot,
    pub symbols: self::symbols::SymbolTable,
    pub program: self::ast::Program,
    pub errors: Vec<WithSource<crate::error::Error>>,
}

impl AnalysisResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.has_syntax_errors() || self.has_semantic_errors()
    }

    #[must_use]
    pub fn has_syntax_errors(&self) -> bool {
        self.source.has_errors()
    }

    #[must_use]
    pub fn has_semantic_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn syntax_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        let mut self_errors = self
            .source
            .errors()
            .iter()
            .map(|e| self.map_parse_error(e.clone()))
            .collect::<Vec<_>>();
        let include_errors = self
            .source
            .includes()
            .iter()
            .flat_map(QasmSource::all_errors)
            .map(|e| self.map_parse_error(e))
            .collect::<Vec<_>>();

        self_errors.extend(include_errors);
        self_errors
    }

    #[must_use]
    #[deprecated(note = "use syntax_errors instead")]
    pub fn sytax_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        self.syntax_errors()
    }

    #[must_use]
    pub fn semantic_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        self.errors.clone()
    }

    #[must_use]
    pub fn all_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        let mut parse_errors = self.syntax_errors();
        let sem_errors = self.semantic_errors();
        parse_errors.extend(sem_errors);
        parse_errors
    }

    #[must_use]
    pub fn errors(&self) -> Vec<WithSource<crate::error::Error>> {
        self.errors.clone()
    }

    fn map_parse_error(&self, error: crate::parser::Error) -> WithSource<crate::error::Error> {
        WithSource::from_map(
            &self.source_map,
            crate::error::Error(crate::error::ErrorKind::Parser(error)),
        )
    }
}

pub fn parse<S: Into<Arc<str>>, P: Into<Arc<str>>>(source: S, path: P) -> AnalysisResult {
    let source = source.into();
    let path = path.into();
    let mut resolver = InMemorySourceResolver::from_iter([(path.clone(), source.clone())]);
    parse_source(source, path, &mut resolver)
}

/// Parse a QASM file and return the parse result.
/// This function will resolve includes using the provided resolver.
/// If an include file cannot be resolved, an error will be returned.
/// Recursive and duplicate includes are reported as parse errors.
pub fn parse_source<R: SourceResolver, S: Into<Arc<str>>, P: Into<Arc<str>>>(
    source: S,
    path: P,
    resolver: &mut R,
) -> AnalysisResult {
    let res = crate::parser::parse_source(source, path, resolver);
    lower_parse_result(res)
}

#[must_use]
pub fn parse_sources(sources: &[(Arc<str>, Arc<str>)]) -> AnalysisResult {
    let (path, source) = sources
        .iter()
        .next()
        .expect("There should be at least one source");
    let mut resolver = sources.iter().cloned().collect::<InMemorySourceResolver>();
    parse_source(source.clone(), path.clone(), &mut resolver)
}

#[must_use]
pub fn lower_parse_result(parse_result: ParseResult) -> AnalysisResult {
    let analyzer = Lowerer::new(
        parse_result.source,
        parse_result.source_map,
        parse_result.source_snapshot,
    );
    analyzer.lower()
}
