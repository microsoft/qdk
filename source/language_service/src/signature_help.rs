// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    Encoding,
    compilation::{Compilation, source_position_to_package_offset},
    protocol::{ParameterInformation, SignatureHelp, SignatureInformation},
};
use qsc::{
    Span,
    ast::{
        self, PathKind,
        visit::{Visitor, walk_expr, walk_item},
    },
    display::{CodeDisplay, Lookup, parse_doc_for_param, parse_doc_for_summary},
    hir,
    line_column::{Position, Range},
    resolve,
};
use std::iter::zip;

pub(crate) fn get_signature_help(
    compilation: &Compilation,
    source_name: &str,
    position: Position,
    position_encoding: Encoding,
) -> Option<SignatureHelp> {
    let offset = source_position_to_package_offset(
        &compilation.user_unit().sources,
        source_name,
        position,
        position_encoding,
    );
    let user_ast_package = &compilation.user_unit().ast.package;

    let mut finder = SignatureHelpFinder {
        compilation,
        offset,
        signature_help: None,
        display: CodeDisplay { compilation },
    };

    finder.visit_package(user_ast_package);

    if position_encoding == Encoding::Utf16 {
        // Fix up parameter offsets to be utf-16 based.
        if let Some(ref mut sig_help) = finder.signature_help {
            for sig in &mut sig_help.signatures {
                for param in &mut sig.parameters {
                    let range = Range::from_span(
                        position_encoding,
                        &sig.label,
                        &Span {
                            lo: param.label.0,
                            hi: param.label.1,
                        },
                    );
                    assert!(
                        range.start.line == 0 && range.end.line == 0,
                        "generated signature help should not have line breaks"
                    );
                    param.label = (range.start.column, range.end.column);
                }
            }
        }
    }

    finder.signature_help
}

struct SignatureHelpFinder<'a> {
    compilation: &'a Compilation,
    offset: u32,
    signature_help: Option<SignatureHelp>,
    display: CodeDisplay<'a>,
}

impl<'a> Visitor<'a> for SignatureHelpFinder<'a> {
    fn visit_item(&mut self, item: &'a ast::Item) {
        if item.span.contains(self.offset) {
            walk_item(self, item);
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if expr.span.touches(self.offset) {
            match &*expr.kind {
                ast::ExprKind::Call(callee, args) if args.span.touches(self.offset) => {
                    walk_expr(self, args);
                    if self.signature_help.is_none() {
                        let callee = unwrap_parens(callee);
                        match try_get_direct_callee(self.compilation, callee) {
                            Some((_, callee_decl, item_doc)) => {
                                self.process_direct_callee(callee_decl, item_doc, args);
                            }
                            None => self.process_indirect_callee(callee, args),
                        }
                    }
                }
                _ => walk_expr(self, expr),
            }
        }
    }
}

