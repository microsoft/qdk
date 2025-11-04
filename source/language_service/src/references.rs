// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::rc::Rc;

use crate::compilation::{Compilation, CompilationKind, source_position_to_package_offset};
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::qsc_utils::into_location;
use qsc::ast::PathKind;
use qsc::ast::visit::{Visitor, walk_callable_decl, walk_expr, walk_import_or_export, walk_ty};
use qsc::display::Lookup;
use qsc::hir::ty::Ty;
use qsc::hir::{PackageId, Res};
use qsc::line_column::{Encoding, Position};
use qsc::location::Location;
use qsc::{Span, ast, hir, resolve};

pub(crate) fn get_references(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
    include_declaration: bool,
) -> Vec<Location> {
    if let CompilationKind::OpenQASM { sources, .. } = &compilation.kind {
        return crate::openqasm::get_references(sources, source_name, position, position_encoding);
    }
    let unit = &compilation.user_unit();
    let offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);
    let user_ast_package = &unit.ast.package;

    let mut name_handler = NameHandler {
        reference_finder: ReferenceFinder::new(position_encoding, compilation, include_declaration),
        references: vec![],
    };

    let mut locator = Locator::new(&mut name_handler, offset, compilation);
    locator.visit_package(user_ast_package);

    name_handler.references
}

pub(crate) struct ReferenceFinder<'a> {
    position_encoding: Encoding,
    compilation: &'a Compilation,
    include_declaration: bool,
}

struct NameHandler<'a> {
    reference_finder: ReferenceFinder<'a>,
    references: Vec<Location>,
}

impl<'a> Handler<'a> for NameHandler<'a> {
    fn at_attr_ref(&mut self, _: &'a ast::Ident) {
        // We don't support find all refs for attributes.
    }

    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) =
            self.reference_finder.compilation.get_res(name.id)
        {
            self.references = self.reference_finder.for_item(item_id, None);
        }
    }

    fn at_callable_ref(
        &mut self,
        _: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        _: &'a hir::CallableDecl,
    ) {
        self.references = self.reference_finder.for_item(item_id, None);
    }

    fn at_type_param_def(
        &mut self,
        context: &LocatorContext<'a>,
        _: &'a ast::Ident,
        param_id: hir::ty::ParamId,
    ) {
        if let Some(curr) = context.current_callable {
            self.references = self.reference_finder.for_ty_param(param_id, curr);
        }
    }

    fn at_type_param_ref(
        &mut self,
        context: &LocatorContext<'a>,
        _: &'a ast::Ident,
        param_id: hir::ty::ParamId,
        _: &'a ast::Ident,
    ) {
        if let Some(curr) = context.current_callable {
            self.references = self.reference_finder.for_ty_param(param_id, curr);
        }
    }

    fn at_new_type_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::TyDef,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) =
            self.reference_finder.compilation.get_res(type_name.id)
        {
            self.references = self.reference_finder.for_item(item_id, None);
        }
    }

    fn at_struct_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::StructDecl,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) =
            self.reference_finder.compilation.get_res(type_name.id)
        {
            self.references = self.reference_finder.for_item(item_id, None);
        }
    }

    fn at_new_type_ref(
        &mut self,
        _: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        _: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.references = self.reference_finder.for_item(item_id, None);
    }

    fn at_field_def(
        &mut self,
        context: &LocatorContext<'a>,
        field_name: &ast::Ident,
        _: &'a ast::Ty,
    ) {
        if let Some(ty_item_id) = context.current_udt_id {
            self.references = self
                .reference_finder
                .for_field(ty_item_id, field_name.name.clone());
        }
    }

    fn at_field_ref(
        &mut self,
        field_ref: &ast::Ident,
        item_id: &hir::ItemId,
        _: &'a hir::ty::UdtField,
    ) {
        self.references = self
            .reference_finder
            .for_field(item_id, field_ref.name.clone());
    }

    fn at_local_def(
        &mut self,
        context: &LocatorContext<'a>,
        ident: &'a ast::Ident,
        _: &'a ast::Pat,
    ) {
        self.references = self
            .reference_finder
            .for_local(ident.id, context.current_callable);
    }

    fn at_local_ref(
        &mut self,
        context: &LocatorContext<'a>,
        _: &ast::Ident,
        _: ast::NodeId,
        definition: &'a ast::Ident,
    ) {
        self.references = self
            .reference_finder
            .for_local(definition.id, context.current_callable);
    }
}

impl<'a> ReferenceFinder<'a> {
    pub fn new(
        position_encoding: Encoding,
        compilation: &'a Compilation,
        include_declaration: bool,
    ) -> Self {
        Self {
            position_encoding,
            compilation,
            include_declaration,
        }
    }

