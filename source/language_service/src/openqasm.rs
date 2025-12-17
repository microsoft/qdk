// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod definition;
mod references;
mod rename;
use std::sync::Arc;

pub use definition::get_definition;
use qsc::SourceMap;
use qsc::line_column::Encoding;
use qsc::line_column::Position;
use qsc::line_column::Range;
use qsc::location::Location;
use qsc::openqasm::semantic::ast::Program;
use qsc::openqasm::semantic::passes::ReferenceFinder;
use qsc::openqasm::semantic::passes::SymbolFinder;
use qsc::openqasm::semantic::symbols::SymbolId;
use qsc::openqasm::semantic::symbols::SymbolTable;
pub use references::get_references;
pub use rename::get_rename;
pub use rename::prepare_rename;

use crate::compilation::source_position_to_package_offset;

/// Tries to find a symbol in the given source at the specified position.
/// returns the semantic parse result and the symbol ID if found.
fn find_symbol_in_sources(
    sources: &[(Arc<str>, Arc<str>)],
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> (
    qsc::openqasm::semantic::QasmSemanticParseResult,
    Option<qsc::openqasm::semantic::symbols::SymbolId>,
) {
    let res = qsc::openqasm::semantic::parse_sources(sources);
    let offset = source_position_to_package_offset(
        &res.source_map,
        source_name,
        position,
        position_encoding,
    );
    let id = SymbolFinder::get_symbol_at_offset(&res.program, offset, &res.symbols);
    (res, id)
}

fn map_spans_to_source_locations(
    position_encoding: Encoding,
    source_map: &SourceMap,
    spans: Vec<qsc::Span>,
) -> Vec<Location> {
    spans
        .into_iter()
        .map(|span| {
            let source = source_map
                .find_by_offset(span.lo)
                .expect("source should exist for offset");
            Location {
                source: source.name.clone(),
                range: Range::from_span(
                    position_encoding,
                    &source.contents,
                    &(span - source.offset),
                ),
            }
        })
        .collect::<Vec<_>>()
}

fn get_reference_locations(
    position_encoding: Encoding,
    program: &Program,
    source_map: &SourceMap,
    symbols: &SymbolTable,
    id: SymbolId,
) -> Vec<Location> {
    let reference_spans = ReferenceFinder::get_references(program, id, symbols);
    map_spans_to_source_locations(position_encoding, source_map, reference_spans)
}
