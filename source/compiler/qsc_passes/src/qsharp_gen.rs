// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! HIR-to-Q# emitter for pass-debugging snapshots.
//!
//! Walks a transformed HIR [`Package`] and writes lexically valid Q# with
//! minimal whitespace, then runs [`qsc_formatter::formatter::format_str`]
//! over the raw output so the layout is human-readable. It is the HIR analog
//! of `FirQSharpGen` in `qsc_fir_transforms::pretty`, which walks the arena
//! based FIR; because HIR embeds its children directly, this emitter walks
//! `&Expr`/`&Stmt`/`&Block` references without any store indirection.
//!
//! The emitter is intended for before/after snapshot tests of HIR transform
//! passes such as `loop_normalize` and `loop_unification`. It is a
//! general-purpose HIR-to-Q# renderer covering every `ExprKind`, so both the
//! input and the output of a pass render faithfully. The rendered output is
//! meant for human review, not for re-compilation.
//!
//! # Borrow strategy
//!
//! Walking the HIR requires shared borrows into the package while also mutating
//! the output buffer. Rather than cloning each node at the traversal boundary,
//! as the FIR emitter does, this emitter copies the `&'a Package` out of
//! `self`, since a `&Package` is `Copy`, so the resulting child references live
//! for `'a` and are independent of the `&mut self` borrow used to push to
//! `output`.

use qsc_formatter::formatter::format_str;
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir::{
    BinOp, Block, CallableDecl, CallableKind, Expr, ExprKind, Field, FieldAssign, FieldPath,
    Functor, Item, ItemId, ItemKind, Lit, LocalItemId, Mutability, NodeId, Package, Pat, PatKind,
    Pauli, PrimField, QubitInit, QubitInitKind, QubitSource, Res, Result as HirResult, SpecBody,
    SpecDecl, Stmt, StmtKind, StringComponent, UnOp,
};
use qsc_hir::ty::{FunctorSetValue, Ty, Udt, UdtDefKind};
use qsc_hir::visit::{Visitor, walk_pat};
use rustc_hash::FxHashMap;
use std::fmt::Write as _;
use std::rc::Rc;

/// Renders a transformed HIR package as Q# source.
///
/// The `package` is passed directly rather than looked up from the `store`
/// because a transform pass mutates a unit's package in place without
/// re-inserting it; the `store` is used only to resolve the names of items in
/// other packages, such as core-library callables.
#[must_use]
pub(crate) fn write_package_qsharp<'a>(store: &'a PackageStore, package: &'a Package) -> String {
    let mut emitter = HirQSharpGen::new(store, package);
    emitter.emit_package();
    format_str(&emitter.output)
}

/// Renders a single HIR expression as Q# source.
///
/// See [`write_package_qsharp`] for the meaning of `store` and `package`.
///
/// This expression-level entry point mirrors [`write_package_qsharp`] for
/// sub-node snapshots; it is retained as part of the emitter surface even
/// though the current in-crate snapshot tests only render whole packages.
#[must_use]
#[allow(dead_code)]
pub(crate) fn write_expr_qsharp<'a>(
    store: &'a PackageStore,
    package: &'a Package,
    expr: &'a Expr,
) -> String {
    let mut emitter = HirQSharpGen::new(store, package);
    emitter.emit_expr(expr);
    format_str(&emitter.output)
}

struct HirQSharpGen<'a> {
    output: String,
    package: &'a Package,
    store: &'a PackageStore,
    local_names: FxHashMap<NodeId, Rc<str>>,
}

