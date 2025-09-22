// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    closure::{self, Lambda, PartialApp},
    resolve::{self, Names, iter_valid_items},
    typeck::{
        self,
        convert::{self, synthesize_functor_params},
    },
};
use miette::Diagnostic;
use qsc_ast::ast::{self, FieldAccess, Ident, Idents, PathKind};
use qsc_data_structures::{
    index_map::IndexMap,
    span::Span,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_hir::{
    assigner::Assigner,
    hir::{self, ItemId, LocalItemId, Res, Visibility},
    mut_visit::MutVisitor,
    ty::{Arrow, FunctorSetValue, GenericArg, ParamId, Ty, TypeParameter},
};
use std::{
    clone::Clone,
    iter::{once, repeat},
    rc::Rc,
    str::FromStr,
    vec,
};
use thiserror::Error;

use self::convert::TyConversionError;

#[derive(Clone, Debug, Diagnostic, Error)]
pub(super) enum Error {
    #[error("unknown attribute {0}")]
    #[diagnostic(help(
        "supported attributes are: EntryPoint, Config, SimulatableIntrinsic, Measurement, Reset"
    ))]
    #[diagnostic(code("Qsc.LowerAst.UnknownAttr"))]
    UnknownAttr(String, #[label] Span),
    #[error("invalid attribute arguments: expected {0}")]
    #[diagnostic(code("Qsc.LowerAst.InvalidAttrArgs"))]
    InvalidAttrArgs(String, #[label] Span),
    #[error("invalid use of the {0} attribute on a function")]
    #[diagnostic(help("try declaring the callable as an operation"))]
    #[diagnostic(code("Qsc.LowerAst.InvalidAttrOnFunction"))]
    InvalidAttrOnFunction(String, #[label] Span),
    #[error("missing callable body")]
    #[diagnostic(code("Qsc.LowerAst.MissingBody"))]
    MissingBody(#[label] Span),
    #[error("duplicate specialization")]
    #[diagnostic(code("Qsc.LowerAst.DuplicateSpec"))]
    DuplicateSpec(#[label] Span),
    #[error("invalid use of elided pattern")]
    #[diagnostic(code("Qsc.LowerAst.InvalidElidedPat"))]
    InvalidElidedPat(#[label] Span),
    #[error("invalid pattern for specialization declaration")]
    #[diagnostic(code("Qsc.LowerAst.InvalidSpecPat"))]
    InvalidSpecPat(#[label] Span),
    #[error("missing type in item signature")]
    #[diagnostic(help("a type must be provided for this item"))]
    #[diagnostic(code("Qsc.LowerAst.MissingTy"))]
    MissingTy {
        #[label]
        span: Span,
    },
    #[error("unrecognized class constraint {name}")]
    #[help(
        "supported classes are Eq, Add, Sub, Mul, Div, Mod, Signed, Ord, Exp, Integral, and Show"
    )]
    #[diagnostic(code("Qsc.LowerAst.UnrecognizedClass"))]
    UnrecognizedClass {
        #[label]
        span: Span,
        name: String,
    },
    #[error("class constraint is recursive via {name}")]
    #[help(
        "if a type refers to itself via its constraints, it is self-referential and cannot ever be resolved"
    )]
    #[diagnostic(code("Qsc.LowerAst.RecursiveClassConstraint"))]
    RecursiveClassConstraint {
        #[label]
        span: Span,
        name: String,
    },
    #[error("expected {expected} parameters for constraint, found {found}")]
    #[diagnostic(code("Qsc.TypeCk.IncorrectNumberOfConstraintParameters"))]
    IncorrectNumberOfConstraintParameters {
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("namespace cannot be exported since it is a parent namespace")]
    #[diagnostic(code("Qsc.LowerAst.ParentNamespaceExport"))]
    #[diagnostic(help(
        "to make this namespace exportable, consider explicitly declaring it in source: `namespace Foo {{ ... }}`"
    ))]
    ParentNamespaceExport {
        #[label]
        span: Span,
    },
    #[error("reexporting a namespace from another package is not supported")]
    #[diagnostic(help("consider reexporting items individually"))]
    #[diagnostic(code("Qsc.LowerAst.CrossPackageNamespaceReexport"))]
    CrossPackageNamespaceReexport(#[label] Span),
}

impl From<TyConversionError> for Error {
    fn from(err: TyConversionError) -> Self {
        use TyConversionError::*;
        match err {
            MissingTy { span } => Error::MissingTy { span },
            UnrecognizedClass { span, name } => Error::UnrecognizedClass { span, name },
            RecursiveClassConstraint { span, name } => {
                Error::RecursiveClassConstraint { span, name }
            }
            IncorrectNumberOfConstraintParameters {
                expected,
                found,
                span,
            } => Error::IncorrectNumberOfConstraintParameters {
                expected,
                found,
                span,
            },
        }
    }
}

pub(super) struct Lowerer {
    nodes: IndexMap<ast::NodeId, hir::NodeId>,
    locals: IndexMap<hir::NodeId, (hir::Ident, Ty)>,
    parent: Option<LocalItemId>,
    items: Vec<hir::Item>,
    errors: Vec<Error>,
}

impl Lowerer {
    pub(super) fn new() -> Self {
        Self {
            nodes: IndexMap::new(),
            locals: IndexMap::new(),
            parent: None,
            items: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn clear_items(&mut self) {
        self.items.clear();
    }

    pub(super) fn drain_errors(&mut self) -> vec::Drain<Error> {
        self.errors.drain(..)
    }

    pub(super) fn with<'a>(
        &'a mut self,
        assigner: &'a mut Assigner,
        names: &'a Names,
        tys: &'a typeck::Table,
    ) -> With<'a> {
        With {
            lowerer: self,
            assigner,
            names,
            tys,
        }
    }
}

pub(super) struct With<'a> {
    lowerer: &'a mut Lowerer,
    assigner: &'a mut Assigner,
    names: &'a Names,
    tys: &'a typeck::Table,
}

