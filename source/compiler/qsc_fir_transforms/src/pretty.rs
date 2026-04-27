// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR-to-Q# pretty-printer for pass debugging.
//!
//! Walks FIR structures via [`PackageLookup`]/[`PackageStoreLookup`] and
//! writes lexically valid Q# with minimal whitespace, then runs
//! [`qsc_formatter::formatter::format_str`] over the raw output.
//!
//! The emitter is intended for before/after snapshot tests of FIR
//! transform passes. It is best-effort — some FIR-only constructs render
//! as Q# comments or synthetic surface syntax:
//!
//! - [`ExprKind::Closure`] → `/* closure item=<id> captures=[<ids>] */`
//!   followed by a reference to the lifted callable item.
//! - [`ExprKind::ArrayLit`] renders with the same surface as
//!   [`ExprKind::Array`].
//! - [`ExprKind::AssignField`] / [`ExprKind::AssignIndex`] /
//!   [`ExprKind::UpdateField`] / [`ExprKind::UpdateIndex`] render via the
//!   idiomatic `r w/= F <- v` / `r w/ F <- v` forms.
//! - [`Field::Path`] chains indices as `::Item<i0>::Item<i1>` when UDT
//!   metadata is not available; otherwise field names resolve through the
//!   owning [`Udt`].
//! - [`Ty::Prim`] renders via [`prim_as_qsharp`].
//!
//! # Borrow strategy
//!
//! Walking the FIR requires shared borrows through [`PackageLookup`] while
//! also mutating the output buffer. The emitter resolves this by *cloning*
//! the FIR node kind at every traversal boundary (the nodes are cheap
//! struct/enum types) before calling back into `&mut self` helpers.

#[cfg(test)]
mod tests;

use qsc_fir::fir::{
    BinOp, BlockId, CallableDecl, CallableImpl, CallableKind, ExprId, ExprKind, Field, FieldAssign,
    FieldPath, Functor, ItemId, ItemKind, Lit, LocalItemId, LocalVarId, Mutability, Package,
    PackageId, PackageLookup, PackageStore, PackageStoreLookup, PatId, PatKind, Pauli, PrimField,
    Res, Result as FirResult, SpecDecl, StmtId, StmtKind, StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, GenericArg, Prim, Ty, TypeParameter, Udt};
use qsc_formatter::formatter::format_str;
use rustc_hash::FxHashMap;
use std::fmt::Write as _;
use std::rc::Rc;

/// Renders the full FIR package as Q# source.
#[must_use]
pub fn write_package_qsharp(store: &PackageStore, package_id: PackageId) -> String {
    let mut emitter = FirQSharpGen::new(store, package_id);
    emitter.emit_package();
    format_str(&emitter.output)
}

/// Renders a single callable item as Q# source.
#[must_use]
pub fn write_callable_qsharp(
    store: &PackageStore,
    package_id: PackageId,
    item: LocalItemId,
) -> String {
    let mut emitter = FirQSharpGen::new(store, package_id);
    let decl = match &emitter.package().get_item(item).kind {
        ItemKind::Callable(decl) => Some((**decl).clone()),
        _ => None,
    };
    if let Some(decl) = decl {
        emitter.emit_callable_decl(&decl);
    }
    format_str(&emitter.output)
}

/// Renders a single block as Q# source.
#[must_use]
pub fn write_block_qsharp(store: &PackageStore, package_id: PackageId, block: BlockId) -> String {
    let mut emitter = FirQSharpGen::new(store, package_id);
    emitter.emit_block(block);
    format_str(&emitter.output)
}

/// Renders a single expression as Q# source.
#[must_use]
pub fn write_expr_qsharp(store: &PackageStore, package_id: PackageId, expr: ExprId) -> String {
    let mut emitter = FirQSharpGen::new(store, package_id);
    emitter.emit_expr(expr);
    format_str(&emitter.output)
}

