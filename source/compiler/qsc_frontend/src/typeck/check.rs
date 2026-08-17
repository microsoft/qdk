// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    Error, ErrorKind, Table,
    infer::Inferrer,
    rules::{self, SpecImpl},
};
use crate::{
    resolve::{Names, Res},
    typeck::convert::{self},
};
use qsc_ast::{
    ast::{self, NodeId, TopLevelNode},
    visit::{self, Visitor},
};
use qsc_data_structures::index_map::IndexMap;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{self, ItemId, PackageId},
    ty::{FunctorSetValue, Scheme, Ty, Udt, UdtDef, UdtDefKind},
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::vec;

pub(crate) struct GlobalTable {
    udts: FxHashMap<ItemId, Udt>,
    terms: FxHashMap<ItemId, Scheme>,
    errors: Vec<Error>,
}

impl GlobalTable {
    pub(crate) fn new() -> Self {
        Self {
            udts: FxHashMap::default(),
            terms: FxHashMap::default(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn add_external_package(
        &mut self,
        package_id: PackageId,
        package: &hir::Package,
        store: &crate::compile::PackageStore,
    ) {
        for item in package.items.values() {
            self.handle_item(item, package_id, store);
        }
    }

    fn handle_item(
        &mut self,
        item: &hir::Item,
        package_id: PackageId,
        store: &crate::compile::PackageStore,
    ) {
        let item_id = ItemId {
            package: package_id,
            item: item.id,
        };
        match &item.kind {
            hir::ItemKind::Callable(decl) => {
                self.terms
                    .insert(item_id, decl.scheme().with_package(package_id));
            }
            hir::ItemKind::Namespace(..) => (),
            hir::ItemKind::Ty(_, udt) => {
                self.udts.insert(item_id, udt.clone());
                self.terms
                    .insert(item_id, udt.cons_scheme(item_id).with_package(package_id));
            }
            hir::ItemKind::Export(
                _,
                hir::Res::Item(ItemId {
                    package: other_package,
                    item: exported_item,
                }),
            ) => {
                // If this item is an export, then we need to grab the ID that it references.
                // It could be from the same package, or it could be from another package.
                let package_id = other_package;
                // So, we get the correct package first,
                let package = store.get(*package_id).expect("package should exist");
                // find the actual item
                let resolved_export = package
                    .package
                    .items
                    .get(*exported_item)
                    .expect("exported item should exist");
                // and recursively resolve it (it could be another export, i.e. a chain of exports.
                self.handle_item(resolved_export, *package_id, store);
            }
            hir::ItemKind::Export(..) => {
                // An export that failed to resolve, skip it.
            }
        }
    }
}

/// This struct is the entry point of the type checker. Constructed with [`Checker::new`], it
/// exposes a method [`Checker::check_package`] that will type check a given [`ast::Package`] and
/// populate its own fields with the results.
pub(crate) struct Checker {
    globals: FxHashMap<ItemId, Scheme>,
    table: Table,
    inferrer: Inferrer,
    new: Vec<NodeId>,
    errors: Vec<Error>,
}

impl Checker {
    pub(crate) fn new(globals: GlobalTable) -> Self {
        Checker {
            globals: globals.terms,
            table: Table {
                udts: globals.udts,
                terms: IndexMap::new(),
                generics: IndexMap::new(),
            },
            inferrer: Inferrer::new(),
            new: Vec::new(),
            errors: globals.errors,
        }
    }

    pub(crate) fn table(&self) -> &Table {
        &self.table
    }

    pub(crate) fn into_table(self) -> (Table, Vec<Error>) {
        (self.table, self.errors)
    }

    pub(crate) fn drain_errors(&mut self) -> vec::Drain<'_, Error> {
        self.errors.drain(..)
    }

    pub(crate) fn check_package(&mut self, names: &Names, package: &ast::Package) {
        let local_udts = {
            let mut collector = ItemCollector::new(self, names);
            collector.visit_package(package);
            collector.local_udts
        };
        self.check_recursive_udts(&local_udts);
        ItemChecker::new(self, names).visit_package(package);

        if let Some(entry) = &package.entry {
            self.errors.append(&mut rules::expr(
                names,
                &self.globals,
                &mut self.table,
                entry,
            ));
        }

        for top_level_node in &package.nodes {
            if let TopLevelNode::Stmt(stmt) = top_level_node {
                self.new.append(&mut rules::stmt(
                    names,
                    &self.globals,
                    &mut self.table,
                    &mut self.inferrer,
                    stmt,
                ));
            }
        }
    }

    /// Rejects any user-defined type declared in this package that can reach itself through
    /// its own field types. Q# has no indirection primitive, so a cycle through an array or a
    /// callable type is just as unrepresentable as a direct one: downstream passes expand UDT
    /// definitions structurally without a visited set, and a cyclic type has no finite expansion.
    fn check_recursive_udts(&mut self, local_udts: &[ItemId]) {
        let mut errors = Vec::new();
        for &id in local_udts {
            let Some(udt) = self.table.udts.get(&id) else {
                continue;
            };
            if let Some(span) = cycle_span(&self.table.udts, id, &udt.definition) {
                errors.push(Error(ErrorKind::RecursiveUdt {
                    name: udt.name.to_string(),
                    span,
                }));
            }
        }
        self.errors.append(&mut errors);
    }

    fn check_callable_decl(&mut self, names: &Names, decl: &ast::CallableDecl) {
        self.check_callable_signature(names, decl);
        let output = convert::ty_from_ast(names, &decl.output, &mut Default::default()).0;
        match &*decl.body {
            ast::CallableBody::Block(block) => self.check_spec(
                names,
                SpecImpl {
                    spec: ast::Spec::Body,
                    callable_input: &decl.input,
                    spec_input: None,
                    output: &output,
                    output_span: decl.output.span,
                    block,
                },
            ),
            ast::CallableBody::Specs(specs) => {
                for spec in specs {
                    if let ast::SpecBody::Impl(input, block) = &spec.body {
                        self.check_spec(
                            names,
                            SpecImpl {
                                spec: spec.spec,
                                callable_input: &decl.input,
                                spec_input: Some(input),
                                output: &output,
                                output_span: decl.output.span,
                                block,
                            },
                        );
                    }
                }
            }
        }
    }

    fn check_callable_signature(&mut self, names: &Names, decl: &ast::CallableDecl) {
        if convert::ast_callable_functors(decl) != FunctorSetValue::Empty {
            let output = convert::ty_from_ast(names, &decl.output, &mut Default::default()).0;
            match &output {
                Ty::Tuple(items) if items.is_empty() => {}
                _ => self.errors.push(Error(ErrorKind::TyMismatch(
                    Ty::UNIT.into(),
                    output.into(),
                    decl.output.span,
                ))),
            }
        }
    }

    /// Used to check all callable bodies
    /// Note that a regular function block callable body is still checked by
    /// this function
    fn check_spec(&mut self, names: &Names, spec: SpecImpl) {
        self.errors.append(&mut rules::spec(
            names,
            &self.globals,
            &mut self.table,
            spec,
        ));
    }

    pub(crate) fn solve(&mut self, names: &Names) {
        self.errors.append(&mut rules::solve(
            names,
            &self.globals,
            &mut self.table,
            &mut self.inferrer,
            std::mem::take(&mut self.new),
        ));
    }
}

/// Populates `Checker` with definitions and errors, while referring to the `Names` table to get
/// definitions.
struct ItemCollector<'a> {
    checker: &'a mut Checker,
    names: &'a Names,
    /// UDTs declared by the package being collected, in declaration order. Scoped to this
    /// collector so it cannot accumulate across the incremental compiler's repeated
    /// `check_package` calls, and ordered so diagnostics do not depend on `FxHashMap` order.
    local_udts: Vec<ItemId>,
}

impl<'a> ItemCollector<'a> {
    fn new(checker: &'a mut Checker, names: &'a Names) -> Self {
        Self {
            checker,
            names,
            local_udts: Vec::new(),
        }
    }
}

impl Visitor<'_> for ItemCollector<'_> {
    fn visit_item(&mut self, item: &ast::Item) {
        match &*item.kind {
            ast::ItemKind::Callable(decl) => {
                let Some(&Res::Item(item, _)) = self.names.get(decl.name.id) else {
                    panic!("callable should have item ID");
                };

                let (scheme, errors) = convert::scheme_for_ast_callable(self.names, decl);
                for err in errors {
                    self.checker.errors.push(err.into());
                }

                self.checker.globals.insert(item, scheme);
            }
            ast::ItemKind::Ty(name, def) => {
                let span = item.span;
                let Some(&Res::Item(item, _)) = self.names.get(name.id) else {
                    panic!("type should have item ID");
                };

                let (cons, cons_errors) =
                    convert::ast_ty_def_cons(self.names, &name.name, item, def);
                let (udt_def, def_errors) = convert::ast_ty_def(self.names, def);
                self.checker
                    .errors
                    .extend(cons_errors.into_iter().chain(def_errors).map(Into::into));

                self.checker.table.udts.insert(
                    item,
                    Udt {
                        name: name.name.clone(),
                        span,
                        definition: udt_def,
                    },
                );
                self.local_udts.push(item);
                self.checker.globals.insert(item, cons);
            }
            ast::ItemKind::Struct(decl) => {
                let span = item.span;
                let Some(&Res::Item(item, _)) = self.names.get(decl.name.id) else {
                    panic!("type should have item ID");
                };

                let def = convert::ast_struct_decl_as_ty_def(decl);

                let (cons, cons_errors) =
                    convert::ast_ty_def_cons(self.names, &decl.name.name, item, &def);
                let (udt_def, def_errors) = convert::ast_ty_def(self.names, &def);
                self.checker
                    .errors
                    .extend(cons_errors.into_iter().chain(def_errors).map(Into::into));

                self.checker.table.udts.insert(
                    item,
                    Udt {
                        name: decl.name.name.clone(),
                        span,
                        definition: udt_def,
                    },
                );
                self.local_udts.push(item);
                self.checker.globals.insert(item, cons);
            }
            _ => {}
        }

        visit::walk_item(self, item);
    }

