// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod openqasm_tests;

use crate::compilation::{Compilation, CompilationKind, source_position_to_package_offset};
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::qsc_utils::into_range;
use crate::references::ReferenceFinder;
use qsc::ast::visit::Visitor;
use qsc::display::Lookup;
use qsc::line_column::{Encoding, Position, Range};
use qsc::location::Location;
use qsc::{Span, ast, hir, resolve};

pub(crate) fn prepare_rename(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<(Range, String)> {
    if let CompilationKind::OpenQASM { sources, .. } = &compilation.kind {
        return crate::openqasm::prepare_rename(sources, source_name, position, position_encoding);
    }
    let unit = &compilation.user_unit();
    let offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);
    let user_ast_package = &unit.ast.package;

    let mut prepare_rename = Rename::new(position_encoding, compilation, true);
    let mut locator = Locator::new(&mut prepare_rename, offset, compilation);
    locator.visit_package(user_ast_package);
    prepare_rename.prepare.map(|p| {
        (
            into_range(position_encoding, p.0, &compilation.user_unit().sources),
            p.1,
        )
    })
}

pub(crate) fn get_rename(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Vec<Location> {
    if let CompilationKind::OpenQASM { sources, .. } = &compilation.kind {
        return crate::openqasm::get_rename(sources, source_name, position, position_encoding);
    }
    let unit = &compilation.user_unit();
    let offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);
    let user_ast_package = &unit.ast.package;

    let mut rename = Rename::new(position_encoding, compilation, false);
    let mut locator = Locator::new(&mut rename, offset, compilation);
    locator.visit_package(user_ast_package);
    rename.locations
}

fn remove_leading_quote_from_type_param_span(span: Span) -> Span {
    // The name includes the leading single quote character, which we don't want as part of the rename.
    assert!(span.hi - span.lo > 1, "Type parameter name is empty");
    Span {
        lo: span.lo + 1, // skip the leading single quote
        ..span
    }
}

fn remove_leading_quote_from_type_param_name(name: &str) -> String {
    // The name includes the leading single quote character, which we don't want as part of the rename.
    assert!(name.len() > 1, "Type parameter name is empty");
    name[1..].to_string()
}

struct Rename<'a> {
    reference_finder: ReferenceFinder<'a>,
    compilation: &'a Compilation,
    locations: Vec<Location>,
    is_prepare: bool,
    prepare: Option<(Span, String)>,
}

impl<'a> Rename<'a> {
    fn new(position_encoding: Encoding, compilation: &'a Compilation, is_prepare: bool) -> Self {
        Self {
            reference_finder: ReferenceFinder::new(position_encoding, compilation, true),
            compilation,
            locations: vec![],
            is_prepare,
            prepare: None,
        }
    }

    fn get_spans_for_item_rename(&mut self, item_id: &hir::ItemId, ast_name: &ast::Ident) {
        let package_id = item_id.package;
        // Only rename items that are part of the user package
        if package_id == self.compilation.user_package_id {
            if self.is_prepare {
                self.prepare = Some((ast_name.span, ast_name.name.to_string()));
            } else {
                self.locations = self
                    .reference_finder
                    .for_item(item_id, Some(&ast_name.name));
            }
        }
    }

    fn get_spans_for_field_rename(&mut self, item_id: &hir::ItemId, ast_name: &ast::Ident) {
        let package_id = item_id.package;
        // Only rename items that are part of the user package
        if package_id == self.compilation.user_package_id {
            if self.is_prepare {
                self.prepare = Some((ast_name.span, ast_name.name.to_string()));
            } else {
                self.locations = self
                    .reference_finder
                    .for_field(item_id, ast_name.name.clone());
            }
        }
    }