impl<'a> HirQSharpGen<'a> {
    fn new(store: &'a PackageStore, package: &'a Package) -> Self {
        Self {
            output: String::new(),
            package,
            store,
            local_names: FxHashMap::default(),
        }
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn writeln(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    /// Removes a trailing newline from the output, if present, so a block-valued
    /// operand can be followed by more tokens on the same line, such as the
    /// ` w/ ... <- ...` continuation of an update expression.
    fn trim_trailing_newline(&mut self) {
        if self.output.ends_with('\n') {
            self.output.pop();
        }
    }

    fn emit_package(&mut self) {
        let package = self.package;
        for item in package.items.values() {
            self.emit_item(item);
        }
        if let Some(entry) = package.entry.as_ref() {
            self.writeln("// entry");
            self.emit_expr(entry);
            self.writeln("");
        }
    }

    fn emit_item(&mut self, item: &'a Item) {
        match &item.kind {
            ItemKind::Callable(decl) => self.emit_callable_decl(decl),
            ItemKind::Ty(name, _) => {
                self.write("// newtype ");
                self.write(&sanitize_ident(&name.name));
                self.writeln("");
            }
            ItemKind::Namespace(..) | ItemKind::Export(..) => {}
        }
    }

    fn emit_callable_decl(&mut self, decl: &'a CallableDecl) {
        let mut collector = LocalNameCollector {
            names: FxHashMap::default(),
        };
        collector.visit_callable_decl(decl);
        self.local_names = collector.names;

        match decl.kind {
            CallableKind::Function => self.write("function "),
            CallableKind::Operation => self.write("operation "),
        }
        self.write(&sanitize_ident(&decl.name.name));
        // Generic parameters are omitted: Q# has no call-site type-argument
        // grammar, so rendering them would not round-trip.
        self.emit_callable_input_pat(&decl.input);
        self.write(" : ");
        self.write(&decl.output.display());
        if decl.functors != FunctorSetValue::Empty {
            self.write(" is ");
            self.write(functor_set_value_as_str(decl.functors));
        }

        let has_specs = decl.adj.is_some() || decl.ctl.is_some() || decl.ctl_adj.is_some();
        if let (SpecBody::Impl(_, block), false) = (&decl.body.body, has_specs) {
            // Single body with no other specializations: emit just the block,
            // which is the parseable shorthand for `body ... { ... }`.
            self.emit_block(block);
        } else {
            self.writeln(" {");
            self.emit_spec_decl("body", &decl.body);
            if let Some(spec) = decl.adj.as_ref() {
                self.emit_spec_decl("adjoint", spec);
            }
            if let Some(spec) = decl.ctl.as_ref() {
                self.emit_spec_decl("controlled", spec);
            }
            if let Some(spec) = decl.ctl_adj.as_ref() {
                self.emit_spec_decl("controlled adjoint", spec);
            }
            self.writeln("}");
        }
    }

    fn emit_spec_decl(&mut self, label: &str, spec: &'a SpecDecl) {
        self.write(label);
        match &spec.body {
            SpecBody::Impl(_, block) => self.emit_block(block),
            SpecBody::Gen(_) => self.writeln(" { ... }"),
        }
    }

    fn emit_callable_input_pat(&mut self, pat: &'a Pat) {
        if matches!(pat.kind, PatKind::Tuple(_)) {
            self.emit_pat_typed(pat);
        } else {
            self.write("(");
            self.emit_pat_typed(pat);
            self.write(")");
        }
    }

    fn emit_pat_typed(&mut self, pat: &'a Pat) {
        match &pat.kind {
            PatKind::Bind(ident) => {
                self.write(&sanitize_ident(&ident.name));
                self.write(" : ");
                self.write(&pat.ty.display());
            }
            PatKind::Discard => {
                self.write("_ : ");
                self.write(&pat.ty.display());
            }
            PatKind::Tuple(pats) => {
                self.write("(");
                for (i, p) in pats.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_pat_typed(p);
                }
                if pats.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            PatKind::Err => self.write("/* err */"),
        }
    }

    fn emit_pat_binding(&mut self, pat: &'a Pat) {
        match &pat.kind {
            PatKind::Bind(ident) => self.write(&sanitize_ident(&ident.name)),
            PatKind::Discard => self.write("_"),
            PatKind::Tuple(pats) => {
                self.write("(");
                for (i, p) in pats.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_pat_binding(p);
                }
                if pats.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            PatKind::Err => self.write("/* err */"),
        }
    }

    fn emit_block(&mut self, block: &'a Block) {
        self.writeln(" {");
        for stmt in &block.stmts {
            self.emit_stmt(stmt);
        }
        self.writeln("}");
    }

    fn emit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.kind {
            StmtKind::Expr(e) => {
                self.emit_expr(e);
                // A block-terminated expression already ends with the block's
                // closing `}` and its trailing newline, so adding another line
                // terminator here would leave a blank line after the block.
                if !expr_ends_with_block(&e.kind) {
                    self.writeln("");
                }
            }
            StmtKind::Semi(e) => {
                self.emit_expr(e);
                self.writeln(";");
            }
            StmtKind::Local(mutability, pat, expr) => {
                match mutability {
                    Mutability::Immutable => self.write("let "),
                    Mutability::Mutable => self.write("mutable "),
                }
                self.emit_pat_binding(pat);
                self.write(" = ");
                self.emit_expr(expr);
                self.writeln(";");
            }
            StmtKind::Qubit(source, pat, init, block) => {
                match source {
                    QubitSource::Fresh => self.write("use "),
                    QubitSource::Dirty => self.write("borrow "),
                }
                self.emit_pat_binding(pat);
                self.write(" = ");
                self.emit_qubit_init(init);
                if let Some(block) = block {
                    self.emit_block(block);
                } else {
                    self.writeln(";");
                }
            }
            StmtKind::Item(item_id) => {
                self.write("// item ");
                self.write(&format!("{item_id}"));
                self.writeln("");
            }
        }
    }

    fn emit_qubit_init(&mut self, init: &'a QubitInit) {
        match &init.kind {
            QubitInitKind::Single => self.write("Qubit()"),
            QubitInitKind::Array(len) => {
                self.write("Qubit[");
                self.emit_expr(len);
                self.write("]");
            }
            QubitInitKind::Tuple(inits) => {
                self.write("(");
                for (i, qi) in inits.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.emit_qubit_init(qi);
                }
                if inits.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            QubitInitKind::Err => self.write("/* err */"),
        }
    }

    fn emit_expr(&mut self, expr: &'a Expr) {
        self.emit_expr_kind(&expr.kind);
    }

    #[allow(clippy::too_many_lines)]
    fn emit_expr_kind(&mut self, kind: &'a ExprKind) {
        match kind {
            ExprKind::Array(exprs) => {
                self.write("[");
                self.emit_exprs_comma(exprs);
                self.write("]");
            }
            ExprKind::Assign(lhs, rhs) => {
                self.emit_expr(lhs);
                self.write(" = ");
                self.emit_expr(rhs);
            }
            ExprKind::AssignOp(op, lhs, rhs) => {
                self.emit_expr(lhs);
                self.write(" ");
                self.write(binop_as_str(*op));
                self.write("= ");
                self.emit_expr(rhs);
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                self.emit_expr(lhs);
                self.write(" ");
                self.write(binop_as_str(*op));
                self.write(" ");
                self.emit_expr(rhs);
            }
            ExprKind::Block(block) => self.emit_block(block),
            ExprKind::Call(callee, arg) => {
                self.emit_expr(callee);
                // The argument must be tuple-like to emit as `callee(args)`;
                // wrap a non-tuple argument in parens, since HIR has no paren node.
                if matches!(arg.kind, ExprKind::Tuple(_)) {
                    self.emit_expr(arg);
                } else {
                    self.write("(");
                    self.emit_expr(arg);
                    self.write(")");
                }
            }
            ExprKind::Fail(e) => {
                self.write("fail ");
                self.emit_expr(e);
            }
            ExprKind::Field(record, field) => {
                let field = self.field_display(&record.ty, field);
                self.emit_expr(record);
                self.write(&field);
            }
            ExprKind::If(cond, body, otherwise) => {
                self.write("if ");
                self.emit_expr(cond);
                self.write(" ");
                self.emit_expr(body);
                if let Some(e) = otherwise {
                    self.write(" else ");
                    self.emit_expr(e);
                }
            }
            ExprKind::Index(array, index) => {
                self.emit_expr(array);
                self.write("[");
                self.emit_expr(index);
                self.write("]");
            }
            ExprKind::Lit(lit) => self.emit_lit(lit),
            ExprKind::Range(start, step, end) => {
                self.emit_range(start.as_deref(), step.as_deref(), end.as_deref());
            }
            ExprKind::Return(e) => {
                self.write("return ");
                self.emit_expr(e);
            }
            ExprKind::String(components) => {
                let all_literal = components
                    .iter()
                    .all(|c| matches!(c, StringComponent::Lit(_)));
                if all_literal {
                    self.write("\"");
                    for component in components {
                        if let StringComponent::Lit(s) = component {
                            self.write(s);
                        }
                    }
                    self.write("\"");
                } else {
                    self.write("$\"");
                    for component in components {
                        match component {
                            StringComponent::Lit(s) => self.write(s),
                            StringComponent::Expr(e) => {
                                self.write("{");
                                self.emit_expr(e);
                                self.write("}");
                            }
                        }
                    }
                    self.write("\"");
                }
            }
            ExprKind::Tuple(exprs) => {
                self.write("(");
                self.emit_exprs_comma(exprs);
                if exprs.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            ExprKind::UnOp(op, expr) => {
                let op_str = unop_as_str(*op);
                if matches!(op, UnOp::Unwrap) {
                    self.emit_expr(expr);
                    self.write(op_str);
                } else {
                    self.write(op_str);
                    self.emit_expr(expr);
                }
            }
            ExprKind::Var(res, _generics) => self.emit_res(res),
            ExprKind::While(cond, block) => {
                self.write("while ");
                self.emit_expr(cond);
                self.emit_block(block);
            }
            ExprKind::Break => self.write("break"),
            ExprKind::Continue => self.write("continue"),
            ExprKind::For(pat, iterable, block) => {
                self.write("for ");
                self.emit_pat_binding(pat);
                self.write(" in ");
                self.emit_expr(iterable);
                self.emit_block(block);
            }
            ExprKind::Repeat(block, cond, fixup) => {
                self.write("repeat");
                self.emit_block(block);
                self.write("until ");
                self.emit_expr(cond);
                if let Some(fixup) = fixup {
                    self.write(" fixup");
                    self.emit_block(fixup);
                }
            }
            ExprKind::UpdateField(record, field, value) => {
                let field = self.field_display(&record.ty, field);
                self.emit_expr(record);
                self.trim_trailing_newline();
                self.write(" w/ ");
                self.write(&field);
                self.write(" <- ");
                self.emit_expr(value);
            }
            ExprKind::UpdateIndex(array, index, value) => {
                self.emit_expr(array);
                self.trim_trailing_newline();
                self.write(" w/ ");
                self.emit_expr(index);
                self.write(" <- ");
                self.emit_expr(value);
            }
            ExprKind::ArrayRepeat(item, size) => {
                self.write("[");
                self.emit_expr(item);
                self.write(", size = ");
                self.emit_expr(size);
                self.write("]");
            }
            ExprKind::AssignField(record, field, value) => {
                let field = self.field_display(&record.ty, field);
                self.emit_expr(record);
                self.trim_trailing_newline();
                self.write(" w/= ");
                self.write(&field);
                self.write(" <- ");
                self.emit_expr(value);
            }
            ExprKind::AssignIndex(array, index, value) => {
                self.emit_expr(array);
                self.trim_trailing_newline();
                self.write(" w/= ");
                self.emit_expr(index);
                self.write(" <- ");
                self.emit_expr(value);
            }
            ExprKind::Closure(captures, item) => self.emit_closure(captures, *item),
            ExprKind::Conjugate(within, apply) => {
                self.write("within");
                self.emit_block(within);
                self.trim_trailing_newline();
                self.write(" apply");
                self.emit_block(apply);
            }
            ExprKind::Hole => self.write("_"),
            ExprKind::Struct(res, copy, fields) => {
                self.emit_struct(res, copy.as_deref(), fields);
            }
            ExprKind::Err => self.write("/* err */"),
        }
    }

    fn emit_exprs_comma(&mut self, exprs: &'a [Expr]) {
        for (i, e) in exprs.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_expr(e);
        }
    }

    fn emit_range(
        &mut self,
        start: Option<&'a Expr>,
        step: Option<&'a Expr>,
        end: Option<&'a Expr>,
    ) {
        match (start, step, end) {
            (None, None, None) => self.write("..."),
            (None, None, Some(e)) => {
                self.write("...");
                self.emit_expr(e);
            }
            (None, Some(s), None) => {
                self.write("...");
                self.emit_expr(s);
                self.write("...");
            }
            (None, Some(s), Some(e)) => {
                self.write("...");
                self.emit_expr(s);
                self.write("..");
                self.emit_expr(e);
            }
            (Some(s), None, None) => {
                self.emit_expr(s);
                self.write("...");
            }
            (Some(s), None, Some(e)) => {
                self.emit_expr(s);
                self.write("..");
                self.emit_expr(e);
            }
            (Some(s), Some(step), None) => {
                self.emit_expr(s);
                self.write("..");
                self.emit_expr(step);
                self.write("...");
            }
            (Some(s), Some(step), Some(e)) => {
                self.emit_expr(s);
                self.write("..");
                self.emit_expr(step);
                self.write("..");
                self.emit_expr(e);
            }
        }
    }

    fn emit_lit(&mut self, lit: &Lit) {
        match lit {
            Lit::BigInt(v) => {
                self.write(&v.to_string());
                self.write("L");
            }
            Lit::Bool(v) => self.write(if *v { "true" } else { "false" }),
            Lit::Double(v) => {
                let s = if v.fract() == 0.0 {
                    format!("{v}.")
                } else {
                    format!("{v}")
                };
                self.write(&s);
            }
            Lit::Int(v) => self.write(&v.to_string()),
            Lit::Pauli(p) => self.write(match p {
                Pauli::I => "PauliI",
                Pauli::X => "PauliX",
                Pauli::Y => "PauliY",
                Pauli::Z => "PauliZ",
            }),
            Lit::Result(r) => self.write(match r {
                HirResult::Zero => "Zero",
                HirResult::One => "One",
            }),
        }
    }

    fn emit_res(&mut self, res: &Res) {
        match res {
            Res::Err => self.write("/* err */"),
            Res::Local(node_id) => {
                let name = match self.local_names.get(node_id) {
                    Some(name) => name.to_string(),
                    None => format!("_local{node_id}"),
                };
                self.write(&name);
            }
            Res::Item(item_id) => {
                let name = self.item_name(*item_id);
                self.write(&name);
            }
        }
    }

    fn item_name(&self, item_id: ItemId) -> String {
        let item = if item_id.package == self.package.package_id {
            self.package.items.get(item_id.item)
        } else {
            self.store
                .get(item_id.package)
                .and_then(|unit| unit.package.items.get(item_id.item))
        };
        match item.map(|i| &i.kind) {
            Some(ItemKind::Callable(decl)) => sanitize_ident(&decl.name.name),
            Some(ItemKind::Ty(name, _)) => sanitize_ident(&name.name),
            _ => "/* unknown item */".to_string(),
        }
    }

    /// Renders `field` accessed on a record of type `record_ty`, resolving a
    /// `Field::Path` to its declared field name via the owning UDT definition
    /// when possible. Mirrors the field resolution in the FIR emitter
    /// `FirQSharpGen`, in `qsc_fir_transforms::pretty`.
    fn field_display(&self, record_ty: &Ty, field: &Field) -> String {
        match field {
            Field::Err => "::/* err */".to_string(),
            Field::Prim(PrimField::Start) => "::Start".to_string(),
            Field::Prim(PrimField::Step) => "::Step".to_string(),
            Field::Prim(PrimField::End) => "::End".to_string(),
            Field::Path(path) => self.resolve_field_path(record_ty, path),
        }
    }

    /// Resolves a tuple-index `FieldPath` to its declared `::Name` when the
    /// record's UDT definition is available, falling back to the raw index
    /// chain `::Item<i0>::Item<i1>` otherwise.
    fn resolve_field_path(&self, record_ty: &Ty, path: &FieldPath) -> String {
        if let Some(udt) = self.lookup_udt(record_ty)
            && let Some(name) = udt_field_name(udt, path)
        {
            return format!("::{name}");
        }
        let mut out = String::new();
        for idx in &path.indices {
            let _ = write!(out, "::Item<{idx}>");
        }
        out
    }

    /// Looks up the [`Udt`] definition backing `ty` when it is a resolved
    /// user-defined type, searching the current package first and then the
    /// dependency store.
    fn lookup_udt(&self, ty: &Ty) -> Option<&Udt> {
        let Ty::Udt(_, Res::Item(item_id)) = ty else {
            return None;
        };
        let item = if item_id.package == self.package.package_id {
            self.package.items.get(item_id.item)
        } else {
            self.store
                .get(item_id.package)
                .and_then(|unit| unit.package.items.get(item_id.item))
        };
        match item.map(|i| &i.kind) {
            Some(ItemKind::Ty(_, udt)) => Some(udt),
            _ => None,
        }
    }

    /// Renders a lifted `Closure` as the target callable's name, prefixed with a
    /// comment recording the source item id and captured locals. Mirrors the
    /// closure rendering in the FIR emitter `FirQSharpGen`.
    fn emit_closure(&mut self, captures: &[NodeId], item: LocalItemId) {
        self.write("/* closure item=");
        self.write(&format!("{item}"));
        self.write(" captures=[");
        for (i, capture) in captures.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            let name = match self.local_names.get(capture) {
                Some(name) => name.to_string(),
                None => format!("_local{capture}"),
            };
            self.write(&name);
        }
        self.write("] */ ");
        let name = self.item_name(ItemId {
            package: self.package.package_id,
            item,
        });
        self.write(&name);
    }

