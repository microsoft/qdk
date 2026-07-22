// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Administrative-normal-form (ANF) operand lifting for loop control flow.
//!
//! A Q# block is an expression, so `break`/`continue` can sit behind a
//! statement-carrying `Block`/`If`/`While`/`For`/`Repeat` that itself feeds an
//! enclosing operator, call, or binding — an operand position the later
//! `loop_unification` desugar cannot rewrite in place. This pass lifts each such
//! operand to a fresh spine `let` binding, innermost-first, so the buried
//! `break`/`continue` is exposed at a statement boundary before the desugar
//! runs. Earlier sibling operands are pinned to their own temps first so
//! left-to-right evaluation order and side effects are preserved.
//!
//! This is a port to HIR of the `return_unify` FIR transform's ANF operand
//! lift, `return_unify::normalize::anf`. Two things change from the original:
//! the IR is HIR rather than FIR, and the lift fires on `break`/`continue`
//! rather than `return`. `return` is already unified downstream by
//! `return_unify` on the FIR codegen path, so hoisting it here would needlessly
//! change return handling. The operand-slot walk and the innermost-first,
//! evaluation-order-preserving lift mirror the original.
//!
//! ## Control-flow scoping
//!
//! A `break`/`continue` escapes the scanned expression only when it binds to a
//! loop outside it. One inside a loop nested within the expression binds to
//! that loop and does not trigger a lift.
//!
//! ## Non-defaultable operands and array-backing
//!
//! A lifted candidate can diverge through the control flow it carries before
//! producing its value, so on the divergence path the hoisted temporary holds
//! no meaningful value. The later desugar linearizes that path and still reads
//! the temp's slot textually, so the slot needs a well-typed placeholder even
//! though the value is never observed, because the read is guarded behind the
//! `break`/`continue` flags. When the operand's type `T` has a classical
//! default the temp is bound directly and that default seeds the divergence
//! path. Otherwise the temp is array-backed: it is stored as `T[]`, whose
//! divergence-path default is the universal `[]` that is well-typed for every
//! element type. The candidate's produced value `v` is wrapped as `[v]`, and
//! the operand slot reads the element back through `.operand_tmp_<id>[0]`. Because
//! the read is only reached on the fall-through path, the empty array is never
//! indexed. This mirrors the array-backed return slot of the `return_unify`
//! FIR transform and covers `Qubit`, arrow, user-defined types, and tuples
//! thereof uniformly, without synthesizing a default of the operand's own
//! type. Only a genuinely-unrepresentable operand type — an inference or
//! type-parameter placeholder, the error type, or an unresolved user-defined
//! type — is rejected with [`Error::UnsupportedType`]. None of these can occur
//! for a well-typed operand post-typecheck, so the rejection is a defensive
//! guard.
//!
//! This module runs immediately before `loop_unification` in the pass drivers,
//! so operand-position `break`/`continue` is exposed at a statement boundary
//! before the loop desugar rewrites it.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    assigner::Assigner,
    hir::{
        BinOp, Block, Expr, ExprKind, Field, FieldPath, Lit, Mutability, QubitInit, QubitInitKind,
        Res, Stmt, StmtKind, StringComponent, UnOp,
    },
    mut_visit::MutVisitor,
    ty::{Prim, Ty},
    visit::Visitor,
};
use thiserror::Error;

use crate::common::{EnclosingBreakContinueScan, IdentTemplate, gen_ident};

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("cannot hoist `break`/`continue` out of an operand-position value of type `{0}`")]
    #[diagnostic(code("Qdk.Qsc.LoopNormalize.UnsupportedBreakContinueType"))]
    #[diagnostic(help(
        "the operand's type could not be resolved to a representable form, so the temporary \
         introduced to lift the break/continue cannot be synthesized; restructure the code so \
         the break/continue is not nested inside this operand"
    ))]
    UnsupportedType(String, #[label("operand with unsupported type")] Span),
}

/// Lifts operand-position `break`/`continue` buried behind a statement-carrying
/// construct to statement-position `let` temps.
///
/// Runs to a fixpoint per statement: each statement's surface expression is
/// rewritten until no operand-position candidate remains, then the resulting
/// statements are visited so nested blocks and deeper operands are normalized.
///
/// # Before
/// ```text
/// Foo({ if c { break; } x });
/// ```
///
/// # After
/// ```text
/// let .operand_tmp_<id> = { if c { break; } x };
/// Foo(.operand_tmp_<id>);
/// ```
pub(super) struct LoopNormalize<'a> {
    pub(super) assigner: &'a mut Assigner,
    pub(super) errors: Vec<Error>,
}

