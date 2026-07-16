// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::compilation::{Compilation, source_position_to_package_offset};
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::protocol::Hover;
use crate::qsc_utils::into_range;
use qsc::ast::visit::Visitor;
use qsc::display::{CodeDisplay, Lookup, parse_doc_for_param, parse_doc_for_summary};
use qsc::hir::Attr;
use qsc::line_column::{Encoding, Position, Range};
use qsc::{Span, ast, hir};
use std::fmt::Display;
use std::rc::Rc;
use std::str::FromStr;

pub(crate) fn get_hover(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<Hover> {
    let unit = &compilation.user_unit();
    let offset =
        source_position_to_package_offset(&unit.sources, source_name, position, position_encoding);
    let user_ast_package = &unit.ast.package;

    let mut hover_visitor = HoverGenerator {
        position_encoding,
        compilation,
        hover: None,
        display: CodeDisplay { compilation },
    };

    let mut locator = Locator::new(&mut hover_visitor, offset, compilation);
    locator.visit_package(user_ast_package);
    hover_visitor.hover
}

enum LocalKind {
    Param,
    TypeParam,
    LambdaParam,
    Local,
}

struct HoverGenerator<'a> {
    position_encoding: Encoding,
    hover: Option<Hover>,
    display: CodeDisplay<'a>,
    compilation: &'a Compilation,
}

