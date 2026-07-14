// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qdk_openqasm_parser::{source::SourceMap as ParserSourceMap, span::Span as ParserSpan};
use qsc_data_structures::{source::SourceMap, span::Span};

pub(crate) trait ParserSpanExt {
    fn to_qsharp(self) -> Span;
}

impl ParserSpanExt for ParserSpan {
    fn to_qsharp(self) -> Span {
        Span {
            lo: self.lo,
            hi: self.hi,
        }
    }
}

impl ParserSpanExt for Span {
    fn to_qsharp(self) -> Span {
        self
    }
}

pub(crate) fn to_qsharp_source_map(source_map: &ParserSourceMap) -> SourceMap {
    let sources = source_map
        .iter()
        .map(|source| (source.name.clone(), source.contents.clone()));
    let entry = source_map.entry().map(|source| source.contents.clone());
    SourceMap::new(sources, entry)
}