    // We do not typecheck attributes, as they are verified during lowering.
    fn visit_attr(&mut self, _: &ast::Attr) {}
}

struct ItemChecker<'a> {
    checker: &'a mut Checker,
    names: &'a Names,
}

impl<'a> ItemChecker<'a> {
    fn new(checker: &'a mut Checker, names: &'a Names) -> Self {
        Self { checker, names }
    }
}

impl Visitor<'_> for ItemChecker<'_> {
    fn visit_callable_decl(&mut self, decl: &ast::CallableDecl) {
        self.checker.check_callable_decl(self.names, decl);
        visit::walk_callable_decl(self, decl);
    }

    // We do not typecheck attributes, as they are verified during lowering.
    fn visit_attr(&mut self, _: &ast::Attr) {}
}

/// Finds the innermost field definition within `def` whose type can reach `target`, and returns
/// its span. Returns `None` when no field closes a cycle back to `target`. Since every outgoing
/// edge in the UDT graph originates at a leaf field, this is exactly the "is `target` recursive"
/// predicate as well as the label selector.
fn cycle_span(udts: &FxHashMap<ItemId, Udt>, target: ItemId, def: &UdtDef) -> Option<Span> {
    match &def.kind {
        UdtDefKind::Field(field) => {
            ty_reaches(udts, target, &field.ty, &mut FxHashSet::default()).then_some(def.span)
        }
        UdtDefKind::Tuple(defs) => defs.iter().find_map(|def| cycle_span(udts, target, def)),
    }
}

