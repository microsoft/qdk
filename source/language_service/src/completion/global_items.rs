// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Completion, text_edits::TextEditRange};
use crate::{
    compilation::Compilation,
    protocol::{CompletionItemKind, TextEdit},
};
use qsc::{
    NamespaceId, PRELUDE,
    display::{CodeDisplay, Lookup},
    hir::{CallableDecl, ItemId, ItemKind, ty::Udt},
    resolve::{Local, NameKind},
};
use rustc_hash::FxHashSet;
use std::{cmp::Ordering, mem::take, rc::Rc};

/// Provides the globals that are visible or importable at the cursor offset.
pub(super) struct Globals<'a> {
    compilation: &'a Compilation,
    locals: Vec<Local>,
}

impl<'a> Globals<'a> {
    pub fn init(offset: u32, compilation: &'a Compilation) -> Self {
        let global_scope = &compilation.user_unit().ast.globals;
        let mut locals = compilation.user_unit().ast.locals.get_all_at_offset(offset);

        // Always include the prelude as an imported namespace as well
        locals.extend(PRELUDE.iter().filter_map(|ns| {
            let namespace_id = global_scope.find_namespace(ns.iter().copied(), None);
            namespace_id.map(|namespace_id| Local::NamespaceImport(namespace_id, None))
        }));

        Self {
            compilation,
            locals,
        }
    }

    /// Returns all terms, and any namespaces that may lead to terms, that
    /// are available at the current offset.
    ///
    /// If an item's name is not in scope, includes the item with any
    /// text edits (auto-imports, etc.) to bring it into scope.
    pub fn expr_names(&self, edit_range: &TextEditRange) -> Vec<Vec<Completion>> {
        // include UDTs as well since they can be constructors
        let mut completions = self.items(
            NameKind::Term,
            false, // in_scope_only
            Some(edit_range),
        );

        completions.push(self.namespaces());

        completions
    }

    /// Returns all types, and any namespaces that may lead to types, that
    /// are available at the current offset.
    ///
    /// If an item's name is not in scope, includes the item with any
    /// text edits (auto-imports, etc.) to bring it into scope.
    pub fn type_names(&self, edit_range: &TextEditRange) -> Vec<Vec<Completion>> {
        let mut completions = self.items(
            NameKind::Ty,
            false, // in_scope_only
            Some(edit_range),
        );

        completions.push(self.namespaces());

        completions
    }

    /// Returns all importables, and any namespaces that may lead to importables,
    /// that are available at the current offset.
    ///
    /// If an item's name is not in scope, the item is *not* included,
    /// as it wouldn't quite make sense, for an `import` completion to
    /// bring in text edits that auto-import other items.
    pub fn importable_names(&self) -> Vec<Vec<Completion>> {
        // include UDTs as well since they can be constructors
        let mut completions = self.items(
            NameKind::Importable,
            true, // in_scope_only
            None,
        );

        completions.push(self.namespaces());

        completions
    }

    /// Returns all namespaces that are available at the current offset.
    pub fn namespaces(&self) -> Vec<Completion> {
        self.namespaces_in(&[]).into_iter().flatten().collect()
    }

    /// Returns all terms, and any namespaces that may lead to terms, that
    /// match the given qualifier prefix.
    pub fn expr_names_in(&self, qualifier: &[Rc<str>]) -> Vec<Vec<Completion>> {
        let mut groups = self.items_in(qualifier, NameKind::Term);

        groups.extend(self.namespaces_in(qualifier));
        groups
    }

    /// Returns all types, and any namespaces that may lead to types, that
    /// match the given qualifier prefix.
    pub fn type_names_in(&self, qualifier: &[Rc<str>]) -> Vec<Vec<Completion>> {
        let mut groups = self.items_in(qualifier, NameKind::Ty);

        groups.extend(self.namespaces_in(qualifier));
        groups
    }

    /// Returns all importables, and any namespaces that may lead to importables, that
    /// match the given qualifier prefix.
    pub fn importable_names_in(&self, qualifier: &[Rc<str>]) -> Vec<Vec<Completion>> {
        let mut groups = self.items_in(qualifier, NameKind::Importable);

        groups.extend(self.namespaces_in(qualifier));
        groups
    }

