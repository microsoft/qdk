// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::display::{CodeDisplay, Lookup, increase_header_level, parse_doc_for_summary};
use crate::table_of_contents::table_of_contents;
use qsc_ast::ast;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_data_structures::source::SourceMap;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_frontend::compile::{self, Dependencies, PackageStore, compile};
use qsc_frontend::resolve;
use qsc_hir::hir::{CallableKind, Item, ItemKind, Package, PackageId, Res, Visibility};
use qsc_hir::{hir, ty};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fmt::{Display, Formatter, Result};
use std::rc::Rc;
use std::sync::Arc;

// Name, Metadata, Content
type Files = Vec<(Rc<str>, Rc<str>, Rc<str>)>;
type FilesWithMetadata = Vec<(Rc<str>, Rc<Metadata>, Rc<str>)>;

// Namespace -> metadata for items
type ToC = FxHashMap<Rc<str>, Vec<Rc<Metadata>>>;

struct Metadata {
    uid: String,
    title: String,
    kind: MetadataKind,
    package: PackageKind,
    namespace: Rc<str>,
    name: Rc<str>,
    summary: String,
    signature: String,
}

impl Metadata {
    fn fully_qualified_name(&self) -> String {
        let mut buf = if let PackageKind::AliasedPackage(ref package_alias) = self.package {
            vec![format!("{package_alias}")]
        } else {
            vec![]
        };

        buf.push(self.namespace.to_string());
        buf.push(self.name.to_string());
        buf.join(".")
    }

    fn display_for_toc(&self) -> String {
        format!(
            "---
uid: {}
title: {}
description: {}
ms.date: {{TIMESTAMP}}
ms.topic: landing-page
---",
            self.uid, self.title, self.summary,
        )
    }

    fn display_for_item(&self) -> String {
        let kind = match &self.kind {
            MetadataKind::Function => "function",
            MetadataKind::Operation => "operation",
            MetadataKind::Udt => "udt",
            MetadataKind::Export => "export",
            MetadataKind::TableOfContents => "table of contents",
        };
        format!(
            "---
uid: {}
title: {}
description: \"Q# {}: {}\"
ms.date: {{TIMESTAMP}}
qsharp.kind: {}
qsharp.package: {}
qsharp.namespace: {}
qsharp.name: {}
qsharp.summary: \"{}\"
---",
            self.uid,
            self.title,
            self.title,
            self.summary,
            kind,
            self.package,
            self.namespace,
            self.name,
            self.summary
        )
    }
}

impl Display for Metadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.kind == MetadataKind::TableOfContents {
            write!(f, "{}", self.display_for_toc())
        } else {
            write!(f, "{}", self.display_for_item())
        }
    }
}

#[derive(PartialOrd, Ord, Eq, PartialEq, Clone)]
enum MetadataKind {
    Function,
    Operation,
    Udt,
    Export,
    TableOfContents,
}

impl Display for MetadataKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match &self {
            MetadataKind::Function => "function",
            MetadataKind::Operation => "operation",
            MetadataKind::Udt => "user defined type",
            MetadataKind::Export => "exported item",
            MetadataKind::TableOfContents => "table of contents",
        };
        write!(f, "{s}")
    }
}

#[derive(PartialOrd, Ord, Eq, PartialEq, Clone)]
enum PackageKind {
    UserCode,
    AliasedPackage(String),
    StandardLibrary,
    Core,
}

impl Display for PackageKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match &self {
            PackageKind::UserCode => "__Main__",
            PackageKind::AliasedPackage(alias) => alias,
            PackageKind::StandardLibrary => "__Std__",
            PackageKind::Core => "__Core__",
        };
        write!(f, "{s}")
    }
}

/// Represents an immutable compilation state.
#[derive(Debug)]
struct Compilation {
    /// Package store, containing the current package and all its dependencies.
    package_store: PackageStore,
    /// Current package id when provided.
    current_package_id: Option<PackageId>,
    /// Aliases for packages.
    dependencies: FxHashMap<PackageId, Arc<str>>,
}