impl<'a> Handler<'a> for HoverGenerator<'a> {
    fn at_attr_ref(&mut self, name: &'a ast::Ident) {
        let description = match Attr::from_str(&name.name) {
            Ok(attr) => attr.description(),
            Err(()) => return, // No hover information for unsupported attributes.
        };

        self.hover = Some(Hover {
            contents: format!("attribute ```{}```\n\n{}", name.name, description),
            span: self.range(name.span),
        });
    }

    fn at_callable_def(
        &mut self,
        context: &LocatorContext<'a>,
        name: &'a ast::Ident,
        decl: &'a ast::CallableDecl,
    ) {
        let contents = display_callable(
            &context.current_item_doc,
            &context.current_namespace,
            self.display.ast_callable_decl(decl),
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(name.span),
        });
    }

    fn at_callable_ref(
        &mut self,
        path: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        decl: &'a hir::CallableDecl,
    ) {
        let (item, package, _) = self
            .compilation
            .resolve_item_relative_to_user_package(item_id);

        let ns = get_namespace_name(item, package);
        let contents = display_callable(&item.doc, &ns, self.display.hir_callable_decl(decl));
        self.hover = Some(Hover {
            contents,
            span: self.range(path.span),
        });
    }

    fn at_type_param_def(
        &mut self,
        context: &LocatorContext<'a>,
        def_name: &'a ast::Ident,
        _: hir::ty::ParamId,
    ) {
        let code = markdown_fenced_block(def_name.name.clone());
        let callable_name = context.current_callable.map(|c| c.name.name.clone());
        let contents = display_local(
            &LocalKind::TypeParam,
            &code,
            &def_name.name,
            callable_name.as_deref(),
            &context.current_item_doc,
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(def_name.span),
        });
    }

    fn at_type_param_ref(
        &mut self,
        context: &LocatorContext<'a>,
        reference: &'a ast::Ident,
        _: hir::ty::ParamId,
        _: &'a ast::Ident,
    ) {
        let code = markdown_fenced_block(reference.name.clone());
        let callable_name = context.current_callable.map(|c| c.name.name.clone());
        let contents = display_local(
            &LocalKind::TypeParam,
            &code,
            &reference.name,
            callable_name.as_deref(),
            &context.current_item_doc,
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(reference.span),
        });
    }

    fn at_new_type_def(
        &mut self,
        context: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        def: &'a ast::TyDef,
    ) {
        let code = self.display.ident_ty_def(type_name, def);
        let contents = display_udt(
            &context.current_item_doc,
            &context.current_namespace,
            code,
            def.is_struct(),
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(type_name.span),
        });
    }

    fn at_struct_def(
        &mut self,
        context: &LocatorContext<'a>,
        type_name: &'a ast::Ident,
        def: &'a ast::StructDecl,
    ) {
        let code = self.display.struct_decl(def);
        let contents = display_udt(
            &context.current_item_doc,
            &context.current_namespace,
            code,
            true,
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(type_name.span),
        });
    }

    fn at_new_type_ref(
        &mut self,
        path: &'a ast::Path,
        _: Option<&'a ast::Ident>,
        item_id: &hir::ItemId,
        _: &'a hir::Ident,
        udt: &'a hir::ty::Udt,
    ) {
        let (item, package, _) = self
            .compilation
            .resolve_item_relative_to_user_package(item_id);

        let ns = get_namespace_name(item, package);
        let code = self.display.hir_udt(udt);
        let contents = display_udt(&item.doc, &ns, code, udt.is_struct());
        self.hover = Some(Hover {
            contents,
            span: self.range(path.span),
        });
    }

    fn at_field_def(
        &mut self,
        context: &LocatorContext<'a>,
        field_name: &ast::Ident,
        ty: &'a ast::Ty,
    ) {
        let contents = display_udt_field(
            &context.current_item_name,
            self.display.ident_ty(field_name, ty),
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(field_name.span),
        });
    }

    fn at_field_ref(
        &mut self,
        field_ref: &ast::Ident,
        item_id: &hir::ItemId,
        field_definition: &'a hir::ty::UdtField,
    ) {
        let (item, _, _) = self
            .compilation
            .resolve_item_relative_to_user_package(item_id);

        if let hir::ItemKind::Ty(name, _) = &item.kind {
            let contents =
                display_udt_field(&name.name, self.display.hir_udt_field(field_definition));
            self.hover = Some(Hover {
                contents,
                span: self.range(field_ref.span),
            });
        }
    }

    fn at_local_def(
        &mut self,
        context: &LocatorContext<'a>,
        ident: &'a ast::Ident,
        pat: &'a ast::Pat,
    ) {
        let code = markdown_fenced_block(self.display.name_ty_id(&ident.name, pat.id));
        let kind = if context.in_params {
            LocalKind::Param
        } else if context.in_lambda_params {
            LocalKind::LambdaParam
        } else {
            LocalKind::Local
        };
        let callable_name = context.current_callable.map(|c| c.name.name.clone());
        let contents = display_local(
            &kind,
            &code,
            &ident.name,
            callable_name.as_deref(),
            &context.current_item_doc,
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(ident.span),
        });
    }

    fn at_local_ref(
        &mut self,
        context: &LocatorContext<'a>,
        name: &ast::Ident,
        node_id: ast::NodeId,
        definition: &'a ast::Ident,
    ) {
        let local_name = &definition.name;
        let callable_name = context.current_callable.map(|c| c.name.name.clone());
        let code = markdown_fenced_block(self.display.name_ty_id(local_name, node_id));
        let kind = if is_param(&curr_callable_to_params(context.current_callable), node_id) {
            LocalKind::Param
        } else if is_param(&context.lambda_params, node_id) {
            LocalKind::LambdaParam
        } else {
            LocalKind::Local
        };
        let contents = display_local(
            &kind,
            &code,
            local_name,
            callable_name.as_deref(),
            &context.current_item_doc,
        );
        self.hover = Some(Hover {
            contents,
            span: self.range(name.span),
        });
    }
}

impl HoverGenerator<'_> {
    fn range(&self, span: Span) -> Range {
        into_range(
            self.position_encoding,
            span,
            &self.compilation.user_unit().sources,
        )
    }
}

fn get_namespace_name(item: &hir::Item, package: &hir::Package) -> Rc<str> {
    item.parent
        .and_then(|parent_id| package.items.get(parent_id))
        .map_or_else(
            || Rc::from(""),
            |parent| match &parent.kind {
                hir::ItemKind::Namespace(namespace, _) => namespace.name(),
                _ => Rc::from(""),
            },
        )
}

