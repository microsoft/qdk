// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Error, Importable, NameKind, Res, Resolver, ScopeKind};
use crate::compile::preprocess::TrackedName;
use qsc_ast::ast::{
    Block, Ident, Idents as _, ImportKind, ImportOrExportDecl, Item, ItemKind, NodeId, Package,
    Path, PathKind, StmtKind, TopLevelNode,
};
use qsc_data_structures::{namespaces::NamespaceId, span::Span};
use rustc_hash::FxHashMap;
use std::{collections::hash_map::Entry, rc::Rc};

const MAX_ITERATIONS: usize = 100;

/// Resolves all imports and exports declared in namespace scopes in the package.
/// Exports are then made available in the global scope, whereas imports are made
/// available in the namespace scope they are declared in.
pub(super) fn resolve_all_namespace_imports_and_exports(
    resolver: &mut Resolver,
    package: &Package,
) {
    let errors = iterate_until_done(|attempted_imports| {
        let mut new_imports_were_added = false;

        for node in &package.nodes {
            if let TopLevelNode::Namespace(namespace) = node {
                let namespace_id = resolver
                    .globals
                    .find_namespace(namespace.name.str_iter(), None)
                    .expect("expected to find namespace"); // namespace name should have been added in the name binding pass

                resolver.push_scope(namespace.span, ScopeKind::Namespace(namespace_id));

                resolver.resolve_and_add_open(&namespace.name, None);

                if try_resolve_imports_and_exports(
                    resolver,
                    || namespace.items.iter().map(AsRef::as_ref),
                    attempted_imports,
                ) {
                    new_imports_were_added = true;
                }

                resolver.pop_scope();
            }
        }

        new_imports_were_added
    });
    resolver.errors.extend(errors);
}

/// Resolve all imports declared in top-level statements.
/// Imports are then made available in the persistent local scope.
pub(super) fn resolve_top_level_imports(resolver: &mut Resolver, package: &Package) {
    let errors = iterate_until_done(|attempted_imports| {
        try_resolve_imports_and_exports(
            resolver,
            || {
                package.nodes.iter().filter_map(|node| {
                    if let TopLevelNode::Stmt(stmt) = node
                        && let StmtKind::Item(item) = &*stmt.kind
                    {
                        return Some(item.as_ref());
                    }
                    None
                })
            },
            attempted_imports,
        )
    });
    resolver.errors.extend(errors);
}

/// Resolve all imports declared in a block.
/// Imports are then made available in that block's scope.
pub(super) fn resolve_block_imports(resolver: &mut Resolver, block: &Block) {
    let errors = iterate_until_done(|attempted_imports| {
        try_resolve_imports_and_exports(
            resolver,
            || {
                block.stmts.iter().filter_map(|stmt| {
                    if let StmtKind::Item(item) = &*stmt.kind {
                        Some(item.as_ref())
                    } else {
                        None
                    }
                })
            },
            attempted_imports,
        )
    });
    resolver.errors.extend(errors);
}