/// Whether `target` is reachable from `ty` by descending through arrays, tuples, callable
/// signatures, and the definitions of the UDTs encountered along the way. `visited` bounds the
/// walk so it terminates on cyclic input.
fn ty_reaches(
    udts: &FxHashMap<ItemId, Udt>,
    target: ItemId,
    ty: &Ty,
    visited: &mut FxHashSet<ItemId>,
) -> bool {
    match ty {
        Ty::Udt(_, hir::Res::Item(id)) => {
            if *id == target {
                return true;
            }
            if !visited.insert(*id) {
                return false;
            }
            // A missing entry means name resolution already failed; a second error would be noise.
            udts.get(id)
                .is_some_and(|udt| def_reaches(udts, target, &udt.definition, visited))
        }
        Ty::Array(item) => ty_reaches(udts, target, item, visited),
        Ty::Tuple(items) => items
            .iter()
            .any(|item| ty_reaches(udts, target, item, visited)),
        Ty::Arrow(arrow) => {
            ty_reaches(udts, target, &arrow.input.borrow(), visited)
                || ty_reaches(udts, target, &arrow.output.borrow(), visited)
        }
        _ => false,
    }
}

fn def_reaches(
    udts: &FxHashMap<ItemId, Udt>,
    target: ItemId,
    def: &UdtDef,
    visited: &mut FxHashSet<ItemId>,
) -> bool {
    match &def.kind {
        UdtDefKind::Field(field) => ty_reaches(udts, target, &field.ty, visited),
        UdtDefKind::Tuple(defs) => defs
            .iter()
            .any(|def| def_reaches(udts, target, def, visited)),
    }
}