    /// Renders a struct constructor `new <Ty> { ... }`, including an optional
    /// `...base` copy prefix and the field assignments. Mirrors the struct
    /// rendering in the FIR emitter `FirQSharpGen`.
    fn emit_struct(&mut self, res: &Res, copy: Option<&'a Expr>, fields: &'a [Box<FieldAssign>]) {
        self.write("new ");
        self.emit_res(res);
        self.writeln(" {");
        if let Some(base) = copy {
            self.write("...");
            self.emit_expr(base);
            if !fields.is_empty() {
                self.writeln(",");
            }
        }
        let record_ty = match res {
            Res::Item(_) => Ty::Udt(Rc::from(""), *res),
            _ => Ty::Err,
        };
        self.emit_field_assigns(&record_ty, fields);
        self.write("}");
    }

    /// Renders the comma-separated `field = value` assignments of a struct
    /// constructor, one per line.
    fn emit_field_assigns(&mut self, record_ty: &Ty, fields: &'a [Box<FieldAssign>]) {
        if let Some((last, most)) = fields.split_last() {
            for fa in most {
                self.emit_field_assign(record_ty, fa);
                self.writeln(",");
            }
            self.emit_field_assign(record_ty, last);
            self.writeln("");
        }
    }

    /// Renders a single `field = value` struct-constructor assignment. The
    /// leading `::` that [`Self::field_display`] emits for a named field is
    /// stripped to match idiomatic Q# constructor syntax.
    fn emit_field_assign(&mut self, record_ty: &Ty, fa: &'a FieldAssign) {
        let display = self.field_display(record_ty, &fa.field);
        let trimmed = display.strip_prefix("::").unwrap_or(&display);
        self.write(trimmed);
        self.write(" = ");
        self.emit_expr(&fa.value);
    }
}