/// Which short-circuit operator form is being reshaped into an `If`, carrying
/// whether the operator is `and` (`true`) or `or` (`false`).
enum ShortCircuitForm {
    /// A value-producing `a and/or b` `BinOp`.
    BinOp(bool),
    /// A compound `set p and=/or= rhs` assignment.
    AssignOp(bool),
}

impl<'a> LoopNormalize<'a> {
    pub(super) fn new(assigner: &'a mut Assigner) -> Self {
        Self {
            assigner,
            errors: Vec::new(),
        }
    }

    /// Extracts the surface expression of `stmt` and lifts one operand-position
    /// candidate out of it, returning the spine `let` bindings to splice before
    /// `stmt`, or `None` when no operand candidate remains.
    fn lift_stmt_surface(&mut self, stmt: &mut Stmt) -> Option<Vec<Stmt>> {
        match &mut stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => self.lift_once(e),
            StmtKind::Qubit(_, _, init, _) => {
                let mut operands = Vec::new();
                collect_qubit_init_operands(init, &mut operands);
                self.lift_operands(operands)
            }
            StmtKind::Item(_) => None,
        }
    }

    /// Lifts one innermost-first operand-position candidate out of `expr`,
    /// rewriting the operand slot in place to read a fresh temp and returning
    /// the spine `let` bindings to splice before the enclosing statement.
    ///
    /// Mirrors the operand-slot enumeration of the `return_unify` ANF: every
    /// eager operand site is descended to find a deeper candidate first; a
    /// statement-carrying construct in an operand slot is lifted whole by its
    /// parent via [`is_candidate`]; an `If` condition is an unconditional
    /// operand site, while `If` branches and loop bodies are separate blocks
    /// normalized by the visitor recursion. A `While` condition is deliberately
    /// not an operand site: it is re-evaluated each iteration, so lifting it
    /// once to a spine temp would break per-iteration re-evaluation.
    ///
    /// # Before
    /// ```text
    /// <outer>(earlier(), { if c { break; } value })
    /// ```
    ///
    /// # After
    /// ```text
    /// let .operand_tmp_<id0> = earlier();
    /// let .operand_tmp_<id1> = { if c { break; } value };
    /// <outer>(.operand_tmp_<id0>, .operand_tmp_<id1>)
    /// ```
    ///
    /// # Mutations
    /// - Rewrites the lifted operand slot in place to read the fresh temp.
    /// - Allocates temp `Pat`/`Expr`/`Stmt` nodes through `assigner`.
    #[allow(clippy::too_many_lines)] // Exhaustive `ExprKind` operand-site dispatch.
    fn lift_once(&mut self, expr: &mut Expr) -> Option<Vec<Stmt>> {
        if !contains_control_flow(expr) {
            return None;
        }
        // A short-circuit `and`/`or` whose conditional right operand buries
        // escaping control flow is first reshaped in place into the equivalent
        // `If`, so the buried break/continue can reach a statement boundary
        // inside a branch block; the reshaped `If` is then re-dispatched here.
        if self.rewrite_short_circuit_rhs_in_place(expr) {
            return self.lift_once(expr);
        }
        match &mut expr.kind {
            // Short-circuit `and`/`or` and `and=`/`or=`: only the left side
            // evaluates unconditionally, so its buried control flow is lifted
            // here. A `BinOp` right operand that itself buries escaping control
            // flow was already reshaped into an `If` above, so any right operand
            // still reaching this arm is conditional and stays put. A compound
            // `and=`/`or=` with control flow in its right operand was already
            // reshaped into an `If`, so its assignment can be guarded as a whole.
            ExprKind::BinOp(BinOp::AndL | BinOp::OrL, lhs, _)
            | ExprKind::AssignOp(BinOp::AndL | BinOp::OrL, lhs, _) => {
                self.lift_short_circuit_lhs(lhs.as_mut())
            }
            // Assign family: the lvalue place never buries control flow, so only
            // the value operand is a lift slot.
            ExprKind::Assign(_, value)
            | ExprKind::AssignOp(_, _, value)
            | ExprKind::AssignField(_, _, value) => self.lift_operands(vec![value.as_mut()]),
            // AssignIndex: the place is excluded; the index and value are operands.
            ExprKind::AssignIndex(_, index, value) => {
                self.lift_operands(vec![index.as_mut(), value.as_mut()])
            }
            // Immutable update evaluates replacement/index operands before the
            // container, matching HIR-to-FIR lowering and preserving effects.
            ExprKind::UpdateField(record, _, replace) => {
                self.lift_operands(vec![replace.as_mut(), record.as_mut()])
            }
            ExprKind::UpdateIndex(container, index, replace) => {
                self.lift_operands(vec![index.as_mut(), replace.as_mut(), container.as_mut()])
            }
            // Two-operand eager compounds.
            ExprKind::ArrayRepeat(a, b)
            | ExprKind::BinOp(_, a, b)
            | ExprKind::Call(a, b)
            | ExprKind::Index(a, b) => self.lift_operands(vec![a.as_mut(), b.as_mut()]),
            // N-ary eager compounds.
            ExprKind::Array(exprs) | ExprKind::Tuple(exprs) => {
                self.lift_operands(exprs.iter_mut().collect())
            }
            // Single-operand eager compounds.
            ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
                self.lift_operands(vec![e.as_mut()])
            }
            // Optional operands in left-to-right order.
            ExprKind::Range(start, step, end) => {
                let operands = [start, step, end]
                    .into_iter()
                    .flatten()
                    .map(Box::as_mut)
                    .collect();
                self.lift_operands(operands)
            }
            // The `copy` operand, when present, evaluates before the field values.
            ExprKind::Struct(_, copy, fields) => {
                let mut operands: Vec<&mut Expr> = Vec::with_capacity(fields.len() + 1);
                if let Some(c) = copy {
                    operands.push(c.as_mut());
                }
                for field in fields.iter_mut() {
                    operands.push(field.value.as_mut());
                }
                self.lift_operands(operands)
            }
            // Interpolated string expression components, in source order.
            ExprKind::String(components) => {
                let operands = components
                    .iter_mut()
                    .filter_map(|c| match c {
                        StringComponent::Expr(e) => Some(e.as_mut()),
                        StringComponent::Lit(_) => None,
                    })
                    .collect();
                self.lift_operands(operands)
            }
            // An `If` condition is an unconditional operand site; the branches
            // are separate blocks visited independently by the recursion.
            ExprKind::If(cond, _, _) => self.lift_operands(vec![cond.as_mut()]),
            // A `return` operand may bury an escaping break/continue; lift the
            // operand to a temp while leaving the `return` node in place for
            // `loop_unification` to guard. The `return` is deliberately not
            // hoisted, because break/continue is not return control flow, so
            // only the buried divergence is exposed.
            ExprKind::Return(e) => self.lift_operands(vec![e.as_mut()]),
            // A `for` iterable is evaluated once in the enclosing scope; lift it
            // so a buried break/continue carried by a compound iterable is
            // exposed at statement position without hoisting the `for` itself.
            // The body is a separate block normalized by the visitor recursion.
            ExprKind::For(_, iterable, _) => self.lift_operands(vec![iterable.as_mut()]),
            // Recursion leaves: statement-carrying constructs are lifted whole
            // by their parent (never descended here), and the remaining kinds
            // carry no operand slot.
            ExprKind::Block(_)
            | ExprKind::Break
            | ExprKind::Closure(_, _)
            | ExprKind::Conjugate(_, _)
            | ExprKind::Continue
            | ExprKind::Err
            | ExprKind::Hole
            | ExprKind::Lit(_)
            | ExprKind::Repeat(_, _, _)
            | ExprKind::Var(_, _)
            | ExprKind::While(_, _) => None,
        }
    }

    fn lift_short_circuit_lhs(&mut self, lhs: &mut Expr) -> Option<Vec<Stmt>> {
        self.lift_operands(vec![lhs])
    }

    /// Reshapes a short-circuit `and`/`or` whose conditional right operand
    /// buries escaping control flow into the equivalent `If`, in place,
    /// returning `true` when a reshape was applied.
    ///
    /// Only the left operand of `and`/`or` evaluates unconditionally, so a
    /// `break`/`continue` buried in the right operand is reachable only on the
    /// not-short-circuited path. Exposing that path as an explicit branch lets
    /// the buried control flow later reach a statement boundary:
    ///
    /// ```text
    /// a and <rhs>       ->  if a { <rhs> } else { false }
    /// a or  <rhs>       ->  if a { true } else { <rhs> }
    /// set p and= <rhs>  ->  if p     { set p = <rhs> }
    /// set p or=  <rhs>  ->  if not p { set p = <rhs> }
    /// ```
    ///
    /// The reshaped value `If` keeps the operator's `Bool` type and span; the
    /// reshaped assignment `If` is `Unit`-typed with no `else`, since the
    /// omitted branch is a no-op because `p` already holds the short-circuit
    /// result. Unlike FIR — where `If` branches are bare expressions — HIR `If`
    /// branches are `Block`-typed exprs, so each branch is a fresh
    /// single-statement block.
    ///
    /// A short-circuit whose right operand carries no escaping control flow is
    /// left untouched, and its left operand is lifted by the caller instead. For
    /// the `and=`/`or=` form the assignment itself must be reshaped whenever the
    /// right operand contains control flow. Even when an `If` already exposes a
    /// `break` at a branch boundary, leaving the compound assignment intact would
    /// let the desugar's synthesized branch value commit to the assignment after
    /// the break. The assignment place is a simple mutable variable, so re-reading
    /// it for the guard condition is side-effect-free. Mirrors
    /// `return_unify::normalize::hoist_short_circuit`.
    fn rewrite_short_circuit_rhs_in_place(&mut self, expr: &mut Expr) -> bool {
        // Classify the short-circuit form first so the borrow of `expr.kind`
        // ends before the in-place rewrite mutates it.
        let form = match &expr.kind {
            ExprKind::BinOp(op @ (BinOp::AndL | BinOp::OrL), _, rhs)
                if contains_control_flow(rhs) =>
            {
                ShortCircuitForm::BinOp(matches!(op, BinOp::AndL))
            }
            ExprKind::AssignOp(op @ (BinOp::AndL | BinOp::OrL), place, rhs)
                if contains_control_flow(rhs) && matches!(place.kind, ExprKind::Var(_, _)) =>
            {
                ShortCircuitForm::AssignOp(matches!(op, BinOp::AndL))
            }
            _ => return false,
        };
        let span = expr.span;
        match form {
            ShortCircuitForm::BinOp(is_and) => {
                let ExprKind::BinOp(_, cond, rhs) = std::mem::take(&mut expr.kind) else {
                    unreachable!("expr.kind was just matched as a short-circuit BinOp");
                };
                let rhs_branch = self.wrap_expr_in_bool_block(rhs);
                let lit_branch = self.gen_bool_lit_block(!is_and, span);
                let (then_branch, else_branch) = if is_and {
                    (rhs_branch, lit_branch)
                } else {
                    (lit_branch, rhs_branch)
                };
                expr.kind = ExprKind::If(cond, Box::new(then_branch), Some(Box::new(else_branch)));
            }
            ShortCircuitForm::AssignOp(is_and) => {
                let ExprKind::AssignOp(_, place, rhs) = std::mem::take(&mut expr.kind) else {
                    unreachable!("expr.kind was just matched as a short-circuit AssignOp");
                };
                let cond = self.gen_place_guard_condition(&place, is_and);
                let assign_block = self.wrap_assign_in_unit_block(place, rhs);
                expr.kind = ExprKind::If(Box::new(cond), Box::new(assign_block), None);
            }
        }
        true
    }

    /// Builds the guard condition for a reshaped compound short-circuit
    /// assignment: a fresh read of the assignment's place for `and=`, or its
    /// logical negation for `or=`. The place is a simple mutable variable
    /// reference, so re-reading it is side-effect-free.
    fn gen_place_guard_condition(&mut self, place: &Expr, is_and: bool) -> Expr {
        let read = Expr {
            id: self.assigner.next_node(),
            span: place.span,
            ty: place.ty.clone(),
            kind: place.kind.clone(),
        };
        if is_and {
            read
        } else {
            Expr {
                id: self.assigner.next_node(),
                span: place.span,
                ty: Ty::Prim(Prim::Bool),
                kind: ExprKind::UnOp(UnOp::NotL, Box::new(read)),
            }
        }
    }

    /// Wraps `set <place> = <rhs>` in a fresh `Unit`-typed single-statement
    /// block expr, for use as the `then` branch of a reshaped compound
    /// short-circuit assignment.
    fn wrap_assign_in_unit_block(&mut self, place: Box<Expr>, rhs: Box<Expr>) -> Expr {
        let span = rhs.span;
        let assign = Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: ExprKind::Assign(place, rhs),
        };
        let stmt = Stmt {
            id: self.assigner.next_node(),
            span,
            kind: StmtKind::Semi(assign),
        };
        self.gen_block(vec![stmt], Ty::UNIT, span)
    }

    /// Wraps `expr` (a `Bool`-valued operand) as a fresh single-statement block
    /// expr typed `Bool`, for use as an `If` branch when reshaping a
    /// short-circuit operator.
    fn wrap_expr_in_bool_block(&mut self, expr: Box<Expr>) -> Expr {
        let span = expr.span;
        let stmt = Stmt {
            id: self.assigner.next_node(),
            span,
            kind: StmtKind::Expr(*expr),
        };
        self.gen_bool_block(vec![stmt], span)
    }

    /// Builds a fresh single-statement block expr whose trailing value is the
    /// `Bool` literal `value`, typed `Bool`, for use as the constant `If`
    /// branch when reshaping a short-circuit operator.
    fn gen_bool_lit_block(&mut self, value: bool, span: Span) -> Expr {
        let lit = Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::Lit(Lit::Bool(value)),
        };
        let stmt = Stmt {
            id: self.assigner.next_node(),
            span,
            kind: StmtKind::Expr(lit),
        };
        self.gen_bool_block(vec![stmt], span)
    }

    /// Wraps `stmts` in a fresh `Bool`-typed block expr.
    fn gen_bool_block(&mut self, stmts: Vec<Stmt>, span: Span) -> Expr {
        self.gen_block(stmts, Ty::Prim(Prim::Bool), span)
    }

    /// Wraps `stmts` in a fresh block expr of type `ty`.
    fn gen_block(&mut self, stmts: Vec<Stmt>, ty: Ty, span: Span) -> Expr {
        Expr {
            id: self.assigner.next_node(),
            span,
            ty: ty.clone(),
            kind: ExprKind::Block(Block {
                id: self.assigner.next_node(),
                span,
                ty,
                stmts,
            }),
        }
    }

    /// Lifts one operand from an ordered operand list, innermost-first.
    ///
    /// First recurses into each operand to lift a deeper candidate; only if
    /// none is found does it lift the first directly-liftable operand at this
    /// level, pinning every earlier operand to its own spine temp so
    /// left-to-right evaluation order is preserved.
    fn lift_operands(&mut self, mut operands: Vec<&mut Expr>) -> Option<Vec<Stmt>> {
        // Innermost-first: try to lift a deeper operand inside any child first.
        for op in &mut operands {
            if let Some(stmts) = self.lift_once(op) {
                return Some(stmts);
            }
        }
        // No deeper candidate; lift the first liftable operand at this level.
        let idx = operands.iter().position(|op| is_candidate(op))?;
        let mut out = Vec::with_capacity(idx + 1);
        for (i, op) in operands.iter_mut().enumerate() {
            if i < idx {
                // Pin each earlier operand to preserve evaluation order.
                out.push(self.bind_temp(op, false));
            } else if i == idx {
                out.push(self.bind_temp(op, true));
                break;
            }
        }
        Some(out)
    }

    /// Binds `op` to a fresh immutable `let .operand_tmp_<id> = <op>;` on the
    /// statement spine and rewrites `op` in place to read the temp.
    ///
    /// When `check_defaultable` is set, the lifted candidate can diverge before
    /// producing a value, so the temp must have a well-typed placeholder on the
    /// divergence path. A type with a classical default is bound directly; any
    /// other representable type is array-backed by
    /// [`Self::bind_array_backed_temp`]; only a genuinely-unrepresentable type
    /// records [`Error::UnsupportedType`], a defensive guard that is unreachable
    /// for a well-typed operand post-typecheck, before falling through to a
    /// direct bind so the tree stays well-formed. Pinned earlier siblings always
    /// produce a value, so they are bound directly without any check.
    fn bind_temp(&mut self, op: &mut Expr, check_defaultable: bool) -> Stmt {
        let ty = op.ty.clone();
        let span = op.span;
        if check_defaultable && !is_defaultable(&ty) {
            if is_representable(&ty) {
                return self.bind_array_backed_temp(op, &ty, span);
            }
            self.errors
                .push(Error::UnsupportedType(format!("{ty}"), span));
        }
        let ident = gen_ident(self.assigner, "operand_tmp", ty, span);
        let init = std::mem::replace(op, ident.gen_local_ref(self.assigner));
        ident.gen_id_init(Mutability::Immutable, init, self.assigner)
    }

    /// If `stmt` is a `let` binding whose initializer buries escaping control
    /// flow and whose representable type has no classical default, array-backs
    /// the initializer and returns the `let .operand_tmp_<id> : ty[] = <arrayified
    /// init>` statement to splice before the binding; the binding itself is
    /// rewritten in place to read the value back through `.operand_tmp_<id>[0]`.
    /// Returns `None` for any other statement.
    ///
    /// A binding such as `let x = if c { break } else v`, where `x` has no
    /// classical default, cannot be guarded in place by the later desugar,
    /// because the divergence path would need a default of `x`'s type.
    /// Array-backing seeds that path with the universal `[]` default of `ty[]`
    /// instead, and the desugar relocates the guarded `.operand_tmp_<id>[0]` read
    /// into the fall-through branch, so the binding never needs a default of its
    /// own type. A defaultable binding, a binding whose initializer carries no
    /// escaping control flow, and an unrepresentable type are all left
    /// untouched.
    fn array_back_control_flow_local(&mut self, stmt: &mut Stmt) -> Option<Stmt> {
        let StmtKind::Local(_, _, init) = &mut stmt.kind else {
            return None;
        };
        if is_defaultable(&init.ty) || !contains_control_flow(init) || !is_representable(&init.ty) {
            return None;
        }
        let ty = init.ty.clone();
        let span = init.span;
        Some(self.bind_array_backed_temp(init, &ty, span))
    }

    /// Array-backs a discarded value-block or `if` statement of the form
    /// `<value>;` that buries escaping control flow and whose type has no
    /// classical default, for example `{ if c { break } else { Complex(..) } };`.
    /// Because the value is discarded, it is retyped to `T[]` in place: each
    /// produced leaf is wrapped as `[leaf]` while the buried break/continue keeps
    /// its value position, so the value-block gains the universal `[]` default of
    /// `T[]` on the divergence path and the later desugar needs no default of
    /// `T`.
    ///
    /// Only a `Block`/`If` value is considered: a bare `break`/`continue`
    /// statement and a loop are handled directly by the desugar, and an operand-
    /// position value has already been lifted by the fixpoint above. A
    /// defaultable value, a value without escaping control flow, and an
    /// unrepresentable type are all left untouched.
    fn array_back_discarded_control_flow_value(&mut self, stmt: &mut Stmt) {
        let StmtKind::Semi(value) = &mut stmt.kind else {
            return;
        };
        if !matches!(value.kind, ExprKind::Block(_) | ExprKind::If(_, _, _))
            || is_defaultable(&value.ty)
            || !is_representable(&value.ty)
            || !contains_control_flow(value)
        {
            return;
        }
        let ty = value.ty.clone();
        self.arrayify_value_in_place(value, &ty);
    }

    /// Binds `op` (a lifted candidate of the non-defaultable type `ty`) to a
    /// fresh `let .operand_tmp_<id> : ty[] = <arrayified op>;` and rewrites `op`
    /// in place to read the single stored element back through
    /// `.operand_tmp_<id>[0]`.
    ///
    /// Storing the value behind a length-1 array lets the later desugar seed the
    /// divergence path with the universal `[]` default of `ty[]`, so no
    /// classical default of `ty` itself is needed. The `[0]` read is only
    /// reached on the fall-through path, which the desugar guards behind the
    /// `break`/`continue` flags, so the empty array is never indexed. This
    /// mirrors the array-backed return slot of the `return_unify` FIR transform.
    fn bind_array_backed_temp(&mut self, op: &mut Expr, ty: &Ty, span: Span) -> Stmt {
        let array_ty = Ty::Array(Box::new(ty.clone()));
        let ident = gen_ident(self.assigner, "operand_tmp", array_ty, span);
        let read = self.gen_array_backed_read(&ident, ty, &[]);
        let mut init = std::mem::replace(op, read);
        self.arrayify_value_in_place(&mut init, ty);
        ident.gen_id_init(Mutability::Immutable, init, self.assigner)
    }

    /// Reads an array-backed value while preserving explicit tuple structure.
    /// Controlled-call analysis peels control layers from tuple expressions, so
    /// tuple-valued temps are rebuilt from projections instead of exposed as one
    /// opaque index expression.
    fn gen_array_backed_read(&mut self, ident: &IdentTemplate, ty: &Ty, path: &[usize]) -> Expr {
        if let Ty::Tuple(items) = ty {
            let items = items
                .iter()
                .enumerate()
                .map(|(index, item_ty)| {
                    let mut item_path = path.to_vec();
                    item_path.push(index);
                    self.gen_array_backed_read(ident, item_ty, &item_path)
                })
                .collect();
            return Expr {
                id: self.assigner.next_node(),
                span: ident.span,
                ty: ty.clone(),
                kind: ExprKind::Tuple(items),
            };
        }

        let Ty::Array(elem_ty) = &ident.ty else {
            unreachable!("array-backed temporary should have array type");
        };
        let read = self.gen_singleton_index_read(ident, elem_ty);
        if path.is_empty() {
            return read;
        }
        Expr {
            id: self.assigner.next_node(),
            span: ident.span,
            ty: ty.clone(),
            kind: ExprKind::Field(
                Box::new(read),
                Field::Path(FieldPath {
                    indices: path.to_vec(),
                }),
            ),
        }
    }

    /// Builds `@<ident>[0]`, reading element 0 (of type `elem_ty`) out of the
    /// array-backed temp `<ident>` (of type `elem_ty[]`).
    fn gen_singleton_index_read(&mut self, ident: &IdentTemplate, elem_ty: &Ty) -> Expr {
        let array_ref = ident.gen_local_ref(self.assigner);
        let zero = Expr {
            id: self.assigner.next_node(),
            span: ident.span,
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Lit(Lit::Int(0)),
        };
        Expr {
            id: self.assigner.next_node(),
            span: ident.span,
            ty: elem_ty.clone(),
            kind: ExprKind::Index(Box::new(array_ref), Box::new(zero)),
        }
    }

    /// Retypes the value produced by `expr` from `elem_ty` to `elem_ty[]` in
    /// place, recursing through the value-producing positions a control-flow-
    /// bearing candidate can take, namely a `Block`'s trailing value and both
    /// `If` branches, so the buried `break`/`continue` is never moved out of
    /// statement position.
    ///
    /// A divergent `break`/`continue`/`return` produces no value, so only its
    /// surrounding type is adjusted; wrapping it as an array element would
    /// re-bury it in an operand position and re-trigger the lift. Any other leaf
    /// is a genuine `elem_ty`-valued result and is replaced with the
    /// single-element array `[ <leaf> ]`. A `While`/`For`/`Repeat` is always
    /// `Unit`-typed, and `Unit` has a classical default, so a loop never takes
    /// the array-backed path and never reaches this helper. Mirrors the
    /// array-backing of the `return_unify` FIR transform.
    fn arrayify_value_in_place(&mut self, expr: &mut Expr, elem_ty: &Ty) {
        let array_ty = Ty::Array(Box::new(elem_ty.clone()));
        match &mut expr.kind {
            ExprKind::Block(block) => {
                self.arrayify_block_tail_value(block, elem_ty);
                expr.ty = array_ty;
            }
            ExprKind::If(_, then_branch, otherwise) => {
                self.arrayify_value_in_place(then_branch, elem_ty);
                if let Some(else_branch) = otherwise {
                    self.arrayify_value_in_place(else_branch, elem_ty);
                }
                expr.ty = array_ty;
            }
            ExprKind::Break | ExprKind::Continue | ExprKind::Return(_) => {
                expr.ty = array_ty;
            }
            _ => self.wrap_leaf_value_in_place(expr, elem_ty),
        }
    }

    /// Retypes a block's trailing value, its last `Expr` statement if any, to
    /// `elem_ty[]` and sets the block's own type to match.
    fn arrayify_block_tail_value(&mut self, block: &mut Block, elem_ty: &Ty) {
        if let Some(Stmt {
            kind: StmtKind::Expr(tail),
            ..
        }) = block.stmts.last_mut()
        {
            self.arrayify_value_in_place(tail, elem_ty);
        }
        block.ty = Ty::Array(Box::new(elem_ty.clone()));
    }

    /// Replaces an `elem_ty`-valued leaf `expr` in place with the single-element
    /// array literal `[ <leaf> ]` of type `elem_ty[]`, moving the original leaf
    /// with all its children intact inside the new array.
    fn wrap_leaf_value_in_place(&mut self, expr: &mut Expr, elem_ty: &Ty) {
        let span = expr.span;
        let inner = std::mem::take(expr);
        *expr = Expr {
            id: self.assigner.next_node(),
            span,
            ty: Ty::Array(Box::new(elem_ty.clone())),
            kind: ExprKind::Array(vec![inner]),
        };
    }
}

