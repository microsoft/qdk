// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::io::SourceResolver;
use crate::parser::ParseResult;
use crate::parser::QasmSource;
use crate::parser::SourceSnapshot;

use crate::error::WithSource;
use crate::source::SourceMap;
pub(crate) use lowerer::Lowerer;

use std::sync::Arc;

pub mod ast;
pub mod broadcast;
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
        self.has_parse_errors() || self.has_semantic_errors()
    }

    #[must_use]
    pub fn has_parse_errors(&self) -> bool {
        self.source.has_errors()
    }

    #[must_use]
    pub fn has_semantic_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn parse_errors(&self) -> Vec<WithSource<crate::error::Error>> {
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
        self.errors.clone()
    }

    #[must_use]
    pub fn all_errors(&self) -> Vec<WithSource<crate::error::Error>> {
        let mut parse_errors = self.parse_errors();
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

pub(crate) fn parse_source<R: SourceResolver, S: Into<Arc<str>>, P: Into<Arc<str>>>(
    source: S,
    path: P,
    resolver: &mut R,
) -> AnalysisResult {
    let res = crate::parser::parse_source(source, path, resolver);
    lower_parse_result(res)
}

#[must_use]
pub(crate) fn lower_parse_result(parse_result: ParseResult) -> AnalysisResult {
    let analyzer = Lowerer::new(
        parse_result.source,
        parse_result.source_map,
        parse_result.source_snapshot,
    );
    analyzer.lower()
}