/// Walks `udt`'s definition following `path`'s tuple indices and returns the
/// declared field name at the destination, or `None` when the path lands on a
/// tuple rather than a named field. Mirrors the FIR emitter's `udt_field_name`.
fn udt_field_name(udt: &Udt, path: &FieldPath) -> Option<Rc<str>> {
    let mut def = &udt.definition;
    for &index in &path.indices {
        match &def.kind {
            UdtDefKind::Tuple(items) => def = items.get(index)?,
            UdtDefKind::Field(_) => return None,
        }
    }
    match &def.kind {
        UdtDefKind::Field(field) => field.name.clone(),
        UdtDefKind::Tuple(_) => None,
    }
}

/// Returns `true` when emitting `kind` ends with a block's closing `}`, which
/// [`HirQSharpGen::emit_block`] already terminates with a newline. Such a
/// statement-wrapped expression needs no extra line terminator, so the emitter
/// avoids leaving a blank line after the block.
fn expr_ends_with_block(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Block(_)
        | ExprKind::If(..)
        | ExprKind::While(..)
        | ExprKind::For(..)
        | ExprKind::Conjugate(..) => true,
        ExprKind::Repeat(_, cond, fixup) => fixup.is_some() || expr_ends_with_block(&cond.kind),
        _ => false,
    }
}