/// Renders a single statement as Q# source.
#[must_use]
pub fn write_stmt_qsharp(store: &PackageStore, package_id: PackageId, stmt: StmtId) -> String {
    let mut emitter = FirQSharpGen::new(store, package_id);
    emitter.emit_stmt(stmt);
    format_str(&emitter.output)
}

struct FirQSharpGen<'a> {
    output: String,
    store: &'a PackageStore,
    package_id: PackageId,
    local_names: FxHashMap<LocalVarId, Rc<str>>,
}

impl<'a> FirQSharpGen<'a> {
    fn new(store: &'a PackageStore, package_id: PackageId) -> Self {
        let mut this = Self {
            output: String::new(),
            store,
            package_id,
            local_names: FxHashMap::default(),
        };
        this.collect_local_names();
        this
    }

    fn package(&self) -> &Package {
        self.store.get(self.package_id)
    }

    fn collect_local_names(&mut self) {
        let pkg = self.package();
        let entries: Vec<(LocalVarId, Rc<str>)> = pkg
            .pats
            .values()
            .filter_map(|pat| match &pat.kind {
                PatKind::Bind(ident) => Some((ident.id, ident.name.clone())),
                PatKind::Discard | PatKind::Tuple(_) => None,
            })
            .collect();
        for (id, name) in entries {
            self.local_names.insert(id, name);
        }
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn writeln(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn emit_package(&mut self) {
        let ids: Vec<LocalItemId> = self.package().items.values().map(|i| i.id).collect();
        for id in ids {
            self.emit_item(id);
        }
        let entry = self.package().entry;
        if let Some(e) = entry {
            self.writeln("// entry");
            self.emit_expr(e);
            self.writeln("");
        }
    }

    fn emit_item(&mut self, id: LocalItemId) {
        let kind = self.package().get_item(id).kind.clone();
        match kind {
            ItemKind::Callable(decl) => self.emit_callable_decl(&decl),
            ItemKind::Namespace(name, _) => {
                self.write("// namespace ");
                self.write(&name.name);
                self.writeln("");
            }
            ItemKind::Ty(name, udt) => {
                let ty = udt.get_pure_ty();
                self.write("newtype ");
                self.write(&name.name);
                self.write(" = ");
                self.emit_ty(&ty);
                self.writeln(";");
            }
            ItemKind::Export(name, res) => {
                self.write("// export ");
                self.write(&name.name);
                self.write(" = ");
                self.emit_res(&res);
                self.writeln("");
            }
        }
    }

    fn emit_callable_decl(&mut self, decl: &CallableDecl) {
        match decl.kind {
            CallableKind::Function => self.write("function "),
            CallableKind::Operation => self.write("operation "),
        }
        self.write(&decl.name.name);
        if !decl.generics.is_empty() {
            self.write("<");
            for (i, g) in decl.generics.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(&type_parameter_name(g));
            }
            self.write(">");
        }
        self.emit_pat(decl.input);
        self.write(" : ");
        self.emit_ty(&decl.output);
        if decl.functors != FunctorSetValue::Empty {
            self.write(" is ");
            self.write(functor_set_value_as_str(decl.functors));
        }

        // Future optimization: omit the body label and braces when only a body exists.

        match &decl.implementation {
            CallableImpl::Intrinsic => {
                self.writeln(" { body intrinsic; }");
            }
            CallableImpl::Spec(spec) => {
                let body = spec.body.clone();
                let adj = spec.adj.clone();
                let ctl = spec.ctl.clone();
                let ctl_adj = spec.ctl_adj.clone();
                self.writeln(" {");
                self.emit_spec_decl("body", &body);
                if let Some(s) = adj {
                    self.emit_spec_decl("adjoint", &s);
                }
                if let Some(s) = ctl {
                    self.emit_spec_decl("controlled", &s);
                }
                if let Some(s) = ctl_adj {
                    self.emit_spec_decl("controlled adjoint", &s);
                }
                self.writeln("}");
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                let spec = spec.clone();
                self.writeln(" {");
                self.emit_spec_decl("body", &spec);
                self.writeln("}");
            }
        }
    }

    fn emit_spec_decl(&mut self, label: &str, spec: &SpecDecl) {
        self.write(label);
        self.emit_block(spec.block);
    }

    fn emit_block(&mut self, block_id: BlockId) {
        let stmts = self.package().get_block(block_id).stmts.clone();
        self.writeln(" {");
        for stmt in stmts {
            self.emit_stmt(stmt);
        }
        self.writeln("}");
    }

    fn emit_stmt(&mut self, stmt_id: StmtId) {
        let kind = self.package().get_stmt(stmt_id).kind.clone();
        match kind {
            StmtKind::Expr(e) => {
                self.emit_expr(e);
                self.writeln("");
            }
            StmtKind::Semi(e) => {
                self.emit_expr(e);
                self.writeln(";");
            }
            StmtKind::Local(mutability, pat_id, expr) => {
                match mutability {
                    Mutability::Immutable => self.write("let "),
                    Mutability::Mutable => self.write("mutable "),
                }
                self.emit_pat(pat_id);
                self.write(" = ");
                self.emit_expr(expr);
                self.writeln(";");
            }
            StmtKind::Item(item_id) => {
                self.write("// item ");
                self.write(&format!("{item_id}"));
                self.writeln("");
            }
        }
    }

    fn emit_expr(&mut self, expr_id: ExprId) {
        let kind = self.package().get_expr(expr_id).kind.clone();
        self.emit_expr_kind(&kind);
    }

    #[allow(clippy::too_many_lines)]
    fn emit_expr_kind(&mut self, kind: &ExprKind) {
        match kind {
            ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) => {
                self.write("[");
                self.emit_comma_separated_exprs(exprs);
                self.write("]");
            }
            ExprKind::ArrayRepeat(item, size) => {
                self.write("[");
                self.emit_expr(*item);
                self.write(", size = ");
                self.emit_expr(*size);
                self.write("]");
            }
            ExprKind::Assign(lhs, rhs) => {
                self.emit_expr(*lhs);
                self.write(" = ");
                self.emit_expr(*rhs);
            }
            ExprKind::AssignOp(op, lhs, rhs) => {
                self.emit_expr(*lhs);
                self.write(" ");
                self.write(binop_as_str(*op));
                self.write("= ");
                self.emit_expr(*rhs);
            }
            ExprKind::AssignField(record, field, value) => {
                self.emit_expr(*record);
                self.write(" w/= ");
                self.emit_field(*record, field);
                self.write(" <- ");
                self.emit_expr(*value);
            }
            ExprKind::AssignIndex(array, index, value) => {
                self.emit_expr(*array);
                self.write(" w/= ");
                self.emit_expr(*index);
                self.write(" <- ");
                self.emit_expr(*value);
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                self.emit_expr(*lhs);
                self.write(" ");
                self.write(binop_as_str(*op));
                self.write(" ");
                self.emit_expr(*rhs);
            }
            ExprKind::Block(block) => self.emit_block(*block),
            ExprKind::Call(callee, arg) => {
                self.emit_expr(*callee);
                // Argument must be tuple-like to emit as `callee(args)`; for
                // non-tuple args, wrap in parens ourselves.
                let arg_is_tuple = matches!(self.package().get_expr(*arg).kind, ExprKind::Tuple(_));
                if arg_is_tuple {
                    self.emit_expr(*arg);
                } else {
                    self.write("(");
                    self.emit_expr(*arg);
                    self.write(")");
                }
            }
            ExprKind::Closure(captures, item) => {
                self.write("/* closure item=");
                self.write(&format!("{item}"));
                self.write(" captures=[");
                for (i, local) in captures.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    let display = self.local_display(*local);
                    self.write(&display);
                }
                self.write("] */ ");
                let name = self.callable_name_for(*item);
                self.write(&name);
            }
            ExprKind::Fail(e) => {
                self.write("fail ");
                self.emit_expr(*e);
            }
            ExprKind::Field(record, field) => {
                self.emit_expr(*record);
                self.emit_field(*record, field);
            }
            ExprKind::Hole => self.write("_"),
            ExprKind::If(cond, body, otherwise) => {
                self.write("if ");
                self.emit_expr(*cond);
                self.write(" ");
                self.emit_expr(*body);
                if let Some(e) = otherwise {
                    let is_elif = matches!(self.package().get_expr(*e).kind, ExprKind::If(..));
                    if is_elif {
                        self.write(" el");
                    } else {
                        self.write(" else ");
                    }
                    self.emit_expr(*e);
                }
            }
            ExprKind::Index(array, index) => {
                self.emit_expr(*array);
                self.write("[");
                self.emit_expr(*index);
                self.write("]");
            }
            ExprKind::Lit(lit) => self.emit_lit(lit),
            ExprKind::Range(start, step, end) => {
                self.emit_range(*start, *step, *end);
            }
            ExprKind::Return(e) => {
                self.write("return ");
                self.emit_expr(*e);
            }
            ExprKind::Struct(res, copy, fields) => {
                self.write("new ");
                self.emit_res(res);
                self.writeln(" {");
                if let Some(c) = copy {
                    self.write("...");
                    self.emit_expr(*c);
                    if !fields.is_empty() {
                        self.writeln(",");
                    }
                }
                let struct_ty = match res {
                    Res::Item(_) => Ty::Udt(*res),
                    _ => Ty::Err,
                };
                self.emit_field_assigns(&struct_ty, fields);
                self.writeln("}");
            }
            ExprKind::String(components) => {
                self.write("$\"");
                for component in components {
                    match component {
                        StringComponent::Expr(e) => {
                            self.write("{");
                            self.emit_expr(*e);
                            self.write("}");
                        }
                        StringComponent::Lit(s) => self.write(s),
                    }
                }
                self.write("\"");
            }
            ExprKind::Tuple(exprs) => {
                self.write("(");
                if let Some((last, most)) = exprs.split_last() {
                    for e in most {
                        self.emit_expr(*e);
                        self.write(", ");
                    }
                    self.emit_expr(*last);
                    if most.is_empty() {
                        self.write(",");
                    }
                }
                self.write(")");
            }
            ExprKind::UnOp(op, expr) => {
                let op_str = unop_as_str(*op);
                if matches!(op, UnOp::Unwrap) {
                    self.emit_expr(*expr);
                    self.write(op_str);
                } else {
                    self.write(op_str);
                    self.emit_expr(*expr);
                }
            }
            ExprKind::UpdateField(record, field, value) => {
                self.emit_expr(*record);
                self.write(" w/ ");
                self.emit_field(*record, field);
                self.write(" <- ");
                self.emit_expr(*value);
            }
            ExprKind::UpdateIndex(array, index, value) => {
                self.emit_expr(*array);
                self.write(" w/ ");
                self.emit_expr(*index);
                self.write(" <- ");
                self.emit_expr(*value);
            }
            ExprKind::Var(res, args) => {
                self.emit_res(res);
                if !args.is_empty() {
                    self.write("<");
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.emit_generic_arg(arg);
                    }
                    self.write(">");
                }
            }
            ExprKind::While(cond, block) => {
                self.write("while ");
                self.emit_expr(*cond);
                self.emit_block(*block);
            }
        }
    }

    fn emit_comma_separated_exprs(&mut self, exprs: &[ExprId]) {
        if let Some((last, most)) = exprs.split_last() {
            for e in most {
                self.emit_expr(*e);
                self.write(", ");
            }
            self.emit_expr(*last);
        }
    }

    fn emit_field_assigns(&mut self, record_ty: &Ty, fields: &[FieldAssign]) {
        if let Some((last, most)) = fields.split_last() {
            for fa in most {
                self.emit_field_assign(record_ty, fa);
                self.writeln(",");
            }
            self.emit_field_assign(record_ty, last);
            self.writeln("");
        }
    }

    fn emit_field_assign(&mut self, record_ty: &Ty, fa: &FieldAssign) {
        let display = self.field_display(record_ty, &fa.field);
        // Field::Path renders as "::Name"; strip the leading "::" in struct
        // constructor assignments to match idiomatic Q#.
        let trimmed = display.strip_prefix("::").unwrap_or(&display);
        self.write(trimmed);
        self.write(" = ");
        self.emit_expr(fa.value);
    }

    fn emit_range(&mut self, start: Option<ExprId>, step: Option<ExprId>, end: Option<ExprId>) {
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
                FirResult::Zero => "Zero",
                FirResult::One => "One",
            }),
        }
    }

    fn emit_pat(&mut self, pat_id: PatId) {
        let pat = self.package().get_pat(pat_id).clone();
        match pat.kind {
            PatKind::Bind(ident) => {
                self.write(&ident.name);
                self.write(" : ");
                self.emit_ty(&pat.ty);
            }
            PatKind::Discard => {
                self.write("_ : ");
                self.emit_ty(&pat.ty);
            }
            PatKind::Tuple(pats) => {
                self.write("(");
                if let Some((last, most)) = pats.split_last() {
                    for p in most {
                        self.emit_pat(*p);
                        self.write(", ");
                    }
                    self.emit_pat(*last);
                    if most.is_empty() {
                        self.write(",");
                    }
                }
                self.write(")");
            }
        }
    }

    fn emit_res(&mut self, res: &Res) {
        match res {
            Res::Err => self.write("/* err */"),
            Res::Local(local) => {
                let display = self.local_display(*local);
                self.write(&display);
            }
            Res::Item(item_id) => {
                let name = self.item_name(*item_id);
                self.write(&name);
            }
        }
    }

    fn local_display(&self, local: LocalVarId) -> String {
        match self.local_names.get(&local) {
            Some(name) => name.to_string(),
            None => format!("_local{local}"),
        }
    }

    fn callable_name_for(&self, item: LocalItemId) -> String {
        let pkg = self.package();
        match &pkg.get_item(item).kind {
            ItemKind::Callable(decl) => decl.name.name.to_string(),
            ItemKind::Ty(name, _) => name.name.to_string(),
            _ => format!("Item({item})"),
        }
    }

    fn item_name(&self, item_id: ItemId) -> String {
        if item_id.package == self.package_id {
            self.callable_name_for(item_id.item)
        } else {
            let store_id = StoreItemId {
                package: item_id.package,
                item: item_id.item,
            };
            match &self.store.get_item(store_id).kind {
                ItemKind::Callable(decl) => decl.name.name.to_string(),
                ItemKind::Ty(name, _) => name.name.to_string(),
                _ => format!("{item_id}"),
            }
        }
    }

    fn emit_field(&mut self, record: ExprId, field: &Field) {
        let record_ty = self.package().get_expr(record).ty.clone();
        let display = self.field_display(&record_ty, field);
        self.write(&display);
    }

    fn field_display(&self, record_ty: &Ty, field: &Field) -> String {
        match field {
            Field::Err => "::/* err */".to_string(),
            Field::Prim(prim) => match prim {
                PrimField::Start => "::Start".to_string(),
                PrimField::Step => "::Step".to_string(),
                PrimField::End => "::End".to_string(),
            },
            Field::Path(path) => self.resolve_field_path(record_ty, path),
        }
    }

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

    fn lookup_udt(&self, ty: &Ty) -> Option<&Udt> {
        let Ty::Udt(Res::Item(item_id)) = ty else {
            return None;
        };
        let store_id = StoreItemId {
            package: item_id.package,
            item: item_id.item,
        };
        let item = self.store.get_item(store_id);
        match &item.kind {
            ItemKind::Ty(_, udt) => Some(udt),
            _ => None,
        }
    }

    fn emit_ty(&mut self, ty: &Ty) {
        self.write(&ty_as_qsharp(ty));
    }

    fn emit_generic_arg(&mut self, arg: &GenericArg) {
        match arg {
            GenericArg::Ty(ty) => self.emit_ty(ty),
            GenericArg::Functor(FunctorSet::Value(fsv)) => {
                self.write(functor_set_value_as_str(*fsv));
            }
            GenericArg::Functor(FunctorSet::Param(p)) => {
                self.write(&format!("functor<{p}>"));
            }
            GenericArg::Functor(FunctorSet::Infer(_)) => {
                self.write("functor<?>");
            }
        }
    }
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