    /// Returns all namespaces that match the given qualifier prefix.
    pub fn namespaces_in(&self, qualifier: &[Rc<str>]) -> Vec<Vec<Completion>> {
        let namespaces_matching_qualifier = self.namespaces_matching_qualifier(qualifier);

        let mut children = namespaces_matching_qualifier
            .flat_map(|namespace| self.global_scope().namespace_children(namespace))
            .collect::<Vec<_>>();

        children.sort();
        children.dedup();

        vec![
            children
                .into_iter()
                .map(|name| Completion::new(name.to_string(), CompletionItemKind::Module))
                .collect(),
        ]
    }

    /// Returns all item names that are available at the current offset,
    /// taking into account any imports that are in scope.
    ///
    /// If the item name is not in scope, and `in_scope_only` is false,
    /// includes the item with text edits (auto-imports, etc.) to bring the item into scope.
    ///
    /// e.g. if the following imports are in scope:
    /// `import A.*;`
    /// `import B as C;`
    ///
    /// Then, if `in_scope_only` is false:
    ///     - Items from namespace `A` will be included without any edits
    ///     - Items from `B` will be included as `C.<item_name>`
    ///     - Items will any other namespace will include an auto-import edit,
    ///         `import D.<item_name>;` to bring the item into scope.
    ///
    /// If `in_scope_only` is true, then only items from `A` will be included.
    fn items(
        &self,
        name_kind: NameKind,
        in_scope_only: bool,
        edit_range: Option<&TextEditRange>,
    ) -> Vec<Vec<Completion>> {
        let namespaces = self.namespaces_to_search(name_kind, in_scope_only);
        self.items_in_namespaces(namespaces, name_kind, edit_range)
    }

    /// Returns all items that match the given qualifier prefix.
    ///
    /// The qualifier is resolved taking into account any imports that are in scope.
    /// e.g. `A.` will return items in:
    ///     - namespace `A`
    ///     - namespace `B` if an `import B as A;` is in scope
    ///     - namespace `C.A` if an `import C.*` is in scope
    fn items_in(&'a self, qualifier: &[Rc<str>], name_kind: NameKind) -> Vec<Vec<Completion>> {
        let namespaces = self
            .namespaces_matching_qualifier(qualifier)
            .map(|namespace_id| (namespace_id, Availability::Qualified));

        self.items_in_namespaces(namespaces, name_kind, None)
    }

    /// Gathers all namespaces and their availability information, i.e. whether
    /// they are already open in the current scope or need a text edit to bring them into scope.
    fn namespaces_to_search(
        &self,
        name_kind: NameKind,
        in_scope_only: bool,
    ) -> impl Iterator<Item = (NamespaceId, Availability)> {
        let open_namespaces = self.namespaces_matching_qualifier(&[]).collect::<Vec<_>>();

        let namespaces: FxHashSet<NamespaceId> = if in_scope_only {
            // Only include already open namespaces
            open_namespaces.iter().copied().collect()
        } else {
            // Include all known namespaces
            self.global_scope()
                .table(name_kind)
                .iter()
                .map(|(namespace_id, _)| namespace_id)
                .collect()
        };

        namespaces.into_iter().filter_map(move |namespace| {
            if open_namespaces.contains(&namespace) {
                // Namespace already in open in the current scope
                Some((namespace, Availability::InScope))
            } else if let Some(alias) = self.locals.iter().find_map(|local| {
                if let Local::NamespaceImport(imported, Some(alias)) = local
                    && namespace == *imported
                {
                    return Some(alias.clone());
                }
                None
            }) {
                // Namespace is imported with an alias
                Some((namespace, Availability::InAliasedNamespace(alias)))
            } else if self
                .global_scope()
                .format_namespace_name(namespace)
                .starts_with("Std.OpenQASM")
            {
                // Don't suggest auto-imports for OpenQASM namespaces
                None
            } else {
                // If there are no existing exact or glob imports of the item,
                // no open aliases for the namespace it's in,
                // and we are not in the same namespace as the item,
                // we need to add an import for it.
                Some((namespace, Availability::NeedImport(namespace)))
            }
        })
    }