/// Makes a single pass over the imports and exports in the current scope,
/// attempting to resolve them.
///
/// Once an import has been successfully resolved, its name is made available
/// in the current scope.
///
/// Exports are only handled if the current scope is a namespace, and
/// once successfully resolved, their names will be added to the global scope.
///
/// Note that this single pass may not result in all imports being resolved.
/// Multiple passes will ensure that any imports referencing other imports
/// in the same scope are resolved.
///
/// For this reason, resolution errors are not reported immediately, but
/// instead stored in `attempted_imports` to be retried later. Returns
/// `true` if any new imports or exports were bound in the current scope,
/// as a signal that we should continue iterating.
fn try_resolve_imports_and_exports<'a, I>(
    resolver: &mut Resolver,
    scope_items_iter: impl Fn() -> I,
    attempted_imports: &mut FxHashMap<NodeId, Result<(), Error>>,
) -> bool
where
    I: Iterator<Item = &'a Item>,
{
    if scope_items_iter().count() == 0 {
        // If there are no items in this scope, short-circuit.
        return false;
    }

    let mut new_imports_added = false;

    // Start by adding the opens and wildcard imports in this scope

    // Handle all opens first
    for item in scope_items_iter() {
        if let ItemKind::Open(PathKind::Ok(path), alias) = &*item.kind {
            resolver.resolve_and_add_open(path.as_ref(), alias.as_deref());
        }
    }

    // Wildcard imports are treated as opens, handle them too
    for item in scope_items_iter() {
        if let ItemKind::ImportOrExport(decl) = &*item.kind {
            for item in iter_valid_items(decl) {
                if let ImportKind::Wildcard = item.kind {
                    resolver.resolve_and_add_open(item.path, None);
                }
            }
        }
    }

    // Now, attempt to resolve all imports, and exports if we're in a namespace scope
    for item in scope_items_iter() {
        if let ItemKind::ImportOrExport(decl) = &*item.kind
            && try_resolve_import_or_export_decl(resolver, decl, attempted_imports)
        {
            new_imports_added = true;
        }
    }

    new_imports_added
}

/// Returns `true` if this call caused new imports to become available in the scope.
/// This is used to decide whether we should keep iterating and resolving imports.
fn try_resolve_import_or_export_decl(
    resolver: &mut Resolver,
    decl: &ImportOrExportDecl,
    attempted_imports: &mut FxHashMap<NodeId, Result<(), Error>>,
) -> bool {
    // The current namespace, if we're in a namespace scope.
    let current_namespace = {
        let current_scope = resolver.current_scope_mut();
        if let ScopeKind::Namespace(ns_id) = current_scope.kind {
            Some(ns_id)
        } else {
            None
        }
    };

    // If we're not in a namespace scope, exports are not allowed.
    if decl.is_export() && current_namespace.is_none() {
        resolver.errors.push(Error::ExportFromLocalScope(decl.span));
        return false;
    }

    let mut any_names_were_added = false;
    for valid_item in iter_valid_items(decl) {
        match &valid_item.kind {
            ImportKind::Wildcard => {
                // We handled wildcard imports already as open statements
            }
            ImportKind::Direct { .. } => {
                if attempted_imports
                    .get(&valid_item.name().id)
                    .is_some_and(Result::is_ok)
                {
                    // If this item has already been bound, skip it.
                    continue;
                }

                // filter out any dropped names
                // this is so you can still export an item that has been conditionally removed from compilation
                // without a resolution error in the export statement itself
                // This is not a perfect solution, re-exporting an aliased name from another namespace that has been
                // conditionally compiled out will still fail. However, this is the only way to solve this
                // problem without upleveling the preprocessor into the resolver, so it can do resolution-aware
                // dropped_names population.
                if valid_item.is_export
                    && let Some(current_namespace) = &current_namespace
                {
                    let current_namespace_name =
                        resolver.globals.format_namespace_name(*current_namespace);
                    if resolver.dropped_names.contains(&TrackedName {
                        name: valid_item.path.name.name.clone(),
                        namespace: Rc::from(current_namespace_name),
                    }) {
                        continue;
                    }
                }

                let (resolution_result, name_was_added) =
                    try_resolve_import_or_export(resolver, current_namespace, &valid_item);

                attempted_imports.insert(valid_item.name().id, resolution_result);

                if name_was_added {
                    any_names_were_added = true;
                }
            }
        }
    }
    any_names_were_added
}