fn prim_as_qsharp(prim: Prim) -> &'static str {
    match prim {
        Prim::BigInt => "BigInt",
        Prim::Bool => "Bool",
        Prim::Double => "Double",
        Prim::Int => "Int",
        Prim::Pauli => "Pauli",
        Prim::Qubit => "Qubit",
        Prim::Range | Prim::RangeTo | Prim::RangeFrom | Prim::RangeFull => "Range",
        Prim::Result => "Result",
        Prim::String => "String",
    }
}

fn ty_as_qsharp(ty: &Ty) -> String {
    match ty {
        Ty::Array(item) => format!("{}[]", ty_as_qsharp(item)),
        Ty::Arrow(arrow) => arrow_as_qsharp(arrow),
        Ty::Infer(_) => "_".to_string(),
        Ty::Param(p) => format!("'T{p}"),
        Ty::Prim(p) => prim_as_qsharp(*p).to_string(),
        Ty::Tuple(items) => {
            if items.is_empty() {
                "Unit".to_string()
            } else if items.len() == 1 {
                format!("({},)", ty_as_qsharp(&items[0]))
            } else {
                let parts: Vec<_> = items.iter().map(ty_as_qsharp).collect();
                format!("({})", parts.join(", "))
            }
        }
        Ty::Udt(Res::Item(item_id)) => format!("UDT<{item_id}>"),
        Ty::Udt(Res::Local(local)) => format!("UDT<Local {local}>"),
        Ty::Udt(Res::Err) => "UDT<?>".to_string(),
        Ty::Err => "?".to_string(),
    }
}

