// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Result-slot use analysis for reusable IR-function bodies.
//!
//! Raw results carry tokens identifying their measuring source body. Re-entering
//! that body makes earlier tokens stale; reading a stale token fails the proof.
//! Unused aliases may remain in scope, and consumed Boolean values carry no token.
//! Branches merge possible aliases and stale tokens conservatively. Loop-carried
//! results and unresolved escapes remain unproved.

use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, ExprId, ExprKind, Global, LocalVarId, PackageId, PackageLookup,
    PackageStore, PackageStoreLookup, PatId, PatKind, Res, StmtKind, StringComponent,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    ResourceCallScanner, get_spec_decl, is_measurement_forwarder,
    measurement_type_may_contain_result,
};

type Body = (PackageId, BlockId);
type Results = FxHashSet<usize>;

#[derive(Clone, Default)]
struct ResultState {
    locals: FxHashMap<LocalVarId, Results>,
    stale: Results,
}

impl ResultState {
    fn join(&mut self, other: Self) {
        self.stale.extend(other.stale);
        for (local, results) in other.locals {
            self.locals.entry(local).or_default().extend(results);
        }
    }

    fn has_results(&self) -> bool {
        self.locals.values().any(|results| !results.is_empty())
    }
}

pub(super) fn measurement_uses_are_safe(
    store: &PackageStore,
    edges: &FxHashMap<Body, Vec<Body>>,
    measuring: &FxHashSet<Body>,
    parameters: &FxHashMap<Body, Vec<LocalVarId>>,
) -> bool {
    let reachable: FxHashMap<_, _> = edges
        .iter()
        .map(|(body, callees)| {
            let mut pending = callees.clone();
            let mut seen = FxHashSet::default();
            while let Some(callee) = pending.pop() {
                if seen.insert(callee) {
                    pending.extend(&edges[&callee]);
                }
            }
            (*body, seen)
        })
        .collect();
    // A Result argument is only consumed by a proved acyclic, nonmeasuring helper.
    // A recursive receiver could observe its caller's overwritten measurement slot.
    let consumers: FxHashSet<_> = parameters
        .keys()
        .filter(|body| {
            !measuring.contains(body)
                && !reachable[body].contains(body)
                && reachable[body].is_disjoint(measuring)
        })
        .copied()
        .collect();
    if parameters.keys().any(|body| !consumers.contains(body)) {
        return false;
    }

    edges.keys().all(|body| {
        if is_measurement_forwarder(store, *body) {
            return true;
        }
        let mut analysis = ResultUseAnalysis {
            store,
            body: *body,
            reachable: &reachable,
            consumers: &consumers,
            origins: Vec::new(),
            safe: true,
        };
        let mut state = ResultState::default();
        if let Some(params) = parameters.get(body) {
            for param in params {
                state.locals.insert(*param, analysis.new_result(*body));
            }
        }
        let output = analysis.block(body.1, &mut state);
        analysis.safe && output.is_empty()
    })
}

struct ResultUseAnalysis<'a> {
    store: &'a PackageStore,
    body: Body,
    reachable: &'a FxHashMap<Body, FxHashSet<Body>>,
    consumers: &'a FxHashSet<Body>,
    // A fresh token keeps old aliases stale when a later measurement is evaluated.
    origins: Vec<Body>,
    safe: bool,
}