impl With<'_> {
    pub(super) fn lower_package(&mut self, package: &ast::Package) -> hir::Package {
        let mut stmts = Vec::new();
        for node in &package.nodes {
            match node {
                ast::TopLevelNode::Namespace(namespace) => self.lower_namespace(namespace),
                ast::TopLevelNode::Stmt(stmt) => {
                    stmts.extend(self.lower_stmt(stmt));
                }
            }
        }

        let entry = package.entry.as_ref().map(|e| self.lower_expr(e));

        let mut items = self
            .lowerer
            .items
            .drain(..)
            .map(|i| (i.id, i))
            .collect::<IndexMap<_, _>>();

        collapse_self_exports(&mut items);

        hir::Package {
            items,
            stmts,
            entry,
        }
    }

    pub(super) fn lower_namespace(&mut self, namespace: &ast::Namespace) {
        let Some(&resolve::Res::Item(hir::ItemId { item: id, .. }, _)) = self.names.get(
            namespace
                .name
                .last()
                .expect("namespace name should contain at least one ident")
                .id,
        ) else {
            panic!("namespace should have item ID");
        };

        self.lowerer.parent = Some(id);

        let items = namespace
            .items
            .iter()
            .flat_map(|i| self.lower_item(i))
            .collect::<Vec<_>>();

        let name = self.lower_idents(&namespace.name);

        self.lowerer.items.push(hir::Item {
            id,
            span: namespace.span,
            parent: None,
            doc: Rc::clone(&namespace.doc),
            attrs: Vec::new(),
            visibility: hir::Visibility::Public,
            kind: hir::ItemKind::Namespace(name, items),
        });

        self.lowerer.parent = None;
    }

    fn lower_item(&mut self, item: &ast::Item) -> Vec<LocalItemId> {
        let attrs: Vec<_> = item
            .attrs
            .iter()
            .filter_map(|a| self.lower_attr(a))
            .collect();

        let resolve_id = |id| match self.names.get(id) {
            Some(&resolve::Res::Item(item, _)) => item,
            _ => panic!("item should have item ID"),
        };

        let mut items = Vec::new();
        match &*item.kind {
            ast::ItemKind::Err | ast::ItemKind::Open(..) => {}
            ast::ItemKind::ImportOrExport(decl) if decl.is_import() => {}
            ast::ItemKind::ImportOrExport(decl) => {
                // Only exports are handled here, imports vanish in the HIR
                for item in iter_valid_items(decl) {
                    let id = resolve_id(item.name().id);
                    let res = self.path_to_res(item.path);
                    let name = self.lower_ident(item.name());
                    items.push((id, hir::ItemKind::Export(name, res), Visibility::Public));
                }
            }
            ast::ItemKind::Callable(callable) => {
                let id = resolve_id(callable.name.id);
                let grandparent = self.lowerer.parent;
                self.lowerer.parent = Some(id.item);
                let (callable, errs) = self.lower_callable_decl(callable, &attrs);
                self.lowerer.errors.extend(
                    errs.into_iter().map(|err| {
                        Into::<Error>::into(Into::<convert::TyConversionError>::into(err))
                    }),
                );
                self.lowerer.parent = grandparent;
                items.push((
                    id,
                    hir::ItemKind::Callable(callable.into()),
                    Visibility::Internal,
                ));
            }
            ast::ItemKind::Ty(name, _) => {
                let id = resolve_id(name.id);
                let udt = self
                    .tys
                    .udts
                    .get(&id)
                    .expect("type item should have lowered UDT");

                items.push((
                    id,
                    hir::ItemKind::Ty(self.lower_ident(name), udt.clone()),
                    Visibility::Internal,
                ));
            }
            ast::ItemKind::Struct(decl) => {
                let id = resolve_id(decl.name.id);
                let strct = self
                    .tys
                    .udts
                    .get(&id)
                    .expect("type item should have lowered struct");

                items.push((
                    id,
                    hir::ItemKind::Ty(self.lower_ident(&decl.name), strct.clone()),
                    Visibility::Internal,
                ));
            }
        }

        let ids = items.iter().map(|(id, _, _)| id.item).collect::<Vec<_>>();

        self.lowerer.items.extend(
            items
                .into_iter()
                .zip(once(attrs).chain(repeat(Vec::new()))) // only apply the attrs to the first item
                .map(|((id, kind, visibility), attrs)| hir::Item {
                    id: id.item,
                    span: item.span,
                    parent: self.lowerer.parent,
                    doc: Rc::clone(&item.doc),
                    attrs,
                    visibility,
                    kind,
                }),
        );

        ids
    }

    fn lower_attr(&mut self, attr: &ast::Attr) -> Option<hir::Attr> {
        match hir::Attr::from_str(attr.name.name.as_ref()) {
            Ok(hir::Attr::EntryPoint) => match &*attr.arg.kind {
                ast::ExprKind::Tuple(args) if args.is_empty() => Some(hir::Attr::EntryPoint),
                // @EntryPoint(Profile)
                ast::ExprKind::Paren(inner)
                    if matches!(inner.kind.as_ref(), ast::ExprKind::Path(PathKind::Ok(path))
                if Profile::from_str(path.name.name.as_ref()).is_ok()) =>
                {
                    Some(hir::Attr::EntryPoint)
                }
                // Any other form is not valid so generates an error.
                _ => {
                    self.lowerer.errors.push(Error::InvalidAttrArgs(
                        "empty or profile name".to_string(),
                        attr.arg.span,
                    ));
                    None
                }
            },
            Ok(hir::Attr::Unimplemented) => match &*attr.arg.kind {
                ast::ExprKind::Tuple(args) if args.is_empty() => Some(hir::Attr::Unimplemented),
                _ => {
                    self.lowerer
                        .errors
                        .push(Error::InvalidAttrArgs("()".to_string(), attr.arg.span));
                    None
                }
            },
            Ok(hir::Attr::Config) => {
                match &*attr.arg.kind {
                    // @Config(Capability)
                    ast::ExprKind::Paren(inner)
                        if matches!(inner.kind.as_ref(), ast::ExprKind::Path(PathKind::Ok(path))
                    if TargetCapabilityFlags::from_str(path.name.name.as_ref()).is_ok()) => {}

                    // @Config(not Capability)
                    ast::ExprKind::Paren(inner)
                        if matches!(inner.kind.as_ref(), ast::ExprKind::UnOp(ast::UnOp::NotL, inner)
                        if matches!(inner.kind.as_ref(), ast::ExprKind::Path(PathKind::Ok(path))
                    if TargetCapabilityFlags::from_str(path.as_ref().name.name.as_ref()).is_ok())) =>
                        {}

                    // Any other form is not valid so generates an error.
                    _ => {
                        self.lowerer.errors.push(Error::InvalidAttrArgs(
                            "runtime capability".to_string(),
                            attr.arg.span,
                        ));
                    }
                }
                None
            }
            Ok(hir::Attr::SimulatableIntrinsic) => match &*attr.arg.kind {
                ast::ExprKind::Tuple(args) if args.is_empty() => {
                    Some(hir::Attr::SimulatableIntrinsic)
                }
                _ => {
                    self.lowerer
                        .errors
                        .push(Error::InvalidAttrArgs("()".to_string(), attr.arg.span));
                    None
                }
            },
            Ok(hir::Attr::Measurement) => match &*attr.arg.kind {
                ast::ExprKind::Tuple(args) if args.is_empty() => Some(hir::Attr::Measurement),
                _ => {
                    self.lowerer
                        .errors
                        .push(Error::InvalidAttrArgs("()".to_string(), attr.arg.span));
                    None
                }
            },
            Ok(hir::Attr::Reset) => match &*attr.arg.kind {
                ast::ExprKind::Tuple(args) if args.is_empty() => Some(hir::Attr::Reset),
                _ => {
                    self.lowerer
                        .errors
                        .push(Error::InvalidAttrArgs("()".to_string(), attr.arg.span));
                    None
                }
            },
            Ok(hir::Attr::Test) => {
                // verify that no args are passed to the attribute
                match &*attr.arg.kind {
                    ast::ExprKind::Tuple(args) if args.is_empty() => {}
                    _ => {
                        self.lowerer
                            .errors
                            .push(Error::InvalidAttrArgs("()".to_string(), attr.arg.span));
                    }
                }
                // lower the attribute even if it has invalid args
                Some(hir::Attr::Test)
            }
            Err(()) => {
                self.lowerer.errors.push(Error::UnknownAttr(
                    attr.name.name.to_string(),
                    attr.name.span,
                ));
                None
            }
        }
    }

    /// Generates generic parameters for the functors, if there were generics on the original callable.
    /// Basically just creates new generic params for the purpose of being used in functor callable
    /// decls.
    pub(crate) fn synthesize_callable_generics(
        &mut self,
        generics: &[ast::TypeParameter],
        input: &mut hir::Pat,
    ) -> (Vec<qsc_hir::ty::TypeParameter>, Vec<TyConversionError>) {
        let (mut params, errs) = convert::type_parameters_for_ast_callable(self.names, generics);
        let mut functor_params =
            Self::synthesize_functor_params_in_pat(&mut params.len().into(), input);
        params.append(&mut functor_params);
        (params, errs)
    }

    fn synthesize_functor_params_in_pat(
        next_param: &mut ParamId,
        pat: &mut hir::Pat,
    ) -> Vec<TypeParameter> {
        match &mut pat.kind {
            hir::PatKind::Discard | hir::PatKind::Err | hir::PatKind::Bind(_) => {
                synthesize_functor_params(next_param, &mut pat.ty)
            }
            hir::PatKind::Tuple(items) => {
                let mut params = Vec::new();
                for item in &mut *items {
                    params.append(&mut Self::synthesize_functor_params_in_pat(
                        next_param, item,
                    ));
                }
                if !params.is_empty() {
                    pat.ty = Ty::Tuple(items.iter().map(|i| i.ty.clone()).collect());
                }
                params
            }
        }
    }

    pub(super) fn lower_callable_decl(
        &mut self,
        decl: &ast::CallableDecl,
        attrs: &[qsc_hir::hir::Attr],
    ) -> (hir::CallableDecl, Vec<TyConversionError>) {
        let id = self.lower_id(decl.id);
        let kind = self.lower_callable_kind(decl.kind, attrs, decl.name.span);
        let name = self.lower_ident(&decl.name);
        let mut input = self.lower_pat(&decl.input);
        let output = convert::ty_from_ast(self.names, &decl.output, &mut Default::default()).0;
        let (generics, errs) = self.synthesize_callable_generics(&decl.generics, &mut input);
        let functors = convert::ast_callable_functors(decl);

        let (body, adj, ctl, ctl_adj) = match decl.body.as_ref() {
            ast::CallableBody::Block(block) => {
                let body = hir::SpecDecl {
                    id: self.assigner.next_node(),
                    span: decl.span,
                    body: hir::SpecBody::Impl(None, self.lower_block(block)),
                };
                (body, None, None, None)
            }
            ast::CallableBody::Specs(specs) => {
                let body = self.find_spec(specs, ast::Spec::Body).unwrap_or_else(|| {
                    self.lowerer.errors.push(Error::MissingBody(decl.span));
                    hir::SpecDecl {
                        id: self.assigner.next_node(),
                        span: decl.span,
                        body: hir::SpecBody::Gen(hir::SpecGen::Auto),
                    }
                });
                let adj = self.find_spec(specs, ast::Spec::Adj);
                let ctl = self.find_spec(specs, ast::Spec::Ctl);
                let ctl_adj = self.find_spec(specs, ast::Spec::CtlAdj);
                (body, adj, ctl, ctl_adj)
            }
        };

        (
            hir::CallableDecl {
                id,
                span: decl.span,
                kind,
                name,
                generics,
                input,
                output,
                functors,
                body,
                adj,
                ctl,
                ctl_adj,
                attrs: attrs.to_vec(),
            },
            errs,
        )
    }

    fn check_invalid_attrs_on_function(&mut self, attrs: &[hir::Attr], span: Span) {
        const INVALID_ATTRS: [hir::Attr; 2] = [hir::Attr::Measurement, hir::Attr::Reset];

        for invalid_attr in &INVALID_ATTRS {
            if attrs.contains(invalid_attr) {
                self.lowerer.errors.push(Error::InvalidAttrOnFunction(
                    format!("{invalid_attr:?}"),
                    span,
                ));
            }
        }
    }

    fn lower_callable_kind(
        &mut self,
        kind: ast::CallableKind,
        attrs: &[hir::Attr],
        span: Span,
    ) -> hir::CallableKind {
        match kind {
            ast::CallableKind::Function => {
                self.check_invalid_attrs_on_function(attrs, span);
                hir::CallableKind::Function
            }
            ast::CallableKind::Operation => hir::CallableKind::Operation,
        }
    }

    fn find_spec(
        &mut self,
        specs: &[Box<ast::SpecDecl>],
        spec: ast::Spec,
    ) -> Option<hir::SpecDecl> {
        match specs
            .iter()
            .filter(|s| s.spec == spec)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] => None,
            [single] => Some(self.lower_spec_decl(single)),
            dupes => {
                for dup in dupes {
                    self.lowerer.errors.push(Error::DuplicateSpec(dup.span));
                }
                Some(self.lower_spec_decl(dupes[0]))
            }
        }
    }

    fn lower_spec_decl(&mut self, decl: &ast::SpecDecl) -> hir::SpecDecl {
        hir::SpecDecl {
            id: self.lower_id(decl.id),
            span: decl.span,
            body: match &decl.body {
                ast::SpecBody::Gen(spec_gen) => hir::SpecBody::Gen(match spec_gen {
                    ast::SpecGen::Auto => hir::SpecGen::Auto,
                    ast::SpecGen::Distribute => hir::SpecGen::Distribute,
                    ast::SpecGen::Intrinsic => hir::SpecGen::Intrinsic,
                    ast::SpecGen::Invert => hir::SpecGen::Invert,
                    ast::SpecGen::Slf => hir::SpecGen::Slf,
                }),
                ast::SpecBody::Impl(input, block) => {
                    hir::SpecBody::Impl(self.lower_spec_decl_pat(input), self.lower_block(block))
                }
            },
        }
    }

    fn lower_spec_decl_pat(&mut self, pat: &ast::Pat) -> Option<hir::Pat> {
        if let ast::PatKind::Paren(inner) = &*pat.kind {
            return self.lower_spec_decl_pat(inner);
        }

        match &*pat.kind {
            ast::PatKind::Elided => return None,
            ast::PatKind::Tuple(items)
                if items.len() == 2 && *items[1].kind == ast::PatKind::Elided =>
            {
                return Some(self.lower_pat(&items[0]));
            }
            _ => self.lowerer.errors.push(Error::InvalidSpecPat(pat.span)),
        }

        None
    }

    fn lower_block(&mut self, block: &ast::Block) -> hir::Block {
        hir::Block {
            id: self.lower_id(block.id),
            span: block.span,
            ty: self.tys.terms.get(block.id).map_or(Ty::Err, Clone::clone),
            stmts: block
                .stmts
                .iter()
                .flat_map(|s| self.lower_stmt(s))
                .collect(),
        }
    }

    pub(super) fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Vec<hir::Stmt> {
        let id = self.lower_id(stmt.id);
        let mut stmts = Vec::new();
        match &*stmt.kind {
            ast::StmtKind::Empty | ast::StmtKind::Err => {}
            ast::StmtKind::Expr(expr) => stmts.push(hir::StmtKind::Expr(self.lower_expr(expr))),
            ast::StmtKind::Item(item) => {
                stmts.extend(self.lower_item(item).into_iter().map(hir::StmtKind::Item));
            }
            ast::StmtKind::Local(mutability, lhs, rhs) => stmts.push(hir::StmtKind::Local(
                lower_mutability(*mutability),
                self.lower_pat(lhs),
                self.lower_expr(rhs),
            )),
            ast::StmtKind::Qubit(source, lhs, rhs, block) => stmts.push(hir::StmtKind::Qubit(
                match source {
                    ast::QubitSource::Fresh => hir::QubitSource::Fresh,
                    ast::QubitSource::Dirty => hir::QubitSource::Dirty,
                },
                self.lower_pat(lhs),
                self.lower_qubit_init(rhs),
                block.as_ref().map(|b| self.lower_block(b)),
            )),
            ast::StmtKind::Semi(expr) => stmts.push(hir::StmtKind::Semi(self.lower_expr(expr))),
        }

        stmts
            .into_iter()
            .map(|kind| hir::Stmt {
                id,
                span: stmt.span,
                kind,
            })
            .collect()
    }

    #[allow(clippy::too_many_lines)]
    fn lower_expr(&mut self, expr: &ast::Expr) -> hir::Expr {
        if let ast::ExprKind::Paren(inner) = &*expr.kind {
            return self.lower_expr(inner);
        }

        let id = self.lower_id(expr.id);
        let ty = self.tys.terms.get(expr.id).map_or(Ty::Err, Clone::clone);

        let kind = match &*expr.kind {
            ast::ExprKind::Array(items) => {
                hir::ExprKind::Array(items.iter().map(|i| self.lower_expr(i)).collect())
            }
            ast::ExprKind::ArrayRepeat(value, size) => hir::ExprKind::ArrayRepeat(
                Box::new(self.lower_expr(value)),
                Box::new(self.lower_expr(size)),
            ),
            ast::ExprKind::Assign(lhs, rhs) => hir::ExprKind::Assign(
                Box::new(self.lower_expr(lhs)),
                Box::new(self.lower_expr(rhs)),
            ),
            ast::ExprKind::AssignOp(op, lhs, rhs) => hir::ExprKind::AssignOp(
                lower_binop(*op),
                Box::new(self.lower_expr(lhs)),
                Box::new(self.lower_expr(rhs)),
            ),
            ast::ExprKind::AssignUpdate(container, index, replace) => {
                if let Some(field) = resolve::extract_field_name(self.names, index) {
                    let container = self.lower_expr(container);
                    let field = self.lower_field(&container.ty, field);
                    let replace = self.lower_expr(replace);
                    hir::ExprKind::AssignField(Box::new(container), field, Box::new(replace))
                } else {
                    hir::ExprKind::AssignIndex(
                        Box::new(self.lower_expr(container)),
                        Box::new(self.lower_expr(index)),
                        Box::new(self.lower_expr(replace)),
                    )
                }
            }
            ast::ExprKind::BinOp(op, lhs, rhs) => hir::ExprKind::BinOp(
                lower_binop(*op),
                Box::new(self.lower_expr(lhs)),
                Box::new(self.lower_expr(rhs)),
            ),
            ast::ExprKind::Block(block) => hir::ExprKind::Block(self.lower_block(block)),
            ast::ExprKind::Call(callee, arg) => match &ty {
                Ty::Arrow(arrow) if is_partial_app(arg) => hir::ExprKind::Block(
                    self.lower_partial_app(callee, arg, arrow.clone(), expr.span),
                ),
                _ => hir::ExprKind::Call(
                    Box::new(self.lower_expr(callee)),
                    Box::new(self.lower_expr(arg)),
                ),
            },
            ast::ExprKind::Conjugate(within, apply) => {
                hir::ExprKind::Conjugate(self.lower_block(within), self.lower_block(apply))
            }
            ast::ExprKind::Fail(message) => hir::ExprKind::Fail(Box::new(self.lower_expr(message))),
            ast::ExprKind::Field(container, FieldAccess::Ok(name)) => {
                let container = self.lower_expr(container);
                let field = self.lower_field(&container.ty, &name.name);
                hir::ExprKind::Field(Box::new(container), field)
            }
            ast::ExprKind::For(pat, iter, block) => hir::ExprKind::For(
                self.lower_pat(pat),
                Box::new(self.lower_expr(iter)),
                self.lower_block(block),
            ),
            ast::ExprKind::Hole => hir::ExprKind::Hole,
            ast::ExprKind::If(cond, if_true, if_false) => hir::ExprKind::If(
                Box::new(self.lower_expr(cond)),
                Box::new(hir::Expr {
                    id: self.assigner.next_node(),
                    span: if_true.span,
                    ty: self.tys.terms.get(if_true.id).map_or(Ty::Err, Clone::clone),
                    kind: hir::ExprKind::Block(self.lower_block(if_true)),
                }),
                if_false.as_ref().map(|e| Box::new(self.lower_expr(e))),
            ),
            ast::ExprKind::Index(container, index) => hir::ExprKind::Index(
                Box::new(self.lower_expr(container)),
                Box::new(self.lower_expr(index)),
            ),
            ast::ExprKind::Lambda(kind, input, body) => {
                let functors = if let Ty::Arrow(arrow) = &ty {
                    arrow
                        .functors
                        .borrow()
                        .expect_value("lambda type should have concrete functors")
                } else {
                    FunctorSetValue::Empty
                };
                let lambda = Lambda {
                    kind: self.lower_callable_kind(*kind, &[], expr.span),
                    functors,
                    input: self.lower_pat(input),
                    body: self.lower_expr(body),
                };
                self.lower_lambda(lambda, expr.span)
            }
            ast::ExprKind::Lit(lit) => self.lower_lit(lit),
            ast::ExprKind::Paren(_) => unreachable!("parentheses should be removed earlier"),
            ast::ExprKind::Path(PathKind::Ok(path)) => {
                let args = self
                    .tys
                    .generics
                    .get(expr.id)
                    .map_or(Vec::new(), Clone::clone);
                self.lower_path(path, args)
            }
            ast::ExprKind::Range(start, step, end) => hir::ExprKind::Range(
                start.as_ref().map(|s| Box::new(self.lower_expr(s))),
                step.as_ref().map(|s| Box::new(self.lower_expr(s))),
                end.as_ref().map(|e| Box::new(self.lower_expr(e))),
            ),
            ast::ExprKind::Repeat(body, cond, fixup) => hir::ExprKind::Repeat(
                self.lower_block(body),
                Box::new(self.lower_expr(cond)),
                fixup.as_ref().map(|f| self.lower_block(f)),
            ),
            ast::ExprKind::Return(expr) => hir::ExprKind::Return(Box::new(self.lower_expr(expr))),
            ast::ExprKind::Struct(PathKind::Ok(path), copy, fields) => hir::ExprKind::Struct(
                self.path_to_res(path),
                copy.as_ref().map(|c| Box::new(self.lower_expr(c))),
                fields
                    .iter()
                    .map(|f| Box::new(self.lower_field_assign(&ty, f)))
                    .collect(),
            ),
            ast::ExprKind::Interpolate(components) => hir::ExprKind::String(
                components
                    .iter()
                    .map(|c| self.lower_string_component(c))
                    .collect(),
            ),
            ast::ExprKind::TernOp(ast::TernOp::Cond, cond, if_true, if_false) => hir::ExprKind::If(
                Box::new(self.lower_expr(cond)),
                Box::new(self.lower_expr(if_true)),
                Some(Box::new(self.lower_expr(if_false))),
            ),
            ast::ExprKind::TernOp(ast::TernOp::Update, container, index, replace) => {
                if let Some(field) = resolve::extract_field_name(self.names, index) {
                    let record = self.lower_expr(container);
                    let field = self.lower_field(&record.ty, field);
                    let replace = self.lower_expr(replace);
                    hir::ExprKind::UpdateField(Box::new(record), field, Box::new(replace))
                } else {
                    hir::ExprKind::UpdateIndex(
                        Box::new(self.lower_expr(container)),
                        Box::new(self.lower_expr(index)),
                        Box::new(self.lower_expr(replace)),
                    )
                }
            }
            ast::ExprKind::Tuple(items) => {
                hir::ExprKind::Tuple(items.iter().map(|i| self.lower_expr(i)).collect())
            }
            ast::ExprKind::UnOp(op, operand) => {
                hir::ExprKind::UnOp(lower_unop(*op), Box::new(self.lower_expr(operand)))
            }
            ast::ExprKind::While(cond, body) => {
                hir::ExprKind::While(Box::new(self.lower_expr(cond)), self.lower_block(body))
            }
            ast::ExprKind::Err
            | &ast::ExprKind::Path(ast::PathKind::Err(_))
            | ast::ExprKind::Struct(ast::PathKind::Err(_), ..)
            | ast::ExprKind::Field(_, FieldAccess::Err) => hir::ExprKind::Err,
        };

        hir::Expr {
            id,
            span: expr.span,
            ty,
            kind,
        }
    }

    fn lower_field_assign(&mut self, ty: &Ty, field_assign: &ast::FieldAssign) -> hir::FieldAssign {
        hir::FieldAssign {
            id: self.lower_id(field_assign.id),
            span: field_assign.span,
            field: self.lower_field(ty, &field_assign.field.name),
            value: Box::new(self.lower_expr(&field_assign.value)),
        }
    }

    fn lower_partial_app(
        &mut self,
        callee: &ast::Expr,
        arg: &ast::Expr,
        arrow: Rc<Arrow>,
        span: Span,
    ) -> hir::Block {
        let callee = self.lower_expr(callee);
        let (arg, app) = self.lower_partial_arg(arg);
        let close = |mut lambda: Lambda| {
            self.assigner.visit_expr(&mut lambda.body);
            self.lower_lambda(lambda, span)
        };

        let mut block = closure::partial_app_block(close, callee, arg, app, arrow, span);
        self.assigner.visit_block(&mut block);
        block
    }

    fn lower_partial_arg(&mut self, arg: &ast::Expr) -> (hir::Expr, PartialApp) {
        match arg.kind.as_ref() {
            ast::ExprKind::Hole => {
                let ty = self.tys.terms.get(arg.id).map_or(Ty::Err, Clone::clone);
                closure::partial_app_hole(self.assigner, &mut self.lowerer.locals, ty, arg.span)
            }
            ast::ExprKind::Paren(inner) => self.lower_partial_arg(inner),
            ast::ExprKind::Tuple(items) => {
                let items = items.iter().map(|item| self.lower_partial_arg(item));
                let (mut arg, mut app) = closure::partial_app_tuple(items, arg.span);
                self.assigner.visit_expr(&mut arg);
                self.assigner.visit_pat(&mut app.input);
                (arg, app)
            }
            _ => {
                let arg = self.lower_expr(arg);
                closure::partial_app_given(self.assigner, &mut self.lowerer.locals, arg)
            }
        }
    }

    fn lower_lambda(&mut self, lambda: Lambda, span: Span) -> hir::ExprKind {
        let (args, callable) = closure::lift(self.assigner, &self.lowerer.locals, lambda, span);

        let id = self.assigner.next_item();
        self.lowerer.items.push(hir::Item {
            id,
            span,
            parent: self.lowerer.parent,
            doc: "".into(),
            attrs: Vec::new(),
            visibility: hir::Visibility::Internal,
            kind: hir::ItemKind::Callable(callable.into()),
        });

        hir::ExprKind::Closure(args, id)
    }

    fn lower_field(&mut self, record_ty: &Ty, name: &str) -> hir::Field {
        if let Ty::Udt(_, hir::Res::Item(id)) = record_ty {
            self.tys
                .udts
                .get(id)
                .and_then(|udt| udt.field_path(name))
                .map_or(hir::Field::Err, hir::Field::Path)
        } else if let Ok(prim) = name.parse() {
            hir::Field::Prim(prim)
        } else {
            hir::Field::Err
        }
    }

    fn lower_string_component(&mut self, component: &ast::StringComponent) -> hir::StringComponent {
        match component {
            ast::StringComponent::Expr(expr) => {
                hir::StringComponent::Expr(self.lower_expr(expr).into())
            }
            ast::StringComponent::Lit(str) => hir::StringComponent::Lit(Rc::clone(str)),
        }
    }

    fn lower_pat(&mut self, pat: &ast::Pat) -> hir::Pat {
        if let ast::PatKind::Paren(inner) = &*pat.kind {
            return self.lower_pat(inner);
        }

        let id = self.lower_id(pat.id);
        let ty = self
            .tys
            .terms
            .get(pat.id)
            .map_or_else(|| convert::ast_pat_ty(self.names, pat).0, Clone::clone);

        let kind = match &*pat.kind {
            ast::PatKind::Bind(name, _) => {
                let name = self.lower_ident(name);
                self.lowerer
                    .locals
                    .insert(name.id, (name.clone(), ty.clone()));
                hir::PatKind::Bind(name)
            }
            ast::PatKind::Discard(_) => hir::PatKind::Discard,
            ast::PatKind::Elided => {
                self.lowerer.errors.push(Error::InvalidElidedPat(pat.span));
                hir::PatKind::Discard
            }
            ast::PatKind::Paren(_) => unreachable!("parentheses should be removed earlier"),
            ast::PatKind::Tuple(items) => {
                hir::PatKind::Tuple(items.iter().map(|i| self.lower_pat(i)).collect())
            }
            ast::PatKind::Err => hir::PatKind::Err,
        };

        hir::Pat {
            id,
            span: pat.span,
            ty,
            kind,
        }
    }

    fn lower_qubit_init(&mut self, init: &ast::QubitInit) -> hir::QubitInit {
        if let ast::QubitInitKind::Paren(inner) = &*init.kind {
            return self.lower_qubit_init(inner);
        }

        let id = self.lower_id(init.id);
        let ty = self.tys.terms.get(init.id).map_or(Ty::Err, Clone::clone);
        let kind = match &*init.kind {
            ast::QubitInitKind::Array(length) => {
                hir::QubitInitKind::Array(Box::new(self.lower_expr(length)))
            }
            ast::QubitInitKind::Paren(_) => unreachable!("parentheses should be removed earlier"),
            ast::QubitInitKind::Single => hir::QubitInitKind::Single,
            ast::QubitInitKind::Tuple(items) => {
                hir::QubitInitKind::Tuple(items.iter().map(|i| self.lower_qubit_init(i)).collect())
            }
            ast::QubitInitKind::Err => hir::QubitInitKind::Err,
        };

        hir::QubitInit {
            id,
            span: init.span,
            ty,
            kind,
        }
    }

    fn path_to_res(&mut self, path: &ast::Path) -> hir::Res {
        match self.names.get(path.id) {
            Some(&resolve::Res::Item(item, _)) => hir::Res::Item(item),
            Some(&resolve::Res::Local(node)) => hir::Res::Local(self.lower_id(node)),
            Some(&resolve::Res::Importable(
                resolve::Importable::Callable(item_id, _) | resolve::Importable::Ty(item_id, _),
                ..,
            )) => hir::Res::Item(item_id),
            Some(&resolve::Res::Importable(resolve::Importable::Namespace(_, Some(item_id)))) => {
                if item_id.package.is_some() {
                    // This is a namespace from an external package, and reexporting is
                    // disallowed since it has no meaningful effect.
                    self.lowerer
                        .errors
                        .push(Error::CrossPackageNamespaceReexport(path.span));
                    hir::Res::Err
                } else {
                    hir::Res::Item(item_id)
                }
            }
            Some(&resolve::Res::Importable(resolve::Importable::Namespace(_, None))) => {
                self.lowerer
                    .errors
                    .push(Error::ParentNamespaceExport { span: path.span });
                hir::Res::Err
            }
            Some(resolve::Res::PrimTy(_) | resolve::Res::UnitTy | resolve::Res::Param { .. })
            | None => hir::Res::Err,
        }
    }

    fn lower_path(&mut self, path: &ast::Path, generic_args: Vec<GenericArg>) -> hir::ExprKind {
        match resolve::path_as_field_accessor(self.names, path) {
            Some((first_id, parts)) => {
                let res = hir::Res::Local(self.lower_id(first_id));
                self.path_parts_to_fields(hir::ExprKind::Var(res, Vec::new()), &parts, path.span.lo)
            }
            None => hir::ExprKind::Var(self.path_to_res(path), generic_args),
        }
    }

    // Lowers the parts of a field accessor Path into nested Field Accessor nodes.
    fn path_parts_to_fields(
        &mut self,
        init_kind: hir::ExprKind,
        parts: &[&Ident],
        lo: u32,
    ) -> hir::ExprKind {
        let (first, rest) = parts
            .split_first()
            .expect("path should have at least one part");

        let mut kind = init_kind;
        let mut prev = first;
        for part in rest {
            let prev_expr = hir::Expr {
                id: self.assigner.next_node(),
                span: Span {
                    lo,
                    hi: prev.span.hi,
                },
                // The ids of the Ident segments are specially mapped in the tys to give us the type of the expressions being created here.
                ty: self.tys.terms.get(prev.id).map_or(Ty::Err, Clone::clone),
                kind,
            };
            let field = self.lower_field(&prev_expr.ty, &part.name);
            kind = hir::ExprKind::Field(Box::new(prev_expr), field);
            prev = part;
        }
        kind
    }

    fn lower_ident(&mut self, ident: &ast::Ident) -> hir::Ident {
        hir::Ident {
            id: self.lower_id(ident.id),
            span: ident.span,
            name: ident.name.clone(),
        }
    }

    fn lower_id(&mut self, id: ast::NodeId) -> hir::NodeId {
        self.lowerer.nodes.get(id).copied().unwrap_or_else(|| {
            let new_id = self.assigner.next_node();
            self.lowerer.nodes.insert(id, new_id);
            new_id
        })
    }

    fn lower_idents(&mut self, name: &impl Idents) -> hir::Idents {
        name.iter().map(|i| self.lower_ident(i)).collect()
    }

    fn lower_lit(&mut self, lit: &ast::Lit) -> hir::ExprKind {
        match lit {
            ast::Lit::BigInt(value) => hir::ExprKind::Lit(hir::Lit::BigInt(value.as_ref().clone())),
            &ast::Lit::Bool(value) => hir::ExprKind::Lit(hir::Lit::Bool(value)),
            &ast::Lit::Double(value) => hir::ExprKind::Lit(hir::Lit::Double(value)),
            &ast::Lit::Imaginary(value) => hir::ExprKind::Struct(
                hir::Res::Item(ItemId::complex()),
                None,
                Box::new([
                    Box::new(hir::FieldAssign {
                        id: self.assigner.next_node(),
                        span: Span::default(),
                        field: hir::Field::Path({
                            let mut path = hir::FieldPath::default();
                            path.indices.insert(0, 0);
                            path
                        }),
                        value: Box::new(hir::Expr {
                            id: self.assigner.next_node(),
                            span: Span::default(),
                            ty: Ty::Prim(qsc_hir::ty::Prim::Double),
                            kind: hir::ExprKind::Lit(hir::Lit::Double(0.0)),
                        }),
                    }),
                    Box::new(hir::FieldAssign {
                        id: self.assigner.next_node(),
                        span: Span::default(),
                        field: hir::Field::Path({
                            let mut path = hir::FieldPath::default();
                            path.indices.insert(0, 1);
                            path
                        }),
                        value: Box::new(hir::Expr {
                            id: self.assigner.next_node(),
                            span: Span::default(),
                            ty: Ty::Prim(qsc_hir::ty::Prim::Double),
                            kind: hir::ExprKind::Lit(hir::Lit::Double(value)),
                        }),
                    }),
                ]),
            ),
            &ast::Lit::Int(value) => hir::ExprKind::Lit(hir::Lit::Int(value)),
            ast::Lit::Pauli(ast::Pauli::I) => hir::ExprKind::Lit(hir::Lit::Pauli(hir::Pauli::I)),
            ast::Lit::Pauli(ast::Pauli::X) => hir::ExprKind::Lit(hir::Lit::Pauli(hir::Pauli::X)),
            ast::Lit::Pauli(ast::Pauli::Y) => hir::ExprKind::Lit(hir::Lit::Pauli(hir::Pauli::Y)),
            ast::Lit::Pauli(ast::Pauli::Z) => hir::ExprKind::Lit(hir::Lit::Pauli(hir::Pauli::Z)),
            ast::Lit::Result(ast::Result::One) => {
                hir::ExprKind::Lit(hir::Lit::Result(hir::Result::One))
            }
            ast::Lit::Result(ast::Result::Zero) => {
                hir::ExprKind::Lit(hir::Lit::Result(hir::Result::Zero))
            }
            ast::Lit::String(value) => {
                hir::ExprKind::String(vec![hir::StringComponent::Lit(Rc::clone(value))])
            }
        }
    }
}