/// Returns the resolution result, and whether the import was successfully
/// resolved *and* bound.
fn try_resolve_import_or_export(
    resolver: &mut Resolver,
    current_namespace: Option<NamespaceId>,
    valid_item: &ValidImportOrExportItem<'_>,
) -> (Result<(), Error>, bool) {
    match resolver.resolve_path(NameKind::Importable, valid_item.path) {
        Ok(Res::Importable(ref imported_item_kind)) => {
            // Successfully resolved. Proceed with binding the import/export name
            // to the original item.
            let bind_result = if valid_item.is_export {
                // If this an export, we bind the name in the global scope.
                bind_export(
                    resolver,
                    valid_item.name(),
                    imported_item_kind,
                    current_namespace.expect("current namespace should be set for exports"),
                )
            } else {
                // If this is an import, we bind in the current scope (block or namespace)
                bind_import(resolver, valid_item, imported_item_kind, current_namespace)
            };

            let new_import_was_added = bind_result.is_ok();

            if let Err(err) = bind_result {
                // Report name binding errors straight away, since
                // unlike resolution errors, these are not retried.
                resolver.errors.push(err);
            }

            (Ok(()), new_import_was_added)
        }
        Err(err) => (Err(err), false),
        Ok(res) => {
            unreachable!("unexpected resolution kind for importable: {res:?}",);
        }
    }
}

/// Binds a successfully resolved export's name in the global scope.
fn bind_export(
    resolver: &mut Resolver,
    name: &Ident,
    imported_item: &Importable,
    namespace: NamespaceId,
) -> Result<(), Error> {
    let global_scope = &mut resolver.globals;

    // Add the name as an importable in the global scope
    match global_scope
        .importables
        .get_mut_or_default(namespace)
        .entry(Rc::clone(&name.name))
    {
        Entry::Vacant(entry) => {
            entry.insert(Res::Importable(*imported_item));
        }
        Entry::Occupied(existing) => {
            if let Importable::Callable(imported_item_id, _) | Importable::Ty(imported_item_id, _) =
                imported_item
                && existing.get().item_id() == Some(*imported_item_id)
            {
                // This is just a self-export, e.g.
                //
                // struct Foo {}
                // export Foo;
                //
                // This is not considered a duplicate name.
                // We won't bind the name here since it's already
                // bound to the original item. But we do
                // still need to check for duplicate self-exports.
                if let Some(existing_span) = global_scope
                    .self_exported_item_ids
                    .insert(*imported_item_id, name.span)
                {
                    return Err(Error::DuplicateExport {
                        name: name.name.to_string(),
                        span: name.span,
                        existing_span,
                    });
                }
                return Ok(());
            }

            return Err(Error::Duplicate(
                name.name.to_string(),
                global_scope.format_namespace_name(namespace),
                name.span,
            ));
        }
    }

    // Also make the name available in the proper lookup tables
    // so it can be used during name resolution in expressions, etc.
    match *imported_item {
        Importable::Callable(imported_item_id, status) => {
            global_scope
                .terms
                .get_mut_or_default(namespace)
                .insert(Rc::clone(&name.name), Res::Item(imported_item_id, status));
        }
        Importable::Ty(imported_item_id, status) => {
            let res = Res::Item(imported_item_id, status);
            global_scope
                .terms
                .get_mut_or_default(namespace)
                .insert(Rc::clone(&name.name), res.clone());
            global_scope
                .tys
                .get_mut_or_default(namespace)
                .insert(Rc::clone(&name.name), res);
        }
        Importable::Namespace(original_namespace_id, _) => {
            global_scope.insert_alias_for_namespace(original_namespace_id, &name.name, namespace);
        }
    }
    Ok(())
}