/// Collects the surface names of every `let`/`mutable`/`use`/`borrow` and
/// callable-input binding in a callable, keyed on the binding [`NodeId`], so
/// later `Res::Local` uses can be rendered by name. Synthetic names introduced
/// by desugaring (for example `.array_id_17`) already embed their `NodeId`, so
/// they are kept verbatim and remain globally unambiguous. Recursion through
/// `PatKind::Tuple` is handled by [`walk_pat`].
struct LocalNameCollector {
    names: FxHashMap<NodeId, Rc<str>>,
}

impl<'a> Visitor<'a> for LocalNameCollector {
    fn visit_pat(&mut self, pat: &'a Pat) {
        if let PatKind::Bind(ident) = &pat.kind {
            self.names
                .insert(ident.id, Rc::from(sanitize_ident(&ident.name)));
        }
        walk_pat(self, pat);
    }
}

/// Rewrites an identifier so it is a lexically valid Q# identifier: any
/// character that is invalid in that position is replaced with `_`, and an
/// empty result becomes `_`. This mirrors the parseable-mode sanitizer in
/// `FirQSharpGen`, in `qsc_fir_transforms::pretty`.
///
/// It matters for the synthetic names a desugaring pass introduces, for
/// example `.array_id_17`: the leading `.` sentinel is not valid in a Q#
/// identifier and would otherwise confuse formatter tokenization.
fn sanitize_ident(name: &str) -> String {
    let mut rendered = String::with_capacity(name.len());
    for (index, ch) in name.chars().enumerate() {
        let is_valid = if index == 0 {
            ch == '_' || ch.is_ascii_alphabetic()
        } else {
            ch == '_' || ch.is_ascii_alphanumeric()
        };
        rendered.push(if is_valid { ch } else { '_' });
    }
    if rendered.is_empty() {
        rendered.push('_');
    }
    rendered
}

