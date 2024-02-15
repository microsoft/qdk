// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::display::{increase_header_level, parse_doc_for_summary};
use crate::display::{CodeDisplay, Lookup};
use qsc_ast::ast;
use qsc_frontend::compile::{self, PackageStore, RuntimeCapabilityFlags};
use qsc_frontend::resolve;
use qsc_hir::hir::{CallableKind, Item, ItemKind, Package, PackageId, Visibility};
use qsc_hir::{hir, ty};
use rustc_hash::FxHashMap;
use std::fmt::{Display, Formatter, Result};
use std::rc::Rc;
use std::sync::Arc;

/// Represents an immutable compilation state.
#[derive(Debug)]
struct Compilation {
    /// Package store, containing the current package and all its dependencies.
    package_store: PackageStore,
}

impl Compilation {
    /// Creates a new `Compilation` by compiling sources.
    pub(crate) fn new() -> Self {
        let mut package_store = PackageStore::new(compile::core());
        package_store.insert(compile::std(&package_store, RuntimeCapabilityFlags::all()));

        Self { package_store }
    }
}

impl Lookup for Compilation {
    fn get_ty(&self, _: ast::NodeId) -> Option<&ty::Ty> {
        unimplemented!("Not needed for docs generation")
    }

    fn get_res(&self, _: ast::NodeId) -> Option<&resolve::Res> {
        unimplemented!("Not needed for docs generation")
    }

    fn resolve_item_relative_to_user_package(
        &self,
        _: &hir::ItemId,
    ) -> (&hir::Item, &hir::Package, hir::ItemId) {
        unimplemented!("Not needed for docs generation")
    }

    /// Returns the hir `Item` node referred to by `res`.
    /// `Res`s can resolve to external packages, and the references
    /// are relative, so here we also need the
    /// local `PackageId` that the `res` itself came from.
    fn resolve_item_res(
        &self,
        local_package_id: PackageId,
        res: &hir::Res,
    ) -> (&hir::Item, hir::ItemId) {
        match res {
            hir::Res::Item(item_id) => {
                let (item, _, resolved_item_id) = self.resolve_item(local_package_id, item_id);
                (item, resolved_item_id)
            }
            _ => panic!("expected to find item"),
        }
    }

    /// Returns the hir `Item` node referred to by `item_id`.
    /// `ItemId`s can refer to external packages, and the references
    /// are relative, so here we also need the local `PackageId`
    /// that the `ItemId` originates from.
    fn resolve_item(
        &self,
        local_package_id: PackageId,
        item_id: &hir::ItemId,
    ) -> (&hir::Item, &hir::Package, hir::ItemId) {
        // If the `ItemId` contains a package id, use that.
        // Lack of a package id means the item is in the
        // same package as the one this `ItemId` reference
        // came from. So use the local package id passed in.
        let package_id = item_id.package.unwrap_or(local_package_id);
        let package = &self
            .package_store
            .get(package_id)
            .expect("package should exist in store")
            .package;
        (
            package
                .items
                .get(item_id.item)
                .expect("item id should exist"),
            package,
            hir::ItemId {
                package: Some(package_id),
                item: item_id.item,
            },
        )
    }
}

pub fn generate_docs() -> FxHashMap<Arc<str>, Arc<str>> {
    let compilation = Compilation::new();
    let mut file_map: FxHashMap<Arc<str>, Arc<str>> = FxHashMap::default();

    let display = &CodeDisplay {
        compilation: &compilation,
    };

    let mut toc: FxHashMap<Rc<str>, Vec<String>> = FxHashMap::default();
    for (_, unit) in &compilation.package_store {
        let package = &unit.package;
        for (_, item) in &package.items {
            if let Some((ns, line)) = generate_doc_for_item(package, item, display, &mut file_map) {
                toc.entry(ns).or_default().push(line);
            }
        }
    }

    generate_toc(&toc, &mut file_map);

    file_map
}