    pub fn for_item(&self, item_id: &hir::ItemId, name_filter: Option<&Rc<str>>) -> Vec<Location> {
        let mut locations = vec![];

        let (def, _, resolved_item_id) = self
            .compilation
            .resolve_item_relative_to_user_package(item_id);
        if self.include_declaration {
            let (decl_name, decl_span) = match &def.kind {
                hir::ItemKind::Callable(decl) => (Some(&decl.name.name), decl.name.span),
                hir::ItemKind::Namespace(name, _) => (None, name.span()),
                hir::ItemKind::Ty(name, _) | hir::ItemKind::Export(name, _) => {
                    (Some(&name.name), name.span)
                }
            };

            if name_filter.is_none() || decl_name == name_filter {
                locations.push(
                    self.location(
                        decl_span,
                        resolved_item_id
                            .package
                            .expect("package id should have been resolved"),
                    ),
                );
            }
        }

        let mut find_refs = FindItemRefs {
            item_id: &resolved_item_id,
            compilation: self.compilation,
            name_filter,
            locations: vec![],
        };

        find_refs.visit_package(&self.compilation.user_unit().ast.package);
        locations.extend(
            find_refs
                .locations
                .drain(..)
                .map(|l| self.location(l, self.compilation.user_package_id)),
        );

        locations
    }

    pub fn for_field(&self, ty_item_id: &hir::ItemId, field_name: Rc<str>) -> Vec<Location> {
        let mut locations = vec![];

        let (ty_def, _, resolved_ty_item_id) = self
            .compilation
            .resolve_item_relative_to_user_package(ty_item_id);
        if self.include_declaration {
            if let hir::ItemKind::Ty(_, udt) = &ty_def.kind {
                let ty_field = udt
                    .find_field_by_name(&field_name)
                    .expect("field name should exist");
                let def_span = ty_field
                    .name_span
                    .expect("field found via name should have a name");
                locations.push(
                    self.location(
                        def_span,
                        resolved_ty_item_id
                            .package
                            .expect("package id should have been resolved"),
                    ),
                );
            } else {
                panic!("item id resolved to non-type: {ty_item_id}");
            }
        }

        let mut find_refs = FindFieldRefs {
            ty_item_id: &resolved_ty_item_id,
            field_name,
            compilation: self.compilation,
            locations: vec![],
        };

        find_refs.visit_package(&self.compilation.user_unit().ast.package);
        locations.extend(
            find_refs
                .locations
                .drain(..)
                .map(|l| self.location(l, self.compilation.user_package_id)),
        );

        locations
    }

    pub fn for_local(
        &self,
        node_id: ast::NodeId,
        callable: Option<&ast::CallableDecl>,
    ) -> Vec<Location> {
        let mut find_refs = FindLocalLocations {
            node_id,
            compilation: self.compilation,
            include_declaration: self.include_declaration,
            locations: vec![],
        };
        if let Some(callable) = callable {
            find_refs.visit_callable_decl(callable);
        } else {
            find_refs.visit_package(&self.compilation.user_unit().ast.package);
        }
        find_refs
            .locations
            .into_iter()
            .map(|l| self.location(l, self.compilation.user_package_id))
            .collect()
    }

    pub fn for_ty_param(
        &self,
        param_id: hir::ty::ParamId,
        callable: &ast::CallableDecl,
    ) -> Vec<Location> {
        let mut find_refs = FindTyParamLocations {
            param_id,
            compilation: self.compilation,
            include_declaration: self.include_declaration,
            locations: vec![],
        };
        find_refs.visit_callable_decl(callable);
        find_refs
            .locations
            .into_iter()
            .map(|l| self.location(l, self.compilation.user_package_id))
            .collect()
    }

    fn location(&self, location: Span, package_id: PackageId) -> Location {
        into_location(
            self.position_encoding,
            self.compilation,
            location,
            package_id,
        )
    }
}

struct FindItemRefs<'a> {
    item_id: &'a hir::ItemId,
    compilation: &'a Compilation,
    name_filter: Option<&'a Rc<str>>,
    locations: Vec<Span>,
}

impl Visitor<'_> for FindItemRefs<'_> {
    fn visit_path(&mut self, path: &ast::Path) {
        let res = self.compilation.get_res(path.id);
        if let Some(res) = res
            && let Some(ref item_id) = res.item_id()
            && self.eq(item_id)
            && (self.name_filter.is_none() || Some(&path.name.name) == self.name_filter)
        {
            self.locations.push(path.name.span);
        }
    }

    fn visit_import_or_export(&mut self, item: &'_ ast::ImportOrExportItem) {
        if let ast::ImportKind::Direct { alias: Some(alias) } = &item.kind
            && let PathKind::Ok(path) = &item.path
        {
            let res = self.compilation.get_res(path.id);
            if let Some(res) = res
                && let Some(item_id) = res.item_id()
                && self.eq(&item_id)
                && (self.name_filter.is_none() || Some(&alias.name) == self.name_filter)
            {
                self.locations.push(alias.span);
            }
        }
        walk_import_or_export(self, item);
    }
}

impl FindItemRefs<'_> {
    fn eq(&mut self, item_id: &hir::ItemId) -> bool {
        item_id.item == self.item_id.item
            && item_id.package.unwrap_or(self.compilation.user_package_id)
                == self.item_id.package.expect("package id should be resolved")
    }
}