    /// Returns all namespaces that match the given qualifier prefix.
    /// The qualifier can be empty, in which case only top-level and open namespaces
    /// are returned.
    ///
    /// The qualifier is resolved taking into account any imports that are in scope.
    /// e.g.:
    ///
    /// namespace A.B {}
    /// namespace C.D {}
    /// namespace E.A.G {}
    /// namespace H { import C as A; import E.*; }
    ///
    /// `A.` inside of the namespace `H` will return `B`, `D` and `G`.
    fn namespaces_matching_qualifier(
        &self,
        qualifier: &[Rc<str>],
    ) -> impl Iterator<Item = NamespaceId> {
        let global_scope = &self.compilation.user_unit().ast.globals;
        self.locals
            .iter()
            .filter_map(|local| match local {
                Local::NamespaceImport(namespace_id, None) => global_scope
                    .find_namespace(qualifier.iter().map(AsRef::as_ref), Some(*namespace_id)),
                Local::NamespaceImport(namespace_id, Some(alias))
                    if Some(alias) == qualifier.first() =>
                {
                    global_scope.find_namespace(
                        qualifier[1..].iter().map(AsRef::as_ref),
                        Some(*namespace_id),
                    )
                }
                _ => None,
            })
            .chain(global_scope.find_namespace(qualifier.iter().map(AsRef::as_ref), None))
    }

    /// Collects all matching items in the given namespaces and returns
    /// them as completions.
    fn items_in_namespaces(
        &self,
        namespaces: impl IntoIterator<Item = (NamespaceId, Availability)>,
        name_kind: NameKind,
        edit_range: Option<&TextEditRange>,
    ) -> Vec<Vec<Completion>> {
        let mut candidates = Vec::new();
        let global_scope = self.global_scope();
        let names_table = global_scope.table(name_kind);

        for (namespace_id, namespace_availability) in namespaces {
            if let Some(names) = names_table.get(namespace_id) {
                for (name, res) in names {
                    if let Some(item_id) = res.item_id() {
                        let (item, _, _) = self
                            .compilation
                            .resolve_item_relative_to_user_package(&item_id);
                        let decl = match &item.kind {
                            ItemKind::Callable(callable_decl) => {
                                ItemDecl::Callable(callable_decl.as_ref())
                            }
                            ItemKind::Ty(_ident, udt) => ItemDecl::Udt(udt),
                            ItemKind::Export(..) | ItemKind::Namespace(..) => continue,
                        };

                        let availability = if namespace_availability != Availability::Qualified
                            && self.locals.iter().any(|local| {
                                if let Local::Item(import_item_id, ..) = local
                                    && *import_item_id == item_id
                                {
                                    return true;
                                }
                                false
                            }) {
                            // Item is available in the local scope through a direct import.
                            Availability::Local
                        } else {
                            namespace_availability.clone()
                        };

                        let item = ItemInfo {
                            item_id,
                            namespace_id,
                            name: name.clone(),
                            decl,
                            availability,
                        };
                        candidates.push(item);
                    }
                }
            }
        }

        // Remove duplicate paths to the same item. The same item may be available
        // by multiple names by way of imports/exports.
        // e.g.
        // `namespace A { function B() : Unit {}; export B; export B as C; }`
        // `namespace D { export A.B; }`
        // `namespace E { open A; import A.B as X }`
        // Makes the same item available as `A.B`, `A.C`, `D.B`, `B` and `X`
        // when in `namespace E`.

        // We want to keep distinct names to the same item (`B`, `C`) but
        // not distinct paths to the same item (`A.B`, `D.B`).

        // Sort by availability so that direct imports and in-scope names
        // are preferred when de-duping.

        // When tied, prefer the smaller namespace ID, assuming that that's the "original" namespace,
        // but really that's just a heuristic when we don't have much else to go on.
        candidates.sort_by_key(|i| {
            (
                i.item_id,
                i.name.clone(),
                i.availability.clone(),
                i.namespace_id,
            )
        });

        candidates.dedup_by_key(|i| (i.item_id, i.name.clone()));

        // Drop any items that were direct local imports, since the locals completion module
        // would have already included them in the ultimate completion list
        candidates.retain(|item| !matches!(item.availability, Availability::Local));

        self.to_completions(candidates, edit_range)
    }