impl MutVisitor for LoopNormalize<'_> {
    fn visit_block(&mut self, block: &mut Block) {
        let stmts = std::mem::take(&mut block.stmts);
        let mut out = Vec::with_capacity(stmts.len());
        for mut stmt in stmts {
            // Per-statement fixpoint: lift operand-position candidates from the
            // surface expression into preceding `let` bindings until stable.
            while let Some(prefix) = self.lift_stmt_surface(&mut stmt) {
                for mut lifted in prefix {
                    // Normalize nested blocks and deeper operands inside each
                    // lifted temp's initializer.
                    self.visit_stmt(&mut lifted);
                    out.push(lifted);
                }
            }
            // Array-back a non-defaultable `let` binding whose initializer buries
            // escaping control flow, for example `let x = if c { break } else v`
            // where `x` has no classical default. The value is stored behind a
            // defaultable array temp, so the buried break/continue reaches a
            // statement boundary without the binding needing a default of its own
            // type; the `loop_unification` desugar then relocates the guarded
            // read into the fall-through branch.
            if let Some(mut backing) = self.array_back_control_flow_local(&mut stmt) {
                self.visit_stmt(&mut backing);
                out.push(backing);
            }
            // Array-back a discarded non-defaultable value-block statement, whose
            // result is dropped, so a break/continue buried in it desugars
            // without a default of the value's type.
            self.array_back_discarded_control_flow_value(&mut stmt);
            // Normalize nested blocks such as branches, loop bodies, and block
            // exprs, along with any remaining sub-structure of the finalized
            // statement.
            self.visit_stmt(&mut stmt);
            out.push(stmt);
        }
        block.stmts = out;
    }
}

