// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use log::trace;
use qsc::line_column::{Encoding, Position};
use qsc::location::Location;
use qsc::openqasm::span::Span;

use crate::openqasm::map_spans_to_source_locations;

pub fn get_definition(
    sources: &[(Arc<str>, Arc<str>)],
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<Location> {
    let (res, id) =
        super::find_symbol_in_sources(sources, source_name, position, position_encoding);
    let id = id?;
    let symbol = &res.symbols[id];
    trace!(
        "get_definition: found symbol {} at {:?}",
        symbol.name, symbol.span
    );

    // If the symbol is a built-in symbol, we can't go to def
    if symbol.span == Span::default() {
        return None;
    }

    map_spans_to_source_locations(position_encoding, &res.source_map, vec![symbol.span]).pop()
}