impl Compilation {
    /// Creates a new `Compilation` by compiling standard library
    /// and additional sources.
    pub(crate) fn new(
        additional_program: Option<(PackageStore, &Dependencies, SourceMap)>,
        capabilities: Option<TargetCapabilityFlags>,
        language_features: Option<LanguageFeatures>,
    ) -> Self {
        let actual_capabilities = capabilities.unwrap_or_default();
        let actual_language_features = language_features.unwrap_or_default();

        let mut current_package_id: Option<PackageId> = None;
        let mut package_aliases: FxHashMap<PackageId, Arc<str>> = FxHashMap::default();

        let package_store =
            if let Some((mut package_store, dependencies, sources)) = additional_program {
                let unit = compile(
                    &package_store,
                    dependencies,
                    sources,
                    actual_capabilities,
                    actual_language_features,
                );
                // We ignore errors here (unit.errors vector) and use whatever
                // documentation we can produce. In future we may consider
                // displaying the fact of error presence on documentation page.

                for (package_id, package_alias) in dependencies {
                    if let Some(package_alias) = package_alias {
                        package_aliases.insert(*package_id, package_alias.clone());
                    }
                }

                current_package_id = Some(package_store.insert(unit));
                package_store
            } else {
                let mut package_store = PackageStore::new(compile::core());
                let std_unit = compile::std(&package_store, actual_capabilities);
                package_store.insert(std_unit);
                package_store
            };

        Self {
            package_store,
            current_package_id,
            dependencies: package_aliases,
        }
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

/// Determines the package kind for a given package in the compilation context
fn determine_package_kind(package_id: PackageId, compilation: &Compilation) -> Option<PackageKind> {
    let is_current_package = compilation.current_package_id == Some(package_id);

    if package_id == PackageId::CORE {
        // Core package is always included in the compilation.
        Some(PackageKind::Core)
    } else if package_id == 1.into() {
        // Standard package is currently always included, but this isn't enforced by the compiler.
        Some(PackageKind::StandardLibrary)
    } else if is_current_package {
        // This package could be user code if current package is specified.
        Some(PackageKind::UserCode)
    } else {
        // This is a either a direct dependency of the user code or
        // is not a package user can access (an indirect dependency).
        compilation
            .dependencies
            .get(&package_id)
            .map(|alias| PackageKind::AliasedPackage(alias.to_string()))
    }
}

/// Processes all packages in a compilation and builds a table-of-contents structure
fn build_toc_from_compilation(
    compilation: &Compilation,
    mut files: Option<&mut FilesWithMetadata>,
) -> ToC {
    let display = &CodeDisplay { compilation };

    let mut toc: ToC = FxHashMap::default();

    for (package_id, unit) in &compilation.package_store {
        let Some(package_kind) = determine_package_kind(package_id, compilation) else {
            continue;
        };

        let is_current_package = compilation.current_package_id == Some(package_id);
        let package = &unit.package;

        for (_, item) in &package.items {
            if let Some((ns, metadata)) = generate_doc_for_item(
                package_id,
                package,
                package_kind.clone(),
                is_current_package,
                item,
                display,
                files.as_deref_mut().unwrap_or(&mut vec![]),
            ) {
                toc.entry(ns).or_default().push(metadata);
            }
        }
    }

    toc
}

/// Generates and returns documentation files for the standard library
/// and additional sources (if specified.)
#[must_use]
pub fn generate_docs(
    additional_sources: Option<(PackageStore, &Dependencies, SourceMap)>,
    capabilities: Option<TargetCapabilityFlags>,
    language_features: Option<LanguageFeatures>,
) -> Files {
    // Capabilities should default to all capabilities for documentation generation.
    let capabilities = Some(capabilities.unwrap_or(TargetCapabilityFlags::all()));
    let compilation = Compilation::new(additional_sources, capabilities, language_features);
    let mut files: FilesWithMetadata = vec![];

    let mut toc = build_toc_from_compilation(&compilation, Some(&mut files));

    // Generate Overview files for each namespace
    for (ns, items) in &mut toc {
        generate_index_file(&mut files, ns, items);
    }

    generate_top_index(&mut files, &mut toc);

    // We want to sort documentation files in a meaningful way.
    // First, we want to put files for the current project, if it exists.
    // Then we want to put explicit dependencies of the current project, if they exist.
    // Then we want to add built-in std package. And finally built-in core package.
    // Namespaces within packages should be sorted alphabetically and
    // items with a namespace should be also sorted alphabetically with the index file appearing first.
    // Also, items without any metadata (table of content) should come last.
    files.sort_by_key(|file| {
        let prefix = if file.0.ends_with("index.md") {
            "0"
        } else {
            "1"
        };
        let name_key = format!("{}{}", prefix, file.1.name);
        (file.1.package.clone(), file.1.namespace.clone(), name_key)
    });

    let mut result: Files = files
        .into_iter()
        .map(|(name, metadata, content)| (name, Rc::from(metadata.to_string().as_str()), content))
        .collect();

    generate_toc(&mut toc, &mut result);

    result
}

fn generate_doc_for_item<'a>(
    default_package_id: PackageId,
    package: &'a Package,
    package_kind: PackageKind,
    include_internals: bool,
    item: &'a Item,
    display: &'a CodeDisplay,
    files: &mut FilesWithMetadata,
) -> Option<(Rc<str>, Rc<Metadata>)> {
    let (true_package, true_item) = resolve_export(
        default_package_id,
        package,
        include_internals,
        item,
        display,
    )?;

    // Get namespace for item
    let ns = get_namespace(package, item)?;

    // Add file
    let (metadata, content) = if matches!(item.kind, ItemKind::Export(_, _)) {
        let true_ns = get_namespace(true_package, true_item)?;
        generate_exported_file_content(
            package_kind.clone(),
            &ns,
            item,
            display,
            &true_ns,
            true_item,
        )?
    } else {
        generate_file_content(package_kind, &ns, item, display)?
    };
    let file_name = Rc::from(format!("{ns}/{}.md", metadata.name).as_str());
    let file_content = Rc::from(content.as_str());
    let met = Rc::from(metadata);
    files.push((file_name, met.clone(), file_content));

    Some((ns.clone(), met))
}

fn generate_file_content(
    package_kind: PackageKind,
    ns: &Rc<str>,
    item: &Item,
    display: &CodeDisplay,
) -> Option<(Metadata, String)> {
    let metadata = get_metadata(package_kind, ns.clone(), item, display)?;

    let doc = increase_header_level(&item.doc);
    let title = &metadata.title;
    let fqn = &metadata.fully_qualified_name();
    let sig = &metadata.signature;

    let content = format!(
        "# {title}

Fully qualified name: {fqn}

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

    Some((metadata, content))
}

#[allow(clippy::assigning_clones)]
fn generate_exported_file_content(
    package_kind: PackageKind,
    ns: &Rc<str>,
    item: &Item,
    display: &CodeDisplay,
    true_ns: &Rc<str>,
    true_item: &Item,
) -> Option<(Metadata, String)> {
    let mut metadata = get_metadata(package_kind.clone(), ns.clone(), item, display)?;

    let doc = increase_header_level(&item.doc);
    let title = &metadata.title;
    let fqn = &metadata.fully_qualified_name();

    // Note: we are assuming the package kind does not change
    let true_metadata = get_metadata(package_kind, true_ns.clone(), true_item, display)?;
    let true_fqn = true_metadata.fully_qualified_name();

    let summary = format!(
        "This is an exported item. The actual definition is found here: [{true_fqn}](xref:Qdk.{true_fqn})"
    );

    metadata.summary = summary.clone();

    let content = format!(
        "# {title}

Fully qualified name: {fqn}

{summary}
"
    );

    let content = if doc.is_empty() {
        content
    } else {
        format!("{content}\n{doc}\n")
    };

    Some((metadata, content))
}

fn generate_index_file(files: &mut FilesWithMetadata, ns: &Rc<str>, items: &mut Vec<Rc<Metadata>>) {
    if items.is_empty() {
        return;
    }

    let short_name = if ns.starts_with("Microsoft.Quantum") {
        ns.as_ref()
    } else {
        ns.split('.')
            .next_back()
            .expect("Namespaces should have at least one part.")
    };

    let package_kind = items[0].package.clone();
    let metadata = Metadata {
        uid: format!("Qdk.{ns}-toc"),
        title: format!("{ns} namespace"),
        kind: MetadataKind::TableOfContents,
        package: package_kind,
        namespace: ns.clone(),
        name: "Overview".into(),
        summary: format!("Table of contents for the Q# {short_name} namespace"),
        signature: String::new(),
    };

    items.sort_by_key(|item| item.name.clone());
    let content = items
        .iter()
        .map(|item| {
            format!(
                "| [{}](xref:Qdk.{}) | {} |",
                item.name,
                item.fully_qualified_name(),
                item.summary.replace('|', "\\|")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        "# {ns}

The {ns} namespace contains the following items:

| Name | Description |
|------|-------------|
{content}
",
    );

    let rc_met = Rc::from(metadata);
    items.insert(0, rc_met.clone());

    let file_name = Rc::from(format!("{ns}/index.md").as_str());
    let file_content = Rc::from(content.as_str());
    files.push((file_name, rc_met, file_content));
}

fn generate_top_index(files: &mut FilesWithMetadata, toc: &mut ToC) {
    let empty_ns: Rc<str> = Rc::from(String::new().as_str());
    let metadata = Metadata {
        uid: "Microsoft.Quantum.apiref-toc".to_string(),
        title: "Q# standard libraries for the Azure Quantum Development Kit".to_string(),
        kind: MetadataKind::TableOfContents,
        package: PackageKind::StandardLibrary,
        namespace: empty_ns.clone(),
        name: "Overview".into(),
        summary:
            "Table of contents for the Q# standard libraries for Azure Quantum Development Kit"
                .to_string(),
        signature: String::new(),
    };

    let contents = table_of_contents();

    files.push((Rc::from("index.md"), Rc::from(metadata), Rc::from(contents)));

    toc.insert(empty_ns, vec![]);
}

/// Generates the Table of Contents file, toc.yml
fn generate_toc(map: &mut ToC, files: &mut Files) {
    let header = "
# This file is automatically generated.
# Please do not modify this file manually, or your changes will be lost when
# documentation is rebuilt.";
    let mut table = map
        .iter_mut()
        .map(|(namespace, items)| {
            if namespace.is_empty() {
                let content = "- items:
  name: Overview
  uid: Microsoft.Quantum.apiref-toc";
                (namespace, content.to_string())
            } else {
                let items_str = items
                    .iter()
                    .map(|item| format!("  - {{name: {}, uid: {}}}", item.name, item.uid))
                    .collect::<Vec<String>>()
                    .join("\n");
                let content =
                    format!("- items:\n{items_str}\n  name: {namespace}\n  uid: Qdk.{namespace}");
                (namespace, content)
            }
        })
        .collect::<Vec<_>>();

    table.sort_unstable_by_key(|(n, _)| {
        // Ensures that the Microsoft.Quantum.Unstable namespaces are listed last.
        if n.starts_with("Microsoft.Quantum.Unstable") {
            format!("1{n}")
        } else {
            format!("0{n}")
        }
    });
    let table = table
        .into_iter()
        .map(|(_, c)| c)
        .collect::<Vec<_>>()
        .join("\n");
    let content = format!("{header}\n{table}");
    let content = content.as_str();

    let file_name = Rc::from("toc.yml");
    let file_metadata = Rc::from("");
    let file_content = Rc::from(content);
    files.push((file_name, file_metadata, file_content));
}

fn get_namespace(package: &Package, item: &Item) -> Option<Rc<str>> {
    let local_id = item.parent?;
    let parent = package
        .items
        .get(local_id)
        .expect("Could not resolve parent item id");
    let ItemKind::Namespace(name, _) = &parent.kind else {
        return None;
    };
    if name.starts_with("QIR") {
        None // We ignore "QIR" namespaces
    } else {
        let name = name.name();
        if name.to_lowercase().starts_with("std.openqasm") {
            None // We ignore openqasm namespaces
        } else {
            Some(name)
        }
    }
}

// Recursively resolves export items until it can find the root definition.
// Returns the package and item of the root definition for an item.
// If given the root definition, it will return the same item.
fn resolve_export<'a>(
    default_package_id: PackageId,
    package: &'a Package,
    include_internals: bool,
    item: &'a Item,
    display: &'a CodeDisplay,
) -> Option<(&'a Package, &'a Item)> {
    // Filter out items that are not visible or are namespaces
    if !include_internals && (item.visibility == Visibility::Internal) {
        return None;
    }
    if matches!(item.kind, ItemKind::Namespace(_, _)) {
        return None;
    }
    if let ItemKind::Export(_, Res::Item(id)) = item.kind {
        let (exported_item, exported_package, _) =
            display.compilation.resolve_item(default_package_id, &id);
        return resolve_export(
            default_package_id,
            exported_package,
            include_internals,
            exported_item,
            display,
        );
    }

    Some((package, item))
}

fn get_metadata(
    package_kind: PackageKind,
    ns: Rc<str>,
    item: &Item,
    display: &CodeDisplay,
) -> Option<Metadata> {
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
        ItemKind::Export(name, _) => Some((
            name.name.clone(),
            // If we want to show docs for exports, we could do that here.
            String::new(),
            MetadataKind::Export,
        )),
    }?;

    let summary = parse_doc_for_summary(&item.doc)
        .replace("\r\n", " ")
        .replace('\n', " ");

    Some(Metadata {
        uid: format!("Qdk.{ns}.{name}"),
        title: format!("{name} {kind}"),
        kind,
        package: package_kind,
        namespace: ns,
        name,
        summary,
        signature,
    })
}

/// Generates summary documentation organized by namespace.
/// Returns a map of namespace -> metadata items for easier testing and manipulation.
fn generate_summaries_map() -> BTreeMap<String, Vec<Rc<Metadata>>> {
    let compilation = Compilation::new(None, None, None);

    // Use the shared logic to build ToC structure
    let toc = build_toc_from_compilation(&compilation, None);

    // Convert ToC to BTreeMap, filtering out table of contents entries
    let mut result = BTreeMap::new();

    for (ns, items) in toc {
        let mut summaries = Vec::new();

        for item in items {
            // Skip table of contents entries
            if item.kind == MetadataKind::TableOfContents {
                continue;
            }

            summaries.push(item);
        }

        if !summaries.is_empty() {
            // Sort items within namespace
            summaries.sort_by_key(|item| item.name.clone());
            result.insert(ns.to_string(), summaries);
        }
    }

    result
}
/// Converts a Metadata item to its markdown representation
fn metadata_to_markdown(item: &Metadata) -> String {
    let mut result = format!("## {}\n\n", item.name);
    let _ = write!(result, "```qsharp\n{}\n```\n\n", item.signature);
    if !item.summary.is_empty() {
        let _ = write!(result, "{}\n\n", item.summary);
    }
    result
}

/// Generates markdown summary for a single namespace
fn generate_namespace_summary(namespace: &str, items: &[Rc<Metadata>]) -> String {
    let mut result = format!("# {namespace}\n\n");

    for item in items {
        result.push_str(&metadata_to_markdown(item));
    }

    result
}

/// Generates summary documentation organized by namespace.
/// Returns a single markdown string with namespace headers and minimal item documentation
/// containing just function signatures and summaries for efficient consumption by language models.
#[must_use]
pub fn generate_summaries() -> String {
    let summaries_map = generate_summaries_map();

    // Generate markdown output organized by namespace
    let mut result = String::new();

    // Sort namespaces for consistent output (BTreeMap already sorts keys)
    for (ns, items) in &summaries_map {
        result.push_str(&generate_namespace_summary(ns, items));
    }

    result
}