struct FindFieldRefs<'a> {
    ty_item_id: &'a hir::ItemId,
    field_name: Rc<str>,
    compilation: &'a Compilation,
    locations: Vec<Span>,
}

impl Visitor<'_> for FindFieldRefs<'_> {
    fn visit_path(&mut self, path: &ast::Path) {
        if let Some((_, parts)) =
            resolve::path_as_field_accessor(&self.compilation.user_unit().ast.names, path)
        {
            let (first, rest) = parts
                .split_first()
                .expect("paths should have at least one part");
            let mut prev_id = first.id;
            // Loop through the parts of the path to find references
            for part in rest {
                if part.name == self.field_name
                    && let Some(Ty::Udt(_, Res::Item(id))) = self.compilation.get_ty(prev_id)
                    && self.eq(id)
                {
                    self.locations.push(part.span);
                }
                prev_id = part.id;
            }
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match &*expr.kind {
            ast::ExprKind::Field(qualifier, ast::FieldAccess::Ok(field_name)) => {
                self.visit_expr(qualifier);
                if field_name.name == self.field_name
                    && let Some(Ty::Udt(_, Res::Item(id))) = self.compilation.get_ty(qualifier.id)
                    && self.eq(id)
                {
                    self.locations.push(field_name.span);
                }
            }
            ast::ExprKind::Struct(PathKind::Ok(struct_name), copy, fields) => {
                self.visit_path(struct_name);
                if let Some(copy) = copy {
                    self.visit_expr(copy);
                }
                for field in fields {
                    if field.field.name == self.field_name
                        && let Some(Ty::Udt(_, Res::Item(id))) = self.compilation.get_ty(expr.id)
                        && self.eq(id)
                    {
                        self.locations.push(field.field.span);
                    }
                    self.visit_expr(&field.value);
                }
            }
            _ => walk_expr(self, expr),
        }
    }
}

impl FindFieldRefs<'_> {
    fn eq(&mut self, item_id: &hir::ItemId) -> bool {
        item_id.item == self.ty_item_id.item
            && item_id.package.unwrap_or(self.compilation.user_package_id)
                == self
                    .ty_item_id
                    .package
                    .expect("package id should be resolved")
    }
}

struct FindLocalLocations<'a> {
    node_id: ast::NodeId,
    compilation: &'a Compilation,
    include_declaration: bool,
    locations: Vec<Span>,
}

impl Visitor<'_> for FindLocalLocations<'_> {
    // Locals don't cross namespace boundaries, so don't visit namespaces.
    fn visit_package(&mut self, package: &ast::Package) {
        package.nodes.iter().for_each(|n| {
            if let ast::TopLevelNode::Stmt(stmt) = n {
                self.visit_stmt(stmt);
            }
        });
        package.entry.iter().for_each(|e| self.visit_expr(e));
    }

    // Locals don't cross item boundaries, so don't visit items.
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match &*stmt.kind {
            ast::StmtKind::Item(_) => {}
            _ => ast::visit::walk_stmt(self, stmt),
        }
    }

    fn visit_pat(&mut self, pat: &ast::Pat) {
        if self.include_declaration {
            match &*pat.kind {
                ast::PatKind::Bind(ident, _) => {
                    if ident.id == self.node_id {
                        self.locations.push(ident.span);
                    }
                }
                _ => ast::visit::walk_pat(self, pat),
            }
        }
    }

    fn visit_path(&mut self, path: &ast::Path) {
        match resolve::path_as_field_accessor(&self.compilation.user_unit().ast.names, path) {
            Some((node_id, parts)) => {
                let first = parts.first().expect("paths should have at least one part");
                if node_id == self.node_id {
                    self.locations.push(first.span);
                }
            }
            None => {
                if let Some(resolve::Res::Local(node_id)) = self.compilation.get_res(path.id)
                    && *node_id == self.node_id
                {
                    self.locations.push(path.name.span);
                }
            }
        }
    }
}

struct FindTyParamLocations<'a> {
    param_id: hir::ty::ParamId,
    compilation: &'a Compilation,
    include_declaration: bool,
    locations: Vec<Span>,
}

impl Visitor<'_> for FindTyParamLocations<'_> {
    fn visit_callable_decl(&mut self, decl: &ast::CallableDecl) {
        if self.include_declaration {
            decl.generics.iter().for_each(|p| {
                let res = self.compilation.get_res(p.ty.id);
                if let Some(resolve::Res::Param { id, .. }) = res
                    && *id == self.param_id
                {
                    self.locations.push(p.ty.span);
                }
            });
        }
        walk_callable_decl(self, decl);
    }

    fn visit_ty(&mut self, ty: &ast::Ty) {
        if let ast::TyKind::Param(param) = &*ty.kind {
            let res = self.compilation.get_res(param.ty.id);
            if let Some(resolve::Res::Param { id, .. }) = res
                && *id == self.param_id
            {
                self.locations.push(param.span);
            }
        } else {
            walk_ty(self, ty);
        }
    }
}