fn curr_callable_to_params(curr_callable: Option<&ast::CallableDecl>) -> Vec<&ast::Pat> {
    match curr_callable {
        Some(decl) => match &*decl.body {
            ast::CallableBody::Block(_) => vec![decl.input.as_ref()],
            ast::CallableBody::Specs(spec_decls) => {
                let mut pats = spec_decls
                    .iter()
                    .filter_map(|spec| match &spec.body {
                        ast::SpecBody::Gen(_) => None,
                        ast::SpecBody::Impl(input, _) => Some(input.as_ref()),
                    })
                    .collect::<Vec<&ast::Pat>>();
                pats.push(decl.input.as_ref());
                pats
            }
        },
        None => vec![],
    }
}

fn is_param(param_pats: &[&ast::Pat], node_id: ast::NodeId) -> bool {
    fn find_in_pat(pat: &ast::Pat, node_id: ast::NodeId) -> bool {
        match &*pat.kind {
            ast::PatKind::Bind(ident, _) => node_id == ident.id,
            ast::PatKind::Discard(_) | ast::PatKind::Elided | ast::PatKind::Err => false,
            ast::PatKind::Paren(inner) => find_in_pat(inner, node_id),
            ast::PatKind::Tuple(inner) => inner.iter().any(|x| find_in_pat(x, node_id)),
        }
    }

    param_pats.iter().any(|pat| find_in_pat(pat, node_id))
}

fn display_local(
    param_kind: &LocalKind,
    markdown: &String,
    local_name: &str,
    callable_name: Option<&str>,
    callable_doc: &str,
) -> String {
    match param_kind {
        LocalKind::Param => {
            let param_doc = parse_doc_for_param(callable_doc, local_name);
            let callable_name = callable_name.expect("param should have a callable name");
            with_doc(
                &param_doc,
                format!("parameter of `{callable_name}`\n{markdown}"),
            )
        }
        LocalKind::TypeParam => {
            let param_doc = parse_doc_for_param(callable_doc, local_name);
            let callable_name = callable_name.expect("type param should have a callable name");
            with_doc(
                &param_doc,
                format!("type parameter of `{callable_name}`\n{markdown}"),
            )
        }
        LocalKind::LambdaParam => format!("lambda parameter\n{markdown}"),
        LocalKind::Local => format!("local\n{markdown}"),
    }
}

fn display_callable(doc: &str, namespace: &str, code: impl Display) -> String {
    let summary = parse_doc_for_summary(doc);
    let markdown = markdown_fenced_block(code);
    if namespace.is_empty() {
        with_doc(&summary, format!("callable\n{markdown}"))
    } else {
        with_doc(&summary, format!("callable of `{namespace}`\n{markdown}"))
    }
}

fn display_udt(doc: &str, namespace: &str, code: impl Display, is_struct: bool) -> String {
    let summary = parse_doc_for_summary(doc);
    let markdown = markdown_fenced_block(code);
    let kind = if is_struct {
        "struct"
    } else {
        "user-defined type"
    };
    if namespace.is_empty() {
        with_doc(&summary, format!("{kind}\n{markdown}"))
    } else {
        with_doc(&summary, format!("{kind} of `{namespace}`\n{markdown}"))
    }
}

fn display_udt_field(udt_name: &str, code: impl Display) -> String {
    let markdown = markdown_fenced_block(code);
    format!("field of `{udt_name}`\n{markdown}")
}

fn with_doc(doc: &String, code: impl Display) -> String {
    if doc.is_empty() {
        code.to_string()
    } else {
        format!("{code}---\n{doc}\n")
    }
}

fn markdown_fenced_block(code: impl Display) -> String {
    format!(
        "```qsharp
{code}
```
"
    )
}
