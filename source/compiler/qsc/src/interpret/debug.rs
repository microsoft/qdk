// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_data_structures::line_column::Encoding;
use qsc_eval::debug::Frame;
use qsc_fir::fir::{Global, PackageStoreLookup, StoreItemId};
use qsc_frontend::compile::PackageStore;
use qsc_frontend::location::Location;
use qsc_hir::hir;
use qsc_hir::hir::{Item, ItemKind};
use qsc_lowerer::map_fir_package_to_hir;
use std::fmt::Write;
use std::rc::Rc;

#[must_use]
pub(crate) fn format_call_stack(
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
        if let Some(item) = get_item_parent(store, frame.id)
            && let Some(ns) = get_ns_name(&item)
        {
            write!(trace, "{ns}.").expect("writing to string should succeed");
        }
        write!(trace, "{}", call.name.name).expect("writing to string should succeed");

        let l = get_location(frame, store);
        write!(
            trace,
            " in {}:{}:{}",
            l.source,
            l.range.start.line + 1,
            l.range.start.column + 1,
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
fn get_ns_name(item: &Item) -> Option<Rc<str>> {
    let ItemKind::Namespace(ns, _) = &item.kind else {
        return None;
    };
    Some(ns.name())
}

/// Converts the [`Span`] of [`Frame`] into a [`Location`].
fn get_location(frame: Frame, store: &PackageStore) -> Location {
    let package_id = map_fir_package_to_hir(frame.id.package);
    Location::from(frame.span, package_id, store, Encoding::Utf8)
}
