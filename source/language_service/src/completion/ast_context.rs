// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc::{
    ast::{
        self, Attr, Block, CallableDecl, Expr, ExprKind, FieldAssign, FieldDef, FunctorExpr, Ident,
        Idents, ImportKind, Item, ItemKind, Namespace, NodeId, Package, Pat, Path, QubitInit,
        SpecDecl, Stmt, StructDecl, Ty, TyDef, TyKind,
        visit::{self, Visitor},
    },
    parse::completion::PathKind,
};
use std::rc::Rc;

/// Provides additional syntactic context for the cursor offset,
/// for various special cases where it's needed.
#[derive(Debug)]
pub(super) struct AstContext<'a> {
    path_qualifier: Option<Vec<&'a Ident>>,
    context: Option<Context<'a>>,
    offset: u32,
}

#[derive(Debug, Clone)]
enum Context<'a> {
    /// The cursor is on a path or incomplete path.
    Path(PathKind),
    /// The cursor is on a field access or a field assignment expression.
    Field {
        /// The type of this expression will be the record used for the field access.
        record: &'a Expr,
    },
    /// The cursor is on an attribute.
    AttrArg(Rc<str>),
}

impl<'a> AstContext<'a> {
    pub fn init(offset: u32, package: &'a Package) -> Self {
        let mut offset_visitor = OffsetVisitor {
            offset,
            visitor: AstContext {
                offset,
                context: None,
                path_qualifier: None,
            },
        };

        offset_visitor.visit_package(package);

        offset_visitor.visitor
    }

    fn set_context(&mut self, context: Context<'a>) {
        // Ignores update of context if the cursor is in an attribute argument
        if matches!(self.context, Some(Context::AttrArg(_))) {
            return;
        }
        self.context = Some(context);
    }
}

impl<'a> Visitor<'a> for AstContext<'a> {
    fn visit_item(&mut self, item: &'a Item) {
        match &*item.kind {
            ItemKind::Open(..) => self.set_context(Context::Path(PathKind::Namespace)),
            ItemKind::ImportOrExport(decl) => {
                self.set_context(Context::Path(PathKind::Import));
                for item in &decl.items {
                    if ImportKind::Wildcard == item.kind && item.span.contains(self.offset) {
                        // Special case when the cursor falls *between* the
                        // `Path` and the glob asterisk,
                        // e.g. `foo.bar.|*` . In that case, the visitor
                        // will not visit the path since the cursor technically
                        // is not within the path.
                        self.visit_path_kind(&item.path);
                    }
                }
            }
            _ => {}
        }
    }

    fn visit_ty(&mut self, ty: &Ty) {
        if let TyKind::Path(..) = *ty.kind {
            self.set_context(Context::Path(PathKind::Ty));
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Path(..) = *expr.kind {
            self.set_context(Context::Path(PathKind::Expr));
        } else if let ExprKind::Struct(..) = expr.kind.as_ref() {
            self.set_context(Context::Field { record: expr });
        } else if let ExprKind::Field(record, _) = expr.kind.as_ref() {
            self.set_context(Context::Field { record });
        }
    }

    fn visit_path_kind(&mut self, path: &'a ast::PathKind) {
        if let Some(Context::Field { .. }) = self.context {
            self.set_context(Context::Path(PathKind::Struct));
        }
        self.path_qualifier = match path {
            ast::PathKind::Ok(path) => Some(path.iter().collect()),
            ast::PathKind::Err(Some(incomplete_path)) => {
                Some(incomplete_path.segments.iter().collect())
            }
            ast::PathKind::Err(None) => None,
        };
    }

    fn visit_attr(&mut self, attr: &'a Attr) {
        if attr.arg.span.contains(self.offset) {
            self.set_context(Context::AttrArg(attr.name.name.clone()));
        }
    }
}

impl AstContext<'_> {
    /// Returns the path kind and the path qualifier before the cursor offset.
    ///
    /// Returns `None` if the cursor is not on a path.
    pub fn path_segment_context(&self) -> Option<(PathKind, Vec<Rc<str>>)> {
        let Some(Context::Path(path_kind)) = self.context else {
            return None;
        };

        let qualifier = self.segments_before_offset();

        if qualifier.is_empty() {
            return None;
        }

        Some((path_kind, qualifier))
    }

    /// Returns the node ID of the node that the field access is being performed on, if any.
    /// When this is a field assignment, returns the node ID of the struct expression.
    ///
    /// This node ID can then be used to look up the type to get field names.
    ///
    /// Returns `None` if the cursor is not on a field access expression or path.
    pub fn field_access_context(&self) -> Option<NodeId> {
        if let Some(Context::Field { record }) = &self.context {
            // Unambiguously a field access expression, record node is an `Expr`
            Some(record.id)
        } else {
            // A `Path` that may or may not be a field access expression,
            // record node is the last identifier before the cursor
            self.idents_before_cursor().last().map(|ident| ident.id)
        }
    }

    /// Returns the name of the attribute, if the cursor is on an attribute argument.
    /// If the cursor is not on an attribute argument, returns `None`.
    pub fn get_name_of_attr_for_attr_arg(&self) -> Option<Rc<str>> {
        if let Some(Context::AttrArg(name)) = &self.context {
            Some(name.clone())
        } else {
            None
        }
    }

    fn idents_before_cursor(&self) -> impl Iterator<Item = &Ident> {
        self.path_qualifier
            .iter()
            .flatten()
            .copied()
            .take_while(|i| i.span.hi < self.offset)
    }

    fn segments_before_offset(&self) -> Vec<Rc<str>> {
        self.idents_before_cursor()
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
    }
}