fn generate_doc_for_item<'a>(
    package: &'a Package,
    item: &'a Item,
    display: &'a CodeDisplay,
    file_map: &mut FxHashMap<Arc<str>, Arc<str>>,
) -> Option<(Rc<str>, String)> {
    // Filter items
    if item.visibility == Visibility::Internal || matches!(item.kind, ItemKind::Namespace(_, _)) {
        return None;
    }

    // Get namespace for item
    let ns = get_namespace(package, item)?;

    // Add file
    let (title, content) = generate_file(&ns, item, display)?;
    let file_name: Arc<str> = Arc::from(format!("{ns}/{title}.md").as_str());
    let file_content: Arc<str> = Arc::from(content.as_str());
    file_map.insert(file_name, file_content);

    // Create toc line
    let line = format!("  - {{name: {title}, uid: {ns}.{title}}}");

    // Return (ns, line)
    Some((ns.clone(), line))
}

fn get_namespace(package: &Package, item: &Item) -> Option<Rc<str>> {
    match item.parent {
        Some(local_id) => {
            let parent = package
                .items
                .get(local_id)
                .expect("Could not resolve parent item id");
            match &parent.kind {
                ItemKind::Namespace(name, _) => {
                    if name.name.starts_with("QIR") {
                        None // We ignore "QIR" namespaces
                    } else {
                        Some(name.name.clone())
                    }
                }
                _ => None,
            }
        }
        None => None,
    }
}

fn generate_file(ns: &Rc<str>, item: &Item, display: &CodeDisplay) -> Option<(Rc<str>, String)> {
    let metadata = get_metadata(ns.clone(), item, display)?;

    let doc = increase_header_level(&item.doc);
    let title = &metadata.title;
    let sig = &metadata.signature;

    let content = format!(
        "{metadata}

# {title}

Namespace: [{ns}](xref:{ns})

```qsharp
{sig}
```
"
    );

    let content = if doc.is_empty() {
        content
    } else {
        format!("{content}\n{doc}\n")
    };

    Some((metadata.name.clone(), content))
}

struct Metadata {
    uid: String,
    title: String,
    topic: String,
    kind: MetadataKind,
    namespace: Rc<str>,
    name: Rc<str>,
    summary: String,
    signature: String,
}

impl Display for Metadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let kind = match &self.kind {
            MetadataKind::Function => "function",
            MetadataKind::Operation => "opeartion",
            MetadataKind::Udt => "udt",
        };
        write!(
            f,
            "---
uid: {}
title: {}
ms.date: {{TIMESTAMP}}
ms.topic: {}
qsharp.kind: {}
qsharp.namespace: {}
qsharp.name: {}
qsharp.summary: {}
---",
            self.uid, self.title, self.topic, kind, self.namespace, self.name, self.summary
        )
    }
}

enum MetadataKind {
    Function,
    Operation,
    Udt,
}

fn get_metadata(ns: Rc<str>, item: &Item, display: &CodeDisplay) -> Option<Metadata> {
    let (name, signature, kind) = match &item.kind {
        ItemKind::Callable(decl) => Some((
            decl.name.name.clone(),
            display.hir_callable_decl(decl).to_string(),
            match &decl.kind {
                CallableKind::Function => MetadataKind::Function,
                CallableKind::Operation => MetadataKind::Operation,
            },
        )),
        ItemKind::Ty(ident, udt) => Some((
            ident.name.clone(),
            display.hir_udt(udt).to_string(),
            MetadataKind::Udt,
        )),
        ItemKind::Namespace(_, _) => None,
    }?;

    Some(Metadata {
        uid: format!("{ns}.{name}"),
        title: match &kind {
            MetadataKind::Function => format!("{name} function"),
            MetadataKind::Operation => format!("{name} operation"),
            MetadataKind::Udt => format!("{name} user defined type"),
        },
        topic: "managed-reference".to_string(),
        kind,
        namespace: ns,
        name,
        summary: parse_doc_for_summary(&item.doc),
        signature,
    })
}

/// Generates the Table of Contents file, toc.yml
fn generate_toc(
    map: &FxHashMap<Rc<str>, Vec<String>>,
    file_map: &mut FxHashMap<Arc<str>, Arc<str>>,
) {
    let header = "
# This file is automatically generated.
# Please do not modify this file manually, or your changes will be lost when
# documentation is rebuilt.";
    let table = map
        .iter()
        .map(|(namespace, lines)| {
            let items_str = lines.join("\n");
            format!("- items:\n{items_str}\n  name: {namespace}\n  uid: {namespace}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!("{header}\n{table}");

    let file_name: Arc<str> = Arc::from("toc.yml");
    let file_content: Arc<str> = Arc::from(content.as_str());
    file_map.insert(file_name, file_content);
}