/// Collects qubit-array length expressions in initializer evaluation order.
fn collect_qubit_init_operands<'a>(init: &'a mut QubitInit, operands: &mut Vec<&'a mut Expr>) {
    match &mut init.kind {
        QubitInitKind::Array(length) => operands.push(length),
        QubitInitKind::Tuple(inits) => {
            for init in inits {
                collect_qubit_init_operands(init, operands);
            }
        }
        QubitInitKind::Single | QubitInitKind::Err => {}
    }
}

/// Returns `true` when `expr` is an operand subexpression the lift should bind
/// to a spine temp: a bare `break`/`continue`, or a statement-carrying
/// `Block`/`If`/`While`/`For`/`Repeat`, that contains control flow escaping
/// `expr`.
///
/// A bare `break`/`continue` sitting directly in an operand slot, for example
/// `Foo(break)` or `arr[break]`, is itself the escaping control flow, so it is
/// lifted to its own spine temp; the later desugar then guards the enclosing
/// operator behind the `break`/`continue` flags. Only the divergence is
/// exposed — the temp's placeholder value is never observed.
fn is_candidate(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Block(_)
            | ExprKind::If(_, _, _)
            | ExprKind::While(_, _)
            | ExprKind::For(_, _, _)
            | ExprKind::Repeat(_, _, _)
    ) && contains_control_flow(expr)
}

