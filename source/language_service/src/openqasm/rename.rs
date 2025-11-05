// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::openqasm::get_reference_locations;
use crate::qsc_utils::into_range;
use log::trace;

use qsc::Span;
use qsc::line_column::{Encoding, Position, Range};
use qsc::location::Location;

pub fn prepare_rename(
    sources: &[(Arc<str>, Arc<str>)],
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<(Range, String)> {
    let (res, id) =
        super::find_symbol_in_sources(sources, source_name, position, position_encoding);
    let id = id?;
    let symbol = &res.symbols[id];

    // If the symbol is a built-in symbol, we can't rename it, return None
    if symbol.span == Span::default() {
        return None;
    }

    let range = into_range(position_encoding, symbol.span, &res.source_map);
    let name = symbol.name.clone();
    trace!("prepare_rename: found symbol {name} at {range:?}");
    Some((range, name))
}

pub fn get_rename(
    sources: &[(Arc<str>, Arc<str>)],
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Vec<Location> {
    let (res, id) =
        super::find_symbol_in_sources(sources, source_name, position, position_encoding);

    let Some(id) = id else {
        return vec![];
    };

    // If the symbol is a built-in symbol, we can't rename it
    let symbol = &res.symbols[id];
    if symbol.span == Span::default() {
        return vec![];
    }

    get_reference_locations(
        position_encoding,
        &res.program,
        &res.source_map,
        &res.symbols,
        id,
    )
}