impl SignatureHelpFinder<'_> {
    fn process_indirect_callee(&mut self, callee: &ast::Expr, args: &ast::Expr) {
        if let Some(ty) = self.compilation.get_ty(callee.id)
            && let hir::ty::Ty::Arrow(arrow) = &ty
        {
            let sig_info = SignatureInformation {
                label: ty.display(),
                documentation: None,
                parameters: get_type_params(&arrow.input.borrow()),
            };

            // Capture arrow.input structure in a fake HIR Pat.
            let params = make_fake_pat(&arrow.input.borrow());

            self.signature_help = Some(SignatureHelp {
                signatures: vec![sig_info],
                active_signature: 0,
                active_parameter: process_args(args, self.offset, &params),
            });
        }
    }

    fn process_direct_callee(
        &mut self,
        callee_decl: &hir::CallableDecl,
        item_doc: &str,
        args: &ast::Expr,
    ) {
        let documentation = parse_doc_for_summary(item_doc);
        let documentation = (!documentation.is_empty()).then_some(documentation);

        let sig_info = SignatureInformation {
            label: self.display.hir_callable_decl(callee_decl).to_string(),
            documentation,
            parameters: self.get_params(callee_decl, item_doc),
        };

        self.signature_help = Some(SignatureHelp {
            signatures: vec![sig_info],
            active_signature: 0,
            active_parameter: process_args(args, self.offset, &callee_decl.input),
        });
    }

    /// Takes a callable declaration node and the callable's doc string and
    /// generates the Parameter Information objects for the callable.
    /// Example:
    /// ```qsharp
    /// operation Foo(bar: Int, baz: Double) : Unit {}
    ///               └──┬───┘  └──┬──────┘
    ///               param 1    param 2
    ///              └─────────┬───────────┘
    ///                      param 0
    /// ```
    fn get_params(&self, decl: &hir::CallableDecl, doc: &str) -> Vec<ParameterInformation> {
        let mut offset = self.display.get_param_offset(decl);

        match &decl.input.kind {
            hir::PatKind::Discard | hir::PatKind::Err | hir::PatKind::Bind(_) => {
                self.make_wrapped_params(offset, &decl.input, doc)
            }
            hir::PatKind::Tuple(_) => self.make_param_with_offset(&mut offset, &decl.input, doc),
        }
    }

    /// Callables with a single parameter in their parameter list are special-cased
    /// because we need to re-wrap the parameter into a tuple.
    fn make_wrapped_params(
        &self,
        offset: u32,
        pat: &hir::Pat,
        doc: &str,
    ) -> Vec<ParameterInformation> {
        let documentation = if let hir::PatKind::Bind(name) = &pat.kind {
            let documentation = parse_doc_for_param(doc, &name.name);
            (!documentation.is_empty()).then_some(documentation)
        } else {
            None
        };

        let len = usize_to_u32(self.display.hir_pat(pat).to_string().len());
        let param = ParameterInformation {
            label: (offset + 1, offset + len + 1),
            documentation,
        };

        let wrapper = ParameterInformation {
            label: (offset, offset + len + 2),
            documentation: None,
        };

        vec![wrapper, param]
    }

    fn make_param_with_offset(
        &self,
        offset: &mut u32,
        pat: &hir::Pat,
        doc: &str,
    ) -> Vec<ParameterInformation> {
        match &pat.kind {
            hir::PatKind::Bind(_) | hir::PatKind::Discard | hir::PatKind::Err => {
                let documentation = if let hir::PatKind::Bind(name) = &pat.kind {
                    let documentation = parse_doc_for_param(doc, &name.name);
                    (!documentation.is_empty()).then_some(documentation)
                } else {
                    None
                };

                let len = usize_to_u32(self.display.hir_pat(pat).to_string().len());
                let start = *offset;
                *offset += len;
                vec![ParameterInformation {
                    label: (start, *offset),
                    documentation,
                }]
            }
            hir::PatKind::Tuple(items) => {
                let len = usize_to_u32(self.display.hir_pat(pat).to_string().len());
                let mut rtrn = vec![ParameterInformation {
                    label: (*offset, *offset + len),
                    documentation: None,
                }];
                *offset += 1; // for the open parenthesis
                let mut is_first = true;
                for item in items {
                    if is_first {
                        is_first = false;
                    } else {
                        *offset += 2; // 2 for the comma and space
                    }
                    rtrn.extend(self.make_param_with_offset(offset, item, doc));
                }
                *offset += 1; // for the close parenthesis
                rtrn
            }
        }
    }
}

fn get_type_params(ty: &hir::ty::Ty) -> Vec<ParameterInformation> {
    let mut offset = 1; // 1 for the open parenthesis character

    match &ty {
        hir::ty::Ty::Tuple(_) => make_type_param_with_offset(&mut offset, ty),
        _ => params_for_single_type_parameter(offset, ty),
    }
}

/// Callables with a single parameter in their parameter list are special-cased
/// because we need to insert an additional parameter info to make the list
/// compatible with the argument processing logic.
fn params_for_single_type_parameter(offset: u32, ty: &hir::ty::Ty) -> Vec<ParameterInformation> {
    let len = usize_to_u32(ty.display().len());
    let start = offset;
    let end = offset + len;

    let param = ParameterInformation {
        label: (start, end),
        documentation: None,
    };

    // The wrapper is a duplicate of the parameter. This is to make
    // the generated list of parameter information objects compatible
    // with the argument processing logic.
    let wrapper = ParameterInformation {
        label: (start, end),
        documentation: None,
    };

    vec![wrapper, param]
}

fn make_type_param_with_offset(offset: &mut u32, ty: &hir::ty::Ty) -> Vec<ParameterInformation> {
    if let hir::ty::Ty::Tuple(items) = &ty {
        let len = usize_to_u32(ty.display().len());
        let mut rtrn = vec![ParameterInformation {
            label: (*offset, *offset + len),
            documentation: None,
        }];
        *offset += 1; // for the open parenthesis
        let mut is_first = true;
        for item in items {
            if is_first {
                is_first = false;
            } else {
                *offset += 2; // 2 for the comma and space
            }
            rtrn.extend(make_type_param_with_offset(offset, item));
        }
        *offset += 1; // for the close parenthesis
        rtrn
    } else {
        let len = usize_to_u32(ty.display().len());
        let start = *offset;
        *offset += len;
        vec![ParameterInformation {
            label: (start, *offset),
            documentation: None,
        }]
    }
}