/// Read-only check whether `ty` has a classical default the `loop_unification`
/// desugar can materialize directly through its `build_default_kind`. This
/// selects the direct binding strategy; any other representable type is
/// array-backed instead. Kept in exact agreement with `build_default_kind` so
/// the two passes never disagree on which types are directly defaultable;
/// `Arrow` and every user-defined type are not.
fn is_defaultable(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(Prim::Qubit)
        | Ty::Arrow(_)
        | Ty::Udt(..)
        | Ty::Infer(_)
        | Ty::Param { .. }
        | Ty::Err => false,
        Ty::Prim(_) | Ty::Array(_) => true,
        Ty::Tuple(elems) => elems.iter().all(is_defaultable),
    }
}

/// Read-only check whether an array-backed temp of `ty` can be synthesized.
/// The universal `[]` default of `ty[]` is well-typed whenever `ty`'s structure
/// is resolvable, so a resolved user-defined type is representable regardless of
/// which package defines it: array-backing needs only the `[]` default of `T[]`,
/// never a default of the user-defined type itself. This excludes only
/// genuinely-unresolvable leaves: inference and type-parameter placeholders, the
/// error type, and an unresolved user-defined type. None of these can occur for
/// a well-typed operand post-typecheck, so the resulting rejection is a
/// defensive guard.
fn is_representable(ty: &Ty) -> bool {
    match ty {
        Ty::Prim(_) | Ty::Array(_) | Ty::Arrow(_) | Ty::Udt(_, Res::Item(_)) => true,
        Ty::Tuple(elems) => elems.iter().all(is_representable),
        Ty::Udt(_, _) | Ty::Infer(_) | Ty::Param { .. } | Ty::Err => false,
    }
}

/// Returns `true` when `expr` contains a `break`/`continue` that escapes to
/// the enclosing statement context, i.e. one binding to a loop outside `expr`.
///
/// `return` is deliberately not treated as escaping: it is unified downstream
/// by `return_unify` on the FIR codegen path, so hoisting it here would change
/// return handling for no benefit to the loop desugar.
fn contains_control_flow(expr: &Expr) -> bool {
    let mut found = false;
    EnclosingBreakContinueScan::new(|_: &Expr| found = true).visit_expr(expr);
    found
}
