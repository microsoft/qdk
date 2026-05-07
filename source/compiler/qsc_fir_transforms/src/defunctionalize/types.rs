// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared types for the defunctionalization pass.
//!
//! These types are used across the analysis, specialization, and rewrite
//! modules to communicate discovered callable parameters, call sites,
//! concrete callable resolutions, and specialization keys.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use rustc_hash::FxHashMap;
use thiserror::Error;

use qsc_data_structures::functors::FunctorApp;
use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    ExprId, ExprKind, Functor, ItemId, LocalItemId, LocalVarId, Package, PackageLookup, PatId, UnOp,
};
use qsc_fir::ty::Ty;

/// A callable parameter detected in a higher-order function declaration.
#[derive(Clone, Debug)]
pub struct CallableParam {
    /// The HOF containing this parameter.
    pub callable_id: LocalItemId,
    /// The pattern node for the parameter.
    pub param_pat_id: PatId,
    /// The outer input-parameter slot selected before any nested tuple
    /// traversal. Single-parameter callables always use `0`.
    pub top_level_param: usize,
    /// The tuple-field path relative to `top_level_param`.
    pub field_path: Vec<usize>,
    /// The local variable bound by the parameter.
    pub param_var: LocalVarId,
    /// The Arrow type of the parameter.
    pub param_ty: Ty,
}

impl CallableParam {
    #[must_use]
    pub fn new(
        callable_id: LocalItemId,
        param_pat_id: PatId,
        top_level_param: usize,
        field_path: Vec<usize>,
        param_var: LocalVarId,
        param_ty: Ty,
    ) -> Self {
        Self {
            callable_id,
            param_pat_id,
            top_level_param,
            field_path,
            param_var,
            param_ty,
        }
    }
}

/// A call site where a HOF is called with a concrete callable argument.
#[derive(Clone, Debug)]
pub struct CallSite {
    /// The Call expression.
    pub call_expr_id: ExprId,
    /// The HOF being called.
    pub hof_item_id: ItemId,
    /// Resolved callable argument.
    pub callable_arg: ConcreteCallable,
    /// Expression for the callable argument.
    pub arg_expr_id: ExprId,
    /// Optional condition `ExprId` for branch-split dispatch. When
    /// present, this callee is selected when the condition is true.
    /// `None` indicates the default (else) branch.
    pub condition: Option<ExprId>,
}

/// A direct call whose callee expression resolves to a concrete callable value.
#[derive(Clone, Debug)]
pub struct DirectCallSite {
    /// The Call expression.
    pub call_expr_id: ExprId,
    /// Resolved concrete callee.
    pub callable: ConcreteCallable,
    /// Optional condition `ExprId` for branch-split dispatch. When present,
    /// this callee is selected when the condition is true. `None` indicates
    /// the default (else) branch.
    pub condition: Option<ExprId>,
}

/// A resolved callable value.
#[derive(Clone, Debug, PartialEq)]
pub enum ConcreteCallable {
    /// A direct global callable reference with accumulated functor application.
    Global {
        item_id: ItemId,
        functor: FunctorApp,
    },
    /// A closure with captured variables and accumulated functor application.
    Closure {
        target: LocalItemId,
        captures: Vec<CapturedVar>,
        functor: FunctorApp,
    },
    /// Cannot be resolved statically.
    Dynamic,
}

/// A variable captured by a closure.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedVar {
    /// The captured local variable.
    pub var: LocalVarId,
    /// The type of the captured variable.
    pub ty: Ty,
    /// An optional initializer expression to reuse when the original local is
    /// scoped to a block that rewrite will erase.
    pub expr: Option<ExprId>,
}

/// Maximum number of concrete callables tracked in a `Multi` lattice element
/// before degrading to `Dynamic`.
pub(super) const MULTI_CAP: usize = 8;

/// Reaching-definitions lattice for callable variables.
/// Tracks the set of possible concrete callables at each program point.
#[derive(Clone, Debug)]
pub enum CalleeLattice {
    /// No value assigned yet (before first definition).
    Bottom,
    /// Exactly one known callable.
    Single(ConcreteCallable),
    /// Multiple known callables (from conditional branches) — up to
    /// [`MULTI_CAP`] before degrading to `Dynamic`.
    ///
    /// Each entry is `(callable, condition)` where `condition` is the
    /// `ExprId` of the if-condition that selects this callee. The last
    /// entry typically has `None` (the else branch).
    Multi(Vec<(ConcreteCallable, Option<ExprId>)>),
    /// Too many or unknown callables — cannot resolve.
    Dynamic,
}