    fn get_spans_for_type_param_rename(
        &mut self,
        param_id: hir::ty::ParamId,
        ast_name: &ast::Ident,
        current_callable: &ast::CallableDecl,
    ) {
        if self.is_prepare {
            let updated_span = remove_leading_quote_from_type_param_span(ast_name.span);
            let updated_name = remove_leading_quote_from_type_param_name(&ast_name.name);
            self.prepare = Some((updated_span, updated_name));
        } else {
            self.locations = self
                .reference_finder
                .for_ty_param(param_id, current_callable)
                .into_iter()
                .map(|l| {
                    assert!(!l.range.empty(), "Type parameter name is empty");
                    Location {
                        range: type_param_ident_range(l.range),
                        ..l
                    }
                })
                .collect();
        }
    }

    fn get_spans_for_local_rename(
        &mut self,
        node_id: ast::NodeId,
        ast_name: &ast::Ident,
        current_callable: Option<&ast::CallableDecl>,
    ) {
        if self.is_prepare {
            self.prepare = Some((ast_name.span, ast_name.name.to_string()));
        } else {
            self.locations = self.reference_finder.for_local(node_id, current_callable);
        }
    }
}

impl<'a> Handler<'a> for Rename<'a> {
    fn at_attr_ref(&mut self, _: &'a ast::Ident) {
        // We don't support renaming attributes.
    }

    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) = self.compilation.get_res(name.id) {
            self.get_spans_for_item_rename(item_id, name);
        }
    }

    fn at_callable_ref(
        &mut self,
        path: &'a ast::Path,
        alias: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        _: &'a hir::CallableDecl,
    ) {
        self.get_spans_for_item_rename(item_id, alias.unwrap_or(&path.name));
    }

    fn at_new_type_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::TyDef,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) = self.compilation.get_res(type_name.id) {
            self.get_spans_for_item_rename(item_id, type_name);
        }
    }

    fn at_struct_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::StructDecl,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) = self.compilation.get_res(type_name.id) {
            self.get_spans_for_item_rename(item_id, type_name);
        }
    }

    fn at_type_param_def(
        &mut self,
        context: &LocatorContext<'a>,
        def_name: &'a ast::Ident,
        param_id: hir::ty::ParamId,
    ) {
        if let Some(curr) = context.current_callable {
            self.get_spans_for_type_param_rename(param_id, def_name, curr);
        }
    }

    fn at_type_param_ref(
        &mut self,
        context: &LocatorContext<'a>,
        reference: &'a ast::Ident,
        param_id: hir::ty::ParamId,
        _: &'a ast::Ident,
    ) {
        if let Some(curr) = context.current_callable {
            self.get_spans_for_type_param_rename(param_id, reference, curr);
        }
    }

    fn at_new_type_ref(
        &mut self,
        path: &'a ast::Path,
        alias: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        _: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.get_spans_for_item_rename(item_id, alias.unwrap_or(&path.name));
    }

    fn at_field_def(
        &mut self,
        context: &LocatorContext<'a>,
        field_name: &ast::Ident,
        _: &'a ast::Ty,
    ) {
        if let Some(item_id) = context.current_udt_id {
            self.get_spans_for_field_rename(item_id, field_name);
        }
    }

    fn at_field_ref(
        &mut self,
        field_ref: &ast::Ident,
        item_id: &hir::ItemId,
        _: &'a hir::ty::UdtField,
    ) {
        self.get_spans_for_field_rename(item_id, field_ref);
    }

    fn at_local_def(
        &mut self,
        context: &LocatorContext<'a>,
        ident: &'a ast::Ident,
        _: &'a ast::Pat,
    ) {
        self.get_spans_for_local_rename(ident.id, ident, context.current_callable);
    }

    fn at_local_ref(
        &mut self,
        context: &LocatorContext<'a>,
        name: &ast::Ident,
        node_id: ast::NodeId,
        _: &'a ast::Ident,
    ) {
        self.get_spans_for_local_rename(node_id, name, context.current_callable);
    }
}

/// Given a range for a type parameter (e.g. `'T`), return a range that excludes the leading single quote.
pub(crate) fn type_param_ident_range(mut range: Range) -> Range {
    range.start.column += 1;
    range
}