/// Removes all self-export items, and makes the corresponding item declarations public.
///
/// Self-exports are exports that refer to items in the same namespace
/// with the same name. e.g.:
///
/// ```qsharp
/// namespace A {
///     operation B() {} : Unit {}
///     export B;
/// }
/// ```
///
/// These exports essentially serve to make the original item public, and don't need
/// to be lowered as items of their own. In fact, lowering them would result in two
/// items with the same name in the same namespace.
fn collapse_self_exports(items: &mut IndexMap<LocalItemId, hir::Item>) {
    let mut to_export = Vec::new();
    for (id, item) in &*items {
        if let hir::ItemKind::Export(name, Res::Item(original_item_id)) = &item.kind {
            if original_item_id.package.is_none() {
                let original_item_id = original_item_id.item;
                let original_item = items
                    .get(original_item_id)
                    .expect("expected to resolve item id");
                if let Some(parent_id) = item.parent {
                    let same_namespace = original_item.parent == item.parent;
                    let same_name = same_namespace
                        && match &original_item.kind {
                            hir::ItemKind::Callable(callable_decl) => {
                                callable_decl.name.name == name.name
                            }
                            hir::ItemKind::Ty(ident, _) => ident.name == name.name,
                            _ => false,
                        };
                    if same_name {
                        to_export.push((parent_id, id, original_item_id));
                    }
                }
            }
        }
    }

    for (parent_id, export_item_id, original_item_id) in to_export {
        // remove the export item
        items.remove(export_item_id);
        // remove the export item from its parent
        if let Some(parent_item) = items.get_mut(parent_id) {
            if let hir::ItemKind::Namespace(_, local_item_ids) = &mut parent_item.kind {
                local_item_ids.retain(|&id| id != export_item_id);
            }
        }
        // make the original item public
        items
            .get_mut(original_item_id)
            .expect("expected to resolve item id")
            .visibility = Visibility::Public;
    }
}

