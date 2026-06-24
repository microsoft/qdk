// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::io::InMemorySourceResolver;
use crate::io::SourceResolver;
use crate::parser::QasmParseResult;
use crate::parser::QasmSource;

pub(crate) use lowerer::Lowerer;
use qsc_data_structures::error::WithSource;
use qsc_data_structures::source::SourceMap;

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
pub struct QasmSemanticParseResult {
    pub source: QasmSource,
    pub source_map: SourceMap,
    pub symbols: self::symbols::SymbolTable,
    pub program: self::ast::Program,
    pub errors: Vec<WithSource<crate::error::Error>>,
}

impl QasmSemanticParseResult {
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

    pub fn sytax_errors(&self) -> Vec<WithSource<crate::error::Error>> {
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
    pub fn semantic_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        self.errors().clone()
    }

    #[must_use]
    pub fn all_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        let mut parse_errors = self.sytax_errors();
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

pub fn parse<S: Into<Arc<str>>, P: Into<Arc<str>>>(source: S, path: P) -> QasmSemanticParseResult {
    let source = source.into();
    let path = path.into();
    let mut resolver = InMemorySourceResolver::from_iter([(path.clone(), source.clone())]);
    parse_source(source, path, &mut resolver)
}

/// Parse a QASM file and return the parse result.
/// This function will resolve includes using the provided resolver.
/// If an include file cannot be resolved, an error will be returned.
/// If a file is included recursively, a stack overflow occurs.
pub fn parse_source<R: SourceResolver, S: Into<Arc<str>>, P: Into<Arc<str>>>(
    source: S,
    path: P,
    resolver: &mut R,
) -> QasmSemanticParseResult {
    let res = crate::parser::parse_source(source, path, resolver);
    lower_parse_result(res)
}

#[must_use]
pub fn parse_sources(sources: &[(Arc<str>, Arc<str>)]) -> QasmSemanticParseResult {
    let (path, source) = sources
        .iter()
        .next()
        .expect("There should be at least one source");
    let mut resolver = sources.iter().cloned().collect::<InMemorySourceResolver>();
    parse_source(source.clone(), path.clone(), &mut resolver)
}

#[must_use]
pub fn lower_parse_result(parse_result: QasmParseResult) -> QasmSemanticParseResult {
    let analyzer = Lowerer::new(parse_result.source, parse_result.source_map);
    let sem_res = analyzer.lower();
    let errors = sem_res.all_errors();
    QasmSemanticParseResult {
        source: sem_res.source,
        source_map: sem_res.source_map,
        symbols: sem_res.symbols,
        program: sem_res.program,
        errors,
    }
}