impl CalleeLattice {
    /// Constructs a lattice element from a resolved [`ConcreteCallable`].
    #[must_use]
    pub fn from_concrete(cc: ConcreteCallable) -> Self {
        match cc {
            ConcreteCallable::Dynamic => Self::Dynamic,
            other => Self::Single(other),
        }
    }

    /// Joins two lattice elements (least upper bound).
    ///
    /// - `Bottom ⊔ x = x`
    /// - `Single(a) ⊔ Single(a) = Single(a)` (when equal)
    /// - `Single(a) ⊔ Single(b) = Multi([a, b])`
    /// - `Multi(s) ⊔ Single(a) = Multi(s ∪ {a})` (cap at [`MULTI_CAP`] → Dynamic)
    /// - `Multi(s1) ⊔ Multi(s2) = Multi(s1 ∪ s2)` (cap at [`MULTI_CAP`] → Dynamic)
    /// - `Dynamic ⊔ _ = Dynamic`
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Bottom, x) | (x, Self::Bottom) => x,
            (Self::Dynamic, _) | (_, Self::Dynamic) => Self::Dynamic,
            (Self::Single(a), Self::Single(b)) => {
                if a == b {
                    Self::Single(a)
                } else {
                    Self::Multi(vec![(a, None), (b, None)])
                }
            }
            (Self::Multi(mut s), Self::Single(a)) | (Self::Single(a), Self::Multi(mut s)) => {
                if !s.iter().any(|(cc, _)| *cc == a) {
                    s.push((a, None));
                }
                if s.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s)
                }
            }
            (Self::Multi(mut s1), Self::Multi(s2)) => {
                for (item, cond) in s2 {
                    if !s1.iter().any(|(cc, _)| *cc == item) {
                        s1.push((item, cond));
                    }
                }
                if s1.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s1)
                }
            }
        }
    }

    /// Joins two lattice elements with an associated condition from an
    /// if/else branch. `self` is the state from the **true** branch and
    /// `other` from the **false** branch.
    ///
    /// Condition-tag provenance rules:
    ///
    /// - When the true branch is a `Single(a)` distinct from the false
    ///   branch, entry `a` is tagged `Some(condition)` and the false-branch
    ///   entry keeps its existing tag (or `None` for the else case).
    /// - When the false branch contributes a new callable via
    ///   `Multi(true) ⊔ Single(false)`, that callable is appended with
    ///   `None` (it is the default/else path).
    /// - Entries inherited from an existing `Multi` retain their original
    ///   tags.
    /// - If both branches are `Multi` with identical callable sets the
    ///   original tags from `s1` are kept unchanged; otherwise the join
    ///   degrades to `Dynamic` because nested dispatch is not yet
    ///   supported.
    #[must_use]
    pub fn join_with_condition(self, other: Self, condition: ExprId) -> Self {
        match (self, other) {
            (Self::Bottom, x) | (x, Self::Bottom) => x,
            (Self::Single(a), Self::Single(b)) => {
                if a == b {
                    Self::Single(a)
                } else {
                    Self::Multi(vec![(a, Some(condition)), (b, None)])
                }
            }
            (Self::Single(a), Self::Multi(mut s)) => {
                // a from true branch (conditioned), s from false branch
                if !s.iter().any(|(cc, _)| *cc == a) {
                    s.insert(0, (a, Some(condition)));
                }
                if s.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s)
                }
            }
            // Multi(true) + Single(false): the true branch already has
            // multiple callables. Insert the single false-branch callable
            // into the set if it is not already present.
            (Self::Multi(mut s), Self::Single(b)) => {
                if !s.iter().any(|(cc, _)| *cc == b) {
                    s.push((b, None));
                }
                if s.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s)
                }
            }
            // Multi from the true branch requires nested dispatch → too
            // complex for the current implementation, UNLESS both sides have
            // the same callable set (variable was not modified in the branch).
            (Self::Multi(s1), Self::Multi(s2)) => {
                let same_callables = s1.len() == s2.len()
                    && s1
                        .iter()
                        .zip(s2.iter())
                        .all(|((cc1, _), (cc2, _))| cc1 == cc2);
                if same_callables {
                    Self::Multi(s1)
                } else {
                    Self::Dynamic
                }
            }
            // Dynamic ⊔ _ = Dynamic.
            (Self::Dynamic, _) | (_, Self::Dynamic) => Self::Dynamic,
        }
    }
}

/// Deduplication key for specializations. Two call sites that share the same
/// `SpecKey` can reuse the same generated dispatch callable.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpecKey {
    /// The HOF being specialized.
    pub hof_id: LocalItemId,
    /// Hashable representations of the concrete callable arguments.
    pub concrete_args: Vec<ConcreteCallableKey>,
}