fn unwrap_parens(expr: &ast::Expr) -> &ast::Expr {
    match &*expr.kind {
        ast::ExprKind::Paren(inner) => unwrap_parens(inner),
        _ => expr,
    }
}

fn usize_to_u32(x: usize) -> u32 {
    u32::try_from(x).expect("failed to cast usize to u32 while generating signature help")
}

/// If the `callee` expression is a direct reference to a callable, returns
/// the callable and its doc string, else returns None.
fn try_get_direct_callee<'a>(
    compilation: &'a Compilation,
    callee: &ast::Expr,
) -> Option<(hir::PackageId, &'a hir::CallableDecl, &'a str)> {
    if let ast::ExprKind::Path(PathKind::Ok(path)) = &*callee.kind
        && let Some(resolve::Res::Item(item_id, _)) = compilation.get_res(path.id)
    {
        let (item, _, resolved_item_id) =
            compilation.resolve_item_relative_to_user_package(item_id);
        if let hir::ItemKind::Callable(callee_decl) = &item.kind {
            return Some((
                resolved_item_id
                    .package
                    .expect("package id should be resolved"),
                callee_decl,
                &item.doc,
            ));
        }
    }

    None
}

/// Captures the hierarchical structure of a type based on Tuples by
/// creating an HIR Pat with the same hierarchical structure. Other than
/// the structure, the Pat and sub-Pats have default field values.
fn make_fake_pat(ty: &hir::ty::Ty) -> hir::Pat {
    hir::Pat {
        id: hir::NodeId::default(),
        span: qsc::Span::default(),
        ty: hir::ty::Ty::Err,
        kind: match ty {
            hir::ty::Ty::Tuple(items) => {
                hir::PatKind::Tuple(items.iter().map(make_fake_pat).collect())
            }
            _ => hir::PatKind::Discard,
        },
    }
}

fn process_args(args: &ast::Expr, location: u32, params: &hir::Pat) -> u32 {
    fn count_params(params: &hir::Pat) -> i32 {
        match &params.kind {
            hir::PatKind::Bind(_) | hir::PatKind::Discard | hir::PatKind::Err => 1,
            hir::PatKind::Tuple(items) => items.iter().map(count_params).sum::<i32>() + 1,
        }
    }

    fn try_as_tuple<'a, 'b>(
        args: &'a ast::Expr,
        params: &'b hir::Pat,
        top: bool,
    ) -> Option<(Vec<&'a ast::Expr>, Vec<&'b hir::Pat>)> {
        let args = match &*args.kind {
            ast::ExprKind::Tuple(arg_items) => Some(
                arg_items
                    .iter()
                    .map(std::convert::AsRef::as_ref)
                    .collect::<Vec<_>>(),
            ),
            ast::ExprKind::Paren(arg) => Some(vec![arg.as_ref()]),
            _ => None,
        };

        let params = if let hir::PatKind::Tuple(param_items) = &params.kind {
            Some(param_items.iter().collect::<Vec<_>>())
        } else if top {
            Some(vec![params])
        } else {
            None
        };

        match (args, params) {
            (Some(args), Some(params)) => Some((args, params)),
            _ => None,
        }
    }

    fn increment_until_cursor(
        args: &ast::Expr,
        cursor: u32,
        params: &hir::Pat,
        i: &mut i32,
        top: bool,
    ) {
        if cursor < args.span.lo {
            return;
        }

        if args.span.hi < cursor {
            *i += count_params(params);
            return;
        }

        if let Some((arg_items, param_items)) = try_as_tuple(args, params, top) {
            // check to see if cursor is inside of tuple, past the starting `(` but before the ending `)`
            if args.span.lo < cursor && cursor < args.span.hi {
                let items = zip(&arg_items, &param_items).collect::<Vec<_>>();

                // is the cursor after the last item of a *finished* parameter tuple?
                let is_inside_coda = param_items.len() <= arg_items.len()
                    && match items.last() {
                        Some(last) => last.0.span.hi < cursor,
                        None => true,
                    };

                if !is_inside_coda {
                    *i += 1;
                    for (arg, param) in items {
                        increment_until_cursor(arg, cursor, param, i, false);
                    }
                }
            }
        }
    }

    let mut i = 0;
    increment_until_cursor(args, location, params, &mut i, true);
    i.try_into().expect("got negative param index")
}
