// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_data_structures::line_column::{Encoding, Position};
use qsc_eval::debug::Frame;
use qsc_fir::fir::{Global, PackageStoreLookup, StoreItemId};
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir;
use qsc_hir::hir::{Item, ItemKind};
use qsc_lowerer::map_fir_package_to_hir;
use std::fmt::Write;
use std::rc::Rc;

#[must_use]
pub(crate) fn format_call_stack(
    position_encoding: Encoding,
    store: &PackageStore,
    globals: &impl PackageStoreLookup,
    frames: Vec<Frame>,
    error: &dyn std::error::Error,
) -> String {
    let mut trace = String::new();
    writeln!(trace, "Error: {error}").expect("writing to string should succeed");
    trace.push_str("Call stack:\n");

    let mut frames = frames;
    frames.reverse();

    for frame in frames {
        let Some(Global::Callable(call)) = globals.get_global(frame.id) else {
            panic!("missing global");
        };

        trace.push_str("    at ");
        if frame.functor.adjoint {
            trace.push_str("Adjoint ");
        }
        if frame.functor.controlled > 0 {
            write!(trace, "Controlled({}) ", frame.functor.controlled)
                .expect("writing to string should succeed");
        }
        if let Some(item) = get_item_parent(store, frame.id) {
            if let Some(ns) = get_ns_name(&item) {
                write!(trace, "{ns}.").expect("writing to string should succeed");
            }
        }
        write!(trace, "{}", call.name.name).expect("writing to string should succeed");

        let name = get_item_file_name(store, frame.id);
        let pos = get_position(position_encoding, frame, store);
        write!(
            trace,
            " in {}:{}:{}",
            name.unwrap_or("<expression>".to_string()),
            pos.line + 1,
            pos.column + 1,
        )
        .expect("writing to string should succeed");

        trace.push('\n');
    }
    trace
}

#[must_use]
fn get_item_parent(store: &PackageStore, id: StoreItemId) -> Option<Item> {
    let package = map_fir_package_to_hir(id.package);
    let item = hir::LocalItemId::from(usize::from(id.item));
    store.get(package).and_then(|unit| {
        let item = unit.package.items.get(item)?;
        if let Some(parent) = item.parent {
            let parent = unit.package.items.get(parent)?;
            Some(parent.clone())
        } else {
            None
        }
    })
}

#[must_use]
fn get_item_file_name(store: &PackageStore, id: StoreItemId) -> Option<String> {
    let package = map_fir_package_to_hir(id.package);
    let item = hir::LocalItemId::from(usize::from(id.item));
    store.get(package).and_then(|unit| {
        let item = unit.package.items.get(item)?;
        let source = unit.sources.find_by_offset(item.span.lo);
        source.map(|s| s.name.to_string())
    })
}

#[must_use]
fn get_ns_name(item: &Item) -> Option<Rc<str>> {
    let ItemKind::Namespace(ns, _) = &item.kind else {
        return None;
    };
    Some(ns.name())
}

/// Converts the [`Span`] of [`Frame`] into a [`Position`].
fn get_position(position_encoding: Encoding, frame: Frame, store: &PackageStore) -> Position {
    let filename = get_item_file_name(store, frame.id).expect("file should exist");
    let package_id = map_fir_package_to_hir(frame.id.package);
    let unit = store.get(package_id).expect("package should exist");
    let source = unit
        .sources
        .find_by_name(&filename)
        .expect("source should exist");
    let contents = &source.contents;
    Position::from_utf8_byte_offset(position_encoding, contents, frame.span.lo - source.offset)
}