    /// Turns the final list of items into completion items, filling in
    /// details and including any text edits if requested.
    ///
    /// Items are sorted by package, with the user package (`None`)
    /// coming first, with the dependencies in reverse order, so
    /// that the "closer" dependencies are listed first in the completion list.
    fn to_completions(
        &self,
        mut items: Vec<ItemInfo<'_>>,
        edit_range: Option<&TextEditRange>,
    ) -> Vec<Vec<Completion>> {
        items.sort_by(|a, b| match (a.item_id.package, b.item_id.package) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (a, b) => a.cmp(&b),
        });
        items.reverse();

        let mut groups = Vec::new();
        let mut group = Vec::new();
        let mut last_package_id = items.first().and_then(|item| item.item_id.package);

        for item in items {
            let curr_package_id = item.item_id.package;
            if curr_package_id != last_package_id {
                // push the group to the groups
                if !group.is_empty() {
                    groups.push(take(&mut group));
                }
            }
            let completion = self.to_completion(&item, edit_range);

            group.push(completion);
            last_package_id = curr_package_id;
        }

        if !group.is_empty() {
            groups.push(take(&mut group));
        }

        groups
    }

    /// Creates a completion list entry for the given item, including
    /// any text edits that would bring the item into scope, if requested.
    fn to_completion(&self, item: &ItemInfo<'a>, text_edits: Option<&TextEditRange>) -> Completion {
        let display = CodeDisplay {
            compilation: self.compilation,
        };
        let (kind, display) = match &item.decl {
            ItemDecl::Callable(callable_decl) => (
                CompletionItemKind::Function,
                display.hir_callable_decl(callable_decl).to_string(),
            ),
            ItemDecl::Udt(udt) => (
                CompletionItemKind::Interface,
                display.hir_udt(udt).to_string(),
            ),
        };

        // Deprioritize names starting with "__" in the completion list
        let mut sort_priority = u32::from(item.name.starts_with("__"));

        match &item.availability {
            Availability::Local | Availability::InScope | Availability::Qualified => {
                Completion::with_text_edits(
                    item.name.to_string(),
                    kind,
                    Some(display),
                    None,
                    sort_priority,
                )
            }
            Availability::NeedImport(namespace) => {
                // Deprioritize auto-import items
                sort_priority += 1;

                let text_edits = text_edits.expect(
                    "a text edit range should have been provided if `in_scope_only` is false",
                );
                // if there is no place to insert an import, then we can't add an import.
                let edits = text_edits.insert_import_at.as_ref().map(|range| {
                    vec![TextEdit {
                        new_text: format!(
                            "import {}.{};{}",
                            self.global_scope().format_namespace_name(*namespace),
                            item.name,
                            &text_edits.indent
                        ),
                        range: *range,
                    }]
                });

                Completion::with_text_edits(
                    item.name.to_string(),
                    kind,
                    Some(display),
                    edits,
                    sort_priority,
                )
            }
            Availability::InAliasedNamespace(prefix) => Completion::with_text_edits(
                format!("{}.{}", prefix, item.name),
                kind,
                Some(display),
                None,
                sort_priority,
            ),
        }
    }

    fn global_scope(&self) -> &qsc::resolve::GlobalScope {
        &self.compilation.user_unit().ast.globals
    }
}

/// The global item that backs a completion item.
struct ItemInfo<'a> {
    item_id: ItemId,
    namespace_id: NamespaceId,
    /// Can be the original name for the decl,
    /// or the alias if this item is found through an import.
    name: Rc<str>,
    decl: ItemDecl<'a>,
    availability: Availability,
}

/// The declaration, used to format completion item details.
#[derive(Debug)]
enum ItemDecl<'a> {
    Callable(&'a CallableDecl),
    Udt(&'a Udt),
}

/// How the item name is available in the current scope.
///
/// The order here is important, as it's used when de-duping.
/// We prefer the first kind of import if there are multiple
/// paths leading to the same item.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
enum Availability {
    /// Available through a direct local import.
    Local,
    /// In scope because of open namespaces.
    /// Safe to include without any additional edits.
    InScope,
    /// In scope because of preceding qualifier.
    /// Safe to include without any additional edits.
    Qualified,
    /// In a namespace that is open with a local alias.
    /// The name should be prefixed with the namespace alias.
    ///
    /// e.g. `Foo` will appear as `Bar.Foo` if it's under an open
    /// namespace that is aliased as `Bar`.
    InAliasedNamespace(Rc<str>),
    /// Not in scope at all. Needs an auto-import entry.
    NeedImport(NamespaceId),
}
