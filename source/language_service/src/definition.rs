// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::compilation::{Compilation, CompilationKind, source_position_to_package_offset};
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::qsc_utils::into_location;
use qsc::ast::visit::Visitor;
use qsc::hir::PackageId;
use qsc::line_column::{Encoding, Position};
use qsc::location::Location;
use qsc::{Span, ast, hir};

pub(crate) fn get_definition(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<Location> {
    if let CompilationKind::OpenQASM { sources, .. } = &compilation.kind {
        return crate::openqasm::get_definition(sources, source_name, position, position_encoding);
    }
    let unit = &compilation.user_unit();
    let offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);
    let user_ast_package = &unit.ast.package;

    let mut definition_finder = DefinitionFinder {
        position_encoding,
        compilation,
        definition: None,
    };

    let mut locator = Locator::new(&mut definition_finder, offset, compilation);
    locator.visit_package(user_ast_package);

    definition_finder.definition
}

struct DefinitionFinder<'a> {
    position_encoding: Encoding,
    compilation: &'a Compilation,
    definition: Option<Location>,
}

impl<'a> Handler<'a> for DefinitionFinder<'a> {
    fn at_attr_ref(&mut self, _: &'a ast::Ident) {
        // We don't support goto def for attributes.
    }

    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        self.definition = Some(self.location(name.span, self.compilation.user_package_id));
    }

    fn at_callable_ref(
        &mut self,
        _: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        decl: &'a hir::CallableDecl,
    ) {
        self.definition = Some(self.location(decl.name.span, item_id.package));
    }

    fn at_type_param_def(
        &mut self,
        _: &LocatorContext<'a>,
        def_name: &'a ast::Ident,
        _: hir::ty::ParamId,
    ) {
        self.definition = Some(self.location(def_name.span, self.compilation.user_package_id));
    }

    fn at_type_param_ref(
        &mut self,
        _: &LocatorContext<'a>,
        _: &'a ast::Ident,
        _: hir::ty::ParamId,
        definition: &'a ast::Ident,
    ) {
        self.definition = Some(self.location(definition.span, self.compilation.user_package_id));
    }

    fn at_new_type_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::TyDef,
    ) {
        self.definition = Some(self.location(type_name.span, self.compilation.user_package_id));
    }

    fn at_struct_def(
        &mut self,
        _: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        _: &'a ast::StructDecl,
    ) {
        self.definition = Some(self.location(type_name.span, self.compilation.user_package_id));
    }

    fn at_new_type_ref(
        &mut self,
        _: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        type_name: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.definition = Some(self.location(type_name.span, item_id.package));
    }

    fn at_field_def(&mut self, _: &LocatorContext<'a>, field_name: &ast::Ident, _: &'a ast::Ty) {
        self.definition = Some(self.location(field_name.span, self.compilation.user_package_id));
    }

    fn at_field_ref(
        &mut self,
        _: &ast::Ident,
        item_id: &hir::ItemId,
        field_def: &'a hir::ty::UdtField,
    ) {
        let span = field_def
            .name_span
            .expect("field found via name should have a name");
        self.definition = Some(self.location(span, item_id.package));
    }

    fn at_local_def(&mut self, _: &LocatorContext<'a>, ident: &'a ast::Ident, _: &'a ast::Pat) {
        self.definition = Some(self.location(ident.span, self.compilation.user_package_id));
    }

    fn at_local_ref(
        &mut self,
        _: &LocatorContext<'a>,
        _: &ast::Ident,
        _: ast::NodeId,
        definition: &'a ast::Ident,
    ) {
        self.definition = Some(self.location(definition.span, self.compilation.user_package_id));
    }
}

impl DefinitionFinder<'_> {
    fn location(&self, location: Span, package_id: PackageId) -> Location {
        into_location(
            self.position_encoding,
            self.compilation,
            location,
            package_id,
        )
    }
}