fn arrow_as_qsharp(arrow: &Arrow) -> String {
    let sep = match arrow.kind {
        CallableKind::Function => "->",
        CallableKind::Operation => "=>",
    };
    let input = ty_as_qsharp(&arrow.input);
    let output = ty_as_qsharp(&arrow.output);
    match arrow.functors {
        FunctorSet::Value(FunctorSetValue::Empty) => format!("({input} {sep} {output})"),
        FunctorSet::Value(v) => format!(
            "({input} {sep} {output} is {})",
            functor_set_value_as_str(v)
        ),
        FunctorSet::Param(p) => format!("({input} {sep} {output} is functor<{p}>)"),
        FunctorSet::Infer(_) => format!("({input} {sep} {output} is functor<?>)"),
    }
}

fn type_parameter_name(p: &TypeParameter) -> String {
    match p {
        TypeParameter::Ty { name, .. } => format!("'{name}"),
        TypeParameter::Functor(fsv) => format!("functor<{}>", functor_set_value_as_str(*fsv)),
    }
}

fn udt_field_name(udt: &Udt, path: &FieldPath) -> Option<Rc<str>> {
    use qsc_fir::ty::UdtDefKind;
    let mut def = &udt.definition;
    for &index in &path.indices {
        match &def.kind {
            UdtDefKind::Tuple(items) => {
                def = items.get(index)?;
            }
            UdtDefKind::Field(_) => return None,
        }
    }
    match &def.kind {
        UdtDefKind::Field(f) => f.name.clone(),
        UdtDefKind::Tuple(_) => None,
    }
}