impl ResultUseAnalysis<'_> {
    fn new_result(&mut self, origin: Body) -> Results {
        let token = self.origins.len();
        self.origins.push(origin);
        [token].into_iter().collect()
    }

    fn read(&mut self, results: &Results, state: &ResultState) {
        self.safe &= results.is_disjoint(&state.stale);
    }

    fn bind(&self, pat: PatId, results: &Results, state: &mut ResultState) {
        match &self.store.get(self.body.0).get_pat(pat).kind {
            PatKind::Bind(ident) => {
                state.locals.insert(ident.id, results.clone());
            }
            PatKind::Tuple(items) => {
                for item in items {
                    self.bind(*item, results, state);
                }
            }
            PatKind::Discard => {}
        }
    }

    fn assign(&mut self, lhs: ExprId, results: &Results, state: &mut ResultState) {
        match &self.store.get(self.body.0).get_expr(lhs).kind {
            ExprKind::Var(Res::Local(local), _) => {
                state.locals.insert(*local, results.clone());
            }
            ExprKind::Tuple(items) => {
                // As with tuple bindings, each destination conservatively retains
                // all RHS origins. The RHS has already been evaluated once.
                for item in items {
                    self.assign(*item, results, state);
                }
            }
            ExprKind::Hole => {}
            _ => self.safe = false,
        }
    }

    fn block(&mut self, block: BlockId, state: &mut ResultState) -> Results {
        let outer: FxHashSet<_> = state.locals.keys().copied().collect();
        let mut output = Results::default();
        let package = self.store.get(self.body.0);
        for stmt in &package.get_block(block).stmts {
            output = match package.get_stmt(*stmt).kind {
                StmtKind::Local(_, pat, expr) => {
                    let results = self.expr(expr, state);
                    self.bind(pat, &results, state);
                    Results::default()
                }
                StmtKind::Expr(expr) => self.expr(expr, state),
                StmtKind::Semi(expr) => {
                    self.expr(expr, state);
                    Results::default()
                }
                StmtKind::Item(_) => Results::default(),
            };
        }
        state.locals.retain(|local, _| outer.contains(local));
        output
    }

    fn call(&mut self, callee: ExprId, arg: ExprId, state: &mut ResultState) -> Results {
        let package = self.store.get(self.body.0);
        let resolver = ResourceCallScanner {
            package,
            package_id: self.body.0,
            calls: Vec::new(),
            unresolved: false,
        };
        let Some((item, functors)) = resolver.resolve_callee(callee) else {
            self.safe = false;
            return Results::default();
        };
        self.expr(callee, state);
        let args = self.expr(arg, state);
        self.read(&args, state);
        let Some(Global::Callable(callable)) = self.store.get_global(item) else {
            self.safe = false;
            return Results::default();
        };
        let body = if let CallableImpl::Spec(specs) = &callable.implementation {
            Some((item.package, get_spec_decl(specs, functors).block))
        } else {
            None
        };
        if body.is_some_and(|body| is_measurement_forwarder(self.store, body))
            || (body.is_none()
                && matches!(
                    callable.name.name.as_ref(),
                    "__quantum__qis__m__body" | "__quantum__qis__mresetz__body"
                ))
        {
            self.safe &= args.is_empty();
            // Track the measuring source body without assuming which later call
            // sites will inline it into distinct slots.
            return self.new_result(body.unwrap_or(self.body));
        }
        if !args.is_empty() && !body.is_some_and(|body| self.consumers.contains(&body)) {
            self.safe = false;
        }
        if let Some(body) = body {
            for (token, origin) in self.origins.iter().enumerate() {
                if *origin == body || self.reachable[&body].contains(origin) {
                    state.stale.insert(token);
                }
            }
        }
        // Other raw-result returns and unknown nominal storage may escape analysis.
        self.safe &= !measurement_type_may_contain_result(&callable.output);
        Results::default()
    }

    #[allow(clippy::too_many_lines)]
    fn expr(&mut self, expr: ExprId, state: &mut ResultState) -> Results {
        if !self.safe {
            return Results::default();
        }
        let package = self.store.get(self.body.0);
        match &package.get_expr(expr).kind {
            ExprKind::Var(Res::Local(local), _) => {
                if !state.locals.contains_key(local)
                    && measurement_type_may_contain_result(&package.get_expr(expr).ty)
                {
                    self.safe = false;
                }
                let results = state.locals.get(local).cloned().unwrap_or_default();
                self.read(&results, state);
                results
            }
            ExprKind::Var(_, _) | ExprKind::Lit(_) | ExprKind::Hole => Results::default(),
            ExprKind::Call(callee, arg) => self.call(*callee, *arg, state),
            ExprKind::Block(block) => self.block(*block, state),
            ExprKind::Array(items) | ExprKind::ArrayLit(items) | ExprKind::Tuple(items) => {
                let mut results = Results::default();
                for item in items {
                    results.extend(self.expr(*item, state));
                }
                results
            }
            ExprKind::Field(value, _) | ExprKind::UnOp(_, value) => self.expr(*value, state),
            ExprKind::Index(array, index) | ExprKind::ArrayRepeat(array, index) => {
                let results = self.expr(*array, state);
                let index_results = self.expr(*index, state);
                self.safe &= index_results.is_empty();
                self.read(&results, state);
                results
            }
            ExprKind::BinOp(BinOp::AndL | BinOp::OrL, lhs, rhs)
            | ExprKind::AssignOp(BinOp::AndL | BinOp::OrL, lhs, rhs) => {
                let lhs = self.expr(*lhs, state);
                self.safe &= lhs.is_empty();
                let skipped = state.clone();
                let rhs = self.expr(*rhs, state);
                self.safe &= rhs.is_empty();
                state.join(skipped);
                Results::default()
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                let left = self.expr(*lhs, state);
                let right = self.expr(*rhs, state);
                // Reading either operand occurs after both operand expressions run.
                self.read(&left, state);
                self.read(&right, state);
                // Comparisons consume results; other operations can retain them,
                // for example concatenating arrays of raw measurement results.
                self.safe &=
                    matches!(op, BinOp::Eq | BinOp::Neq) || (left.is_empty() && right.is_empty());
                Results::default()
            }
            ExprKind::AssignOp(_, lhs, rhs) => {
                let left = self.expr(*lhs, state);
                let right = self.expr(*rhs, state);
                self.safe &= left.is_empty() && right.is_empty();
                Results::default()
            }
            ExprKind::Assign(lhs, rhs) => {
                let results = self.expr(*rhs, state);
                self.assign(*lhs, &results, state);
                Results::default()
            }
            ExprKind::If(condition, yes, no) => {
                let condition = self.expr(*condition, state);
                self.safe &= condition.is_empty();
                let mut otherwise = state.clone();
                let mut results = self.expr(*yes, state);
                if let Some(no) = no {
                    results.extend(self.expr(*no, &mut otherwise));
                }
                state.join(otherwise);
                results
            }
            ExprKind::While(condition, block) => {
                // Raw results crossing a loop boundary need a fixed-point proof.
                // For now only loop-local, nonescaping measurements are supported.
                self.safe &= !state.has_results();
                let condition = self.expr(*condition, state);
                self.safe &= condition.is_empty() && !state.has_results();
                let skipped = state.clone();
                let results = self.block(*block, state);
                self.safe &= results.is_empty() && !state.has_results();
                state.join(skipped);
                Results::default()
            }
            ExprKind::UpdateIndex(array, index, replacement) => {
                let index = self.expr(*index, state);
                self.safe &= index.is_empty();
                let mut results = self.expr(*replacement, state);
                results.extend(self.expr(*array, state));
                self.read(&results, state);
                results
            }
            ExprKind::AssignIndex(array, index, replacement) => {
                let index = self.expr(*index, state);
                let replacement = self.expr(*replacement, state);
                let array = self.expr(*array, state);
                self.safe &= index.is_empty() && replacement.is_empty() && array.is_empty();
                Results::default()
            }
            ExprKind::AssignField(record, _, value) | ExprKind::UpdateField(record, _, value) => {
                let mut results = self.expr(*record, state);
                results.extend(self.expr(*value, state));
                self.safe &= results.is_empty();
                Results::default()
            }
            ExprKind::Closure(captures, _) => {
                self.safe &= captures
                    .iter()
                    .all(|local| state.locals.get(local).is_none_or(Results::is_empty));
                Results::default()
            }
            ExprKind::Range(start, step, end) => {
                for part in [start, step, end].into_iter().flatten() {
                    let results = self.expr(*part, state);
                    self.safe &= results.is_empty();
                }
                Results::default()
            }
            ExprKind::String(parts) => {
                for part in parts {
                    if let StringComponent::Expr(expr) = part {
                        let results = self.expr(*expr, state);
                        self.safe &= results.is_empty();
                    }
                }
                Results::default()
            }
            ExprKind::Fail(value) => {
                self.expr(*value, state);
                Results::default()
            }
            ExprKind::Return(_) | ExprKind::Parallel(..) | ExprKind::Struct(..) => {
                self.safe = false;
                Results::default()
            }
        }
    }
}