fn lower_mutability(mutability: ast::Mutability) -> hir::Mutability {
    match mutability {
        ast::Mutability::Immutable => hir::Mutability::Immutable,
        ast::Mutability::Mutable => hir::Mutability::Mutable,
    }
}

fn lower_unop(op: ast::UnOp) -> hir::UnOp {
    match op {
        ast::UnOp::Functor(f) => hir::UnOp::Functor(lower_functor(f)),
        ast::UnOp::Neg => hir::UnOp::Neg,
        ast::UnOp::NotB => hir::UnOp::NotB,
        ast::UnOp::NotL => hir::UnOp::NotL,
        ast::UnOp::Pos => hir::UnOp::Pos,
        ast::UnOp::Unwrap => hir::UnOp::Unwrap,
    }
}

fn lower_binop(op: ast::BinOp) -> hir::BinOp {
    match op {
        ast::BinOp::Add => hir::BinOp::Add,
        ast::BinOp::AndB => hir::BinOp::AndB,
        ast::BinOp::AndL => hir::BinOp::AndL,
        ast::BinOp::Div => hir::BinOp::Div,
        ast::BinOp::Eq => hir::BinOp::Eq,
        ast::BinOp::Exp => hir::BinOp::Exp,
        ast::BinOp::Gt => hir::BinOp::Gt,
        ast::BinOp::Gte => hir::BinOp::Gte,
        ast::BinOp::Lt => hir::BinOp::Lt,
        ast::BinOp::Lte => hir::BinOp::Lte,
        ast::BinOp::Mod => hir::BinOp::Mod,
        ast::BinOp::Mul => hir::BinOp::Mul,
        ast::BinOp::Neq => hir::BinOp::Neq,
        ast::BinOp::OrB => hir::BinOp::OrB,
        ast::BinOp::OrL => hir::BinOp::OrL,
        ast::BinOp::Shl => hir::BinOp::Shl,
        ast::BinOp::Shr => hir::BinOp::Shr,
        ast::BinOp::Sub => hir::BinOp::Sub,
        ast::BinOp::XorB => hir::BinOp::XorB,
    }
}

fn lower_functor(functor: ast::Functor) -> hir::Functor {
    match functor {
        ast::Functor::Adj => hir::Functor::Adj,
        ast::Functor::Ctl => hir::Functor::Ctl,
    }
}

fn is_partial_app(arg: &ast::Expr) -> bool {
    match arg.kind.as_ref() {
        ast::ExprKind::Hole => true,
        ast::ExprKind::Paren(inner) => is_partial_app(inner),
        ast::ExprKind::Tuple(items) => items.iter().any(|i| is_partial_app(i)),
        _ => false,
    }
}