fn bind_import(
    resolver: &mut Resolver,
    valid_item: &ValidImportOrExportItem<'_>,
    imported_item: &Importable,
    current_namespace: Option<NamespaceId>,
) -> Result<(), Error> {
    let name = valid_item.name();

    if let Some(current_namespace) = current_namespace {
        // Even though imports aren't bound as globals
        // (they're only bound in the current scope),
        // if we're in a namespace, still check for collisions
        // with the global scope. e.g.
        //
        // namespace A {
        //    operation Foo() : Unit {}
        //    import Bar as Foo; // not allowed to shadow `Foo`
        // }
        if resolver
            .globals
            .importables
            .get_mut_or_default(current_namespace)
            .contains_key(&name.name)
        {
            return Err(Error::Duplicate(
                name.name.to_string(),
                resolver.globals.format_namespace_name(current_namespace),
                name.span,
            ));
        }
    }

    let scope = resolver.current_scope_mut();
    match (
        scope.importables.entry(name.name.clone()),
        current_namespace,
    ) {
        (Entry::Vacant(entry), _) => {
            entry.insert(Res::Importable(*imported_item));
        }
        (Entry::Occupied(mut entry), None) => {
            // allow shadowing in non-namespace (block) scopes
            entry.insert(Res::Importable(*imported_item));
        }
        (Entry::Occupied(_), Some(namespace_id)) => {
            // collision within the namespace scope
            return Err(Error::Duplicate(
                name.name.to_string(),
                resolver.globals.format_namespace_name(namespace_id),
                name.span,
            ));
        }
    }

    match *imported_item {
        Importable::Callable(imported_item_id, status) => {
            scope
                .terms
                .insert(name.name.clone(), Res::Item(imported_item_id, status));
        }
        Importable::Ty(imported_item_id, status) => {
            scope
                .terms
                .insert(name.name.clone(), Res::Item(imported_item_id, status));
            scope
                .tys
                .insert(name.name.clone(), Res::Item(imported_item_id, status));
        }
        Importable::Namespace(namespace_id, _) => {
            // A direct import of a namespace is the same as an open with an alias
            resolver.bind_open(
                valid_item.path,
                namespace_id,
                Some(valid_item.name().name.clone()),
            );
        }
    }
    Ok(())
}

/// Iterative resolution wrapper that handles the common pattern of
/// running resolution attempts in a loop until convergence or maximum iterations.
///
/// The iterative approach allows imports and exports to refer to each other
/// regardless of the order that they are declared in the source code.
///
/// Consider this example:
/// ```qsharp
/// namespace A {
///     function B() : Unit {}
/// }
///
/// namespace D {
///     export C.B;  // Depends on C.B being available
/// }
///
/// namespace C {
///     export A.B;  // Makes A.B available as C.B
/// }
/// ```
///
/// Here, `D.B` depends on `C.B`, which depends on `A.B`.
/// The iterative approach resolves these dependencies in multiple passes.
#[must_use]
fn iterate_until_done<F>(mut resolver_fn: F) -> Vec<Error>
where
    F: FnMut(&mut FxHashMap<NodeId, Result<(), Error>>) -> bool,
{
    let mut errors = Vec::new();
    let mut attempted_items = FxHashMap::default();
    for i in 1..=MAX_ITERATIONS {
        if !resolver_fn(&mut attempted_items) {
            // If no new imports were made available in this pass, we can stop.
            break;
        }

        if i >= MAX_ITERATIONS {
            errors.push(Error::ImportResolutionLimitExceeded(i));
            return errors;
        }
    }

    for (_, result) in attempted_items.drain() {
        if let Err(err) = result {
            errors.push(err);
        }
    }

    errors
}

/// Iterates over the items in an import or export declaration, yielding only those
/// that were parsed successfully.
pub fn iter_valid_items(
    decl: &ImportOrExportDecl,
) -> impl Iterator<Item = ValidImportOrExportItem<'_>> + '_ {
    decl.items.iter().filter_map(move |item| {
        if let PathKind::Ok(path) = &item.path {
            Some(ValidImportOrExportItem {
                span: item.span,
                path,
                kind: &item.kind,
                is_export: decl.is_export(),
            })
        } else {
            None
        }
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidImportOrExportItem<'a> {
    pub span: Span,
    pub path: &'a Path,
    pub kind: &'a ImportKind,
    pub is_export: bool,
}

impl ValidImportOrExportItem<'_> {
    pub fn name(&self) -> &Ident {
        let alias = match &self.kind {
            ImportKind::Wildcard => None,
            ImportKind::Direct { alias, .. } => alias.as_ref(),
        };

        match alias {
            Some(alias) => alias,
            None => &self.path.name,
        }
    }
}