/// Hashable variant of [`ConcreteCallable`] used for deduplication. Closures
/// are keyed only by their target and functor (captures are structural, not
/// identity-defining).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConcreteCallableKey {
    /// A direct global callable reference.
    Global {
        item_id: ItemId,
        functor: FunctorApp,
    },
    /// A closure keyed by target and functor.
    ///
    /// Captured variables are intentionally omitted so that two closures
    /// with identical targets and functors share a specialization; the
    /// captured values are threaded as ordinary arguments at the call site
    /// rather than being part of the dispatch identity.
    Closure {
        target: LocalItemId,
        functor: FunctorApp,
    },
}

/// Per-callable lattice snapshot: maps each callable's `LocalItemId` to the
/// sorted list of `(LocalVarId, CalleeLattice)` entries observed after flow
/// analysis.
pub type LatticeStates = FxHashMap<LocalItemId, Vec<(LocalVarId, CalleeLattice)>>;

/// Output of the analysis phase.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    /// Callable parameters with arrow types found in HOF declarations.
    pub callable_params: Vec<CallableParam>,
    /// Call sites where HOFs are invoked with concrete callable arguments.
    pub call_sites: Vec<CallSite>,
    /// Direct calls whose callee resolves to a concrete callable value.
    pub direct_call_sites: Vec<DirectCallSite>,
    /// Per-callable lattice states for all callable-typed local variables
    /// after flow analysis.
    pub lattice_states: LatticeStates,
}

/// Errors that can occur during defunctionalization.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// Emitted when a callable argument cannot be statically resolved to a
    /// concrete set of callables, typically because the number of conditional
    /// branches exceeds `MULTI_CAP`, a conditional has mismatched Multi
    /// variants, or a mutable callable variable is reassigned in a loop.
    #[error("callable argument could not be resolved statically")]
    #[diagnostic(code("Qsc.Defunctionalize.DynamicCallable"))]
    #[diagnostic(help("ensure all callable arguments are known at compile time"))]
    DynamicCallable(#[label] Span),

    /// Reserved; currently unused. Mutable callable parameters are handled
    /// via branch-splitting (resolving to `Multi` in the `CalleeLattice`)
    /// rather than producing this error. Retained for future use when
    /// rejection of mutable callables becomes appropriate.
    #[error("callable parameter is mutably assigned")]
    #[diagnostic(code("Qsc.Defunctionalize.MutableCallable"))]
    MutableCallable(#[label] Span),

    #[error("specialization leads to infinite recursion")]
    #[diagnostic(code("Qsc.Defunctionalize.RecursiveSpecialization"))]
    RecursiveSpecialization(#[label] Span),

    #[error(
        "defunctionalization did not converge within {0} iterations; {1} callable values remain"
    )]
    #[diagnostic(code("Qsc.Defunctionalize.FixpointNotReached"))]
    #[diagnostic(help("consider reducing the nesting depth of higher-order function chains"))]
    FixpointNotReached(usize, usize, #[label("remaining callable value")] Span),

    #[error(
        "higher-order function `{0}` generated {1} specializations, exceeding the warning threshold"
    )]
    #[diagnostic(code("Qsc.Defunctionalize.ExcessiveSpecializations"))]
    #[diagnostic(severity(warning))]
    #[diagnostic(help(
        "consider reducing the number of distinct callable arguments passed to this function"
    ))]
    ExcessiveSpecializations(
        String,
        usize,
        #[label("excessive specializations generated here")] Span,
    ),
}

/// Composes two `FunctorApp` values.
///
/// Adjoint toggles (XOR) and controlled counts stack (saturating addition).
/// This correctly handles double-adjoint cancellation:
/// `compose_functors({adj:true, ..}, {adj:true, ..})` yields `{adj:false, ..}`.
#[must_use]
pub fn compose_functors(creation: &FunctorApp, body: &FunctorApp) -> FunctorApp {
    FunctorApp {
        adjoint: creation.adjoint ^ body.adjoint,
        controlled: creation.controlled.saturating_add(body.controlled),
    }
}

/// Recursively strips `UnOp(Functor(Adj|Ctl), inner)` layers from an
/// expression, accumulating the functor applications into a `FunctorApp`.
///
/// Returns `(base_expr_id, accumulated_functor_app)` where `base_expr_id`
/// is the innermost expression after all functor wrappers are removed.
#[must_use]
pub fn peel_body_functors(package: &Package, expr_id: ExprId) -> (ExprId, FunctorApp) {
    let mut current = expr_id;
    let mut functor = FunctorApp::default();
    loop {
        let expr = package.get_expr(current);
        match &expr.kind {
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), inner) => {
                functor.adjoint = !functor.adjoint;
                current = *inner;
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner) => {
                functor.controlled = functor.controlled.saturating_add(1);
                current = *inner;
            }
            _ => return (current, functor),
        }
    }
}