fn binop_as_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::AndB => "&&&",
        BinOp::AndL => "and",
        BinOp::Div => "/",
        BinOp::Eq => "==",
        BinOp::Exp => "^",
        BinOp::Gt => ">",
        BinOp::Gte => ">=",
        BinOp::Lt => "<",
        BinOp::Lte => "<=",
        BinOp::Mod => "%",
        BinOp::Mul => "*",
        BinOp::Neq => "!=",
        BinOp::OrB => "|||",
        BinOp::OrL => "or",
        BinOp::Shl => "<<<",
        BinOp::Shr => ">>>",
        BinOp::Sub => "-",
        BinOp::XorB => "^^^",
    }
}

fn unop_as_str(op: UnOp) -> &'static str {
    match op {
        UnOp::Functor(Functor::Adj) => "Adjoint ",
        UnOp::Functor(Functor::Ctl) => "Controlled ",
        UnOp::Neg => "-",
        UnOp::NotB => "~~~",
        UnOp::NotL => "not ",
        UnOp::Pos => "+",
        UnOp::Unwrap => "!",
    }
}

fn functor_set_value_as_str(fsv: FunctorSetValue) -> &'static str {
    match fsv {
        FunctorSetValue::Empty => "()",
        FunctorSetValue::Adj => "Adj",
        FunctorSetValue::Ctl => "Ctl",
        FunctorSetValue::CtlAdj => "Adj + Ctl",
    }
}