/// A [`Visitor`] wrapper that only descends into a node
/// if the given offset falls within that node.
struct OffsetVisitor<T> {
    offset: u32,
    visitor: T,
}

impl<'a, T> Visitor<'a> for OffsetVisitor<T>
where
    T: Visitor<'a>,
{
    fn visit_namespace(&mut self, namespace: &'a Namespace) {
        if namespace.span.touches(self.offset) {
            self.visitor.visit_namespace(namespace);
            visit::walk_namespace(self, namespace);
        }
    }

    fn visit_item(&mut self, item: &'a Item) {
        if item.span.touches(self.offset) {
            self.visitor.visit_item(item);
            visit::walk_item(self, item);
        }
    }

    fn visit_attr(&mut self, attr: &'a Attr) {
        if attr.span.touches(self.offset) {
            self.visitor.visit_attr(attr);
            visit::walk_attr(self, attr);
        }
    }

    fn visit_ty_def(&mut self, def: &'a TyDef) {
        if def.span.touches(self.offset) {
            self.visitor.visit_ty_def(def);
            visit::walk_ty_def(self, def);
        }
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        if decl.span.touches(self.offset) {
            self.visitor.visit_callable_decl(decl);
            visit::walk_callable_decl(self, decl);
        }
    }

    fn visit_struct_decl(&mut self, decl: &'a StructDecl) {
        if decl.span.touches(self.offset) {
            self.visitor.visit_struct_decl(decl);
            visit::walk_struct_decl(self, decl);
        }
    }

    fn visit_field_def(&mut self, def: &'a FieldDef) {
        if def.span.touches(self.offset) {
            self.visitor.visit_field_def(def);
            visit::walk_field_def(self, def);
        }
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        if decl.span.touches(self.offset) {
            self.visitor.visit_spec_decl(decl);
            visit::walk_spec_decl(self, decl);
        }
    }

    fn visit_functor_expr(&mut self, expr: &'a FunctorExpr) {
        if expr.span.touches(self.offset) {
            self.visitor.visit_functor_expr(expr);
            visit::walk_functor_expr(self, expr);
        }
    }

    fn visit_ty(&mut self, ty: &'a Ty) {
        if ty.span.touches(self.offset) {
            self.visitor.visit_ty(ty);
            visit::walk_ty(self, ty);
        }
    }

    fn visit_block(&mut self, block: &'a Block) {
        if block.span.touches(self.offset) {
            self.visitor.visit_block(block);
            visit::walk_block(self, block);
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if stmt.span.touches(self.offset) {
            self.visitor.visit_stmt(stmt);
            visit::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.span.touches(self.offset) {
            self.visitor.visit_expr(expr);
            visit::walk_expr(self, expr);
        }
    }

    fn visit_field_assign(&mut self, assign: &'a FieldAssign) {
        if assign.span.touches(self.offset) {
            self.visitor.visit_field_assign(assign);
            visit::walk_field_assign(self, assign);
        }
    }

    fn visit_pat(&mut self, pat: &'a Pat) {
        if pat.span.touches(self.offset) {
            self.visitor.visit_pat(pat);
            visit::walk_pat(self, pat);
        }
    }

    fn visit_qubit_init(&mut self, init: &'a QubitInit) {
        if init.span.touches(self.offset) {
            self.visitor.visit_qubit_init(init);
            visit::walk_qubit_init(self, init);
        }
    }

    fn visit_path(&mut self, path: &'a Path) {
        if path.span.touches(self.offset) {
            self.visitor.visit_path(path);
            visit::walk_path(self, path);
        }
    }

    fn visit_path_kind(&mut self, path: &'a ast::PathKind) {
        let span = match path {
            ast::PathKind::Ok(path) => &path.span,
            ast::PathKind::Err(Some(incomplete_path)) => &incomplete_path.span,
            ast::PathKind::Err(None) => return,
        };

        if span.touches(self.offset) {
            self.visitor.visit_path_kind(path);
            visit::walk_path_kind(self, path);
        }
    }

    fn visit_ident(&mut self, ident: &'a Ident) {
        if ident.span.touches(self.offset) {
            self.visitor.visit_ident(ident);
        }
    }
}
