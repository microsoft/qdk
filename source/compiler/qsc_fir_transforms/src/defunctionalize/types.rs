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
    ExprId, ExprKind, Functor, ItemId, LocalItemId, LocalVarId, Package, PackageId, PackageLookup,
    PatId, StoreExprId, StoreItemId, UnOp,
};
use qsc_fir::ty::Ty;

/// A callable parameter detected in a higher-order function declaration.
#[derive(Clone, Debug)]
pub struct CallableParam {
    /// The HOF containing this parameter.
    pub callable_id: StoreItemId,
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
    /// Whether the owning HOF's input pattern is a tuple. Precomputed during
    /// analysis (which has `PackageStore` access) so later passes can derive
    /// the call-argument input path without re-reading the HOF's owning
    /// package, which may differ from the package currently being rewritten.
    pub hof_input_is_tuple: bool,
}

impl CallableParam {
    #[must_use]
    pub fn new(
        callable_id: StoreItemId,
        param_pat_id: PatId,
        top_level_param: usize,
        field_path: Vec<usize>,
        param_var: LocalVarId,
        param_ty: Ty,
        hof_input_is_tuple: bool,
    ) -> Self {
        Self {
            callable_id,
            param_pat_id,
            top_level_param,
            field_path,
            param_var,
            param_ty,
            hof_input_is_tuple,
        }
    }
}

/// A call site where a HOF is called with a concrete callable argument.
#[derive(Clone, Debug)]
pub struct CallSite {
    /// The Call expression.
    pub call_expr_id: ExprId,
    /// The package owning the body that contains this call expression. The
    /// specialized callable is allocated into this package and the call is
    /// rewritten within it, which may differ from the entry package when the
    /// call site lives in a foreign body walked by analysis.
    pub call_pkg_id: PackageId,
    /// The HOF being called.
    pub hof_item_id: ItemId,
    /// The outer input-parameter slot of the HOF this call site resolves.
    /// Copied from the originating [`CallableParam::top_level_param`] so that
    /// specialize and rewrite can recover the exact parameter for each row
    /// instead of collapsing every arrow parameter onto the lowest index. Which
    /// parameter a call site resolves is independent of the `condition`, which
    /// selects among the branch-dispatch candidates for that one parameter.
    pub top_level_param: usize,
    /// The tuple-field path relative to `top_level_param`, copied from the
    /// originating [`CallableParam::field_path`]. Empty for a separate
    /// top-level arrow parameter; non-empty for an arrow field nested inside a
    /// single tuple parameter.
    pub field_path: Vec<usize>,
    /// Whether the owning HOF's input pattern is a tuple, copied from the
    /// originating [`CallableParam::hof_input_is_tuple`]. Distinguishes a
    /// multi-parameter HOF, whose arrow input is a tuple of parameters, from a
    /// single tuple-valued parameter, whose arrow input is that tuple. This
    /// changes where a nested `field_path` indexes.
    pub hof_input_is_tuple: bool,
    /// Resolved callable argument.
    pub callable_arg: ConcreteCallable,
    /// Expression for the callable argument.
    pub arg_expr_id: ExprId,
    /// Branch-split guard list: a left-associated conjunction stored
    /// outermost-first. Selected when every guard is true; an empty list is the
    /// default (else) branch.
    pub condition: Vec<ExprId>,
}

/// A direct call whose callee expression resolves to a concrete callable value.
#[derive(Clone, Debug)]
pub struct DirectCallSite {
    /// The Call expression.
    pub call_expr_id: ExprId,
    /// The package owning the body that contains this call expression.
    pub call_pkg_id: PackageId,
    /// Resolved concrete callee.
    pub callable: ConcreteCallable,
    /// Branch-split guard list: a left-associated conjunction stored
    /// outermost-first. Selected when every guard is true; an empty list is the
    /// default (else) branch.
    pub condition: Vec<ExprId>,
    /// Optional source span of the original lambda body to stamp onto the surviving Call.
    pub def_span: Option<Span>,
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
        /// The closure body, local to the package that produced this closure
        /// value. This value target must not be threaded across packages; the
        /// dispatch *key* is package-qualified separately via `StoreItemId`
        /// (see [`ConcreteCallableKey::Closure::target`]).
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
    /// Caller-scope substitutions to apply when `expr` is a producer-scope
    /// compound literal (a struct/tuple/array constructor whose sub-exprs
    /// reference the producing function's parameters).
    ///
    /// Each `(local, caller_expr)` entry maps a producer-parameter
    /// [`LocalVarId`] appearing inside `expr` to the caller-scope argument
    /// [`ExprId`] bound to that parameter at the call site. Rewrite deep-clones
    /// `expr` and rebinds each recorded inner `Var(Res::Local(local))` leaf to
    /// `caller_expr`, so the literal is reconstructed entirely from caller-scope
    /// values instead of splicing unbound producer-scope locals into the
    /// caller. Empty for scalar captures and for captures that need no remap.
    pub caller_substitutions: Vec<(LocalVarId, ExprId)>,
}

/// Maximum number of concrete callables tracked in a `Multi` lattice element
/// before degrading to `Dynamic`.
pub(super) const MULTI_CAP: usize = 1000;

/// Reaching-definitions lattice for callable variables.
/// Tracks the set of possible concrete callables at each program point.
#[derive(Clone, Debug, PartialEq)]
pub enum CalleeLattice {
    /// No value assigned yet (before first definition).
    Bottom,
    /// Exactly one known callable.
    Single(ConcreteCallable),
    /// Multiple known callables from conditional branches — up to
    /// `MULTI_CAP` before degrading to `Dynamic`.
    ///
    /// Each entry is `(callable, guards)`, where `guards` is a left-associated
    /// conjunction stored outermost-first; the entry is selected when every
    /// guard is true. Exactly one trailing entry has an empty guard list,
    /// denoting the default (else) branch.
    Multi(Vec<(ConcreteCallable, Vec<ExprId>)>),
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
    /// - `Multi(s) ⊔ Single(a) = Multi(s ∪ {a})` (cap at `MULTI_CAP` => Dynamic)
    /// - `Multi(s1) ⊔ Multi(s2) = Multi(s1 ∪ s2)` (cap at `MULTI_CAP` => Dynamic)
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
                    Self::Multi(vec![(a, vec![]), (b, vec![])])
                }
            }
            (Self::Multi(mut s), Self::Single(a)) | (Self::Single(a), Self::Multi(mut s)) => {
                if !s.iter().any(|(cc, _)| *cc == a) {
                    s.push((a, vec![]));
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

    /// Joins two lattice elements with the `condition` of an if/else branch:
    /// `self` is the **true** branch, `other` the **false** branch. Guards are
    /// stored outermost-first; sequential ordering of entries supplies the
    /// implicit `!condition` for later arms, so only true-branch entries gain
    /// `condition`.
    ///
    /// - `Single(a)` vs distinct `Single(b)`: `[(a,[condition]), (b,[])]`.
    /// - `Single(a)` vs `Multi(s)`: prepend `(a,[condition])` unconditionally;
    ///   `s` unchanged. The true-branch arm is kept even when `a` already
    ///   appears inside `s` under a different guard — collapsing it would drop
    ///   the `condition` arm.
    /// - `Multi(s)` vs `Single(b)`: prepend `condition` onto every entry of
    ///   `s`, then append `(b,[])` as the trailing default.
    /// - `Multi(s1)` vs `Multi(s2)`: if the callable sets are identical (the
    ///   variable was not modified in the branch), keep `s1` unchanged.
    ///   Otherwise merge: prepend `condition` onto every `s1` guard list, keep
    ///   `s2` guards as-is, and concatenate `s1`-then-`s2` **without**
    ///   deduplicating by callable identity — the same callable under
    ///   `condition` (s1) and `!condition` (s2) is two distinct dispatch arms.
    ///   Only the `s2` empty-guard default survives as the trailing fall-through.
    ///
    /// Overflow past `MULTI_CAP` degrades to `Dynamic`.
    #[must_use]
    pub fn join_with_condition(self, other: Self, condition: ExprId) -> Self {
        match (self, other) {
            (Self::Bottom, x) | (x, Self::Bottom) => x,
            (Self::Single(a), Self::Single(b)) => {
                if a == b {
                    Self::Single(a)
                } else {
                    Self::Multi(vec![(a, vec![condition]), (b, vec![])])
                }
            }
            (Self::Single(a), Self::Multi(mut s)) => {
                // Prepend the conditioned true-branch entry; `s` supplies the
                // implicit `!condition` via sequential ordering. Prepended
                // unconditionally: even when `a` already appears inside `s`
                // under a different guard, the true-branch arm is a distinct
                // dispatch case — deduplicating it against the inner occurrence
                // would drop the `condition` arm and reroute that path through
                // `s`'s guards instead of unconditionally selecting `a`.
                s.insert(0, (a, vec![condition]));
                if s.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s)
                }
            }
            // Multi(true) + Single(false): prepend `condition` onto every
            // inherited entry, then append the false-branch callable as the
            // trailing default.
            (Self::Multi(mut s), Self::Single(b)) => {
                for (_, guards) in &mut s {
                    guards.insert(0, condition);
                }
                // Appended unconditionally: the else branch is a distinct
                // fall-through arm even when `b` duplicates an inner callable.
                s.push((b, vec![]));
                if s.len() > MULTI_CAP {
                    Self::Dynamic
                } else {
                    Self::Multi(s)
                }
            }
            // Multi from both branches (nested dispatch on each side). Identical
            // dispatch chains — same callables *and* same guards — mean the
            // variable was not modified in the branch, so keep `s1` to stay
            // byte-stable; otherwise merge the two chains. Comparing callable
            // identity alone is unsound: two branches can reassign the local to
            // the same set of callables under *different* inner guards (e.g.
            // `if rb {X} else {Z}` vs `if rc {X} else {Z}`), and collapsing to
            // `s1` would drop the outer condition and reroute the false-branch
            // path through the true branch's guards.
            (Self::Multi(s1), Self::Multi(s2)) => {
                if s1 == s2 {
                    Self::Multi(s1)
                } else {
                    // Prepend `condition` onto every `s1` guard list; keep `s2`
                    // guards as-is. Concatenated without dedup by callable: the
                    // same callable under `condition` (s1) and `!condition` (s2)
                    // names two distinct arms, and dropping the `s2` arm would
                    // reroute its path to the trailing default instead.
                    let mut merged: Vec<(ConcreteCallable, Vec<ExprId>)> =
                        Vec::with_capacity(s1.len() + s2.len());
                    for (cc, mut guards) in s1 {
                        guards.insert(0, condition);
                        merged.push((cc, guards));
                    }
                    // Keep exactly one trailing default: after the prepend, any
                    // `s1` default is now guarded by `condition`, so the `s2`
                    // default is the unconditional fall-through. Hold it back so
                    // it terminates the chain.
                    let mut trailing_default: Option<(ConcreteCallable, Vec<ExprId>)> = None;
                    for (cc, guards) in s2 {
                        if guards.is_empty() {
                            trailing_default = Some((cc, guards));
                        } else {
                            merged.push((cc, guards));
                        }
                    }
                    if let Some(default_entry) = trailing_default {
                        merged.push(default_entry);
                    }
                    if merged.len() > MULTI_CAP {
                        Self::Dynamic
                    } else {
                        Self::Multi(merged)
                    }
                }
            }
            (Self::Dynamic, _) | (_, Self::Dynamic) => Self::Dynamic,
        }
    }
}

/// Deduplication key for specializations. Two call sites that share the same
/// `SpecKey` can reuse the same generated dispatch callable.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpecKey {
    /// The HOF being specialized.
    pub hof_id: StoreItemId,
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
    ///
    /// The target is package-qualified (`StoreItemId`) so that closures with
    /// the same package-local id in different packages do not collide.
    Closure {
        target: StoreItemId,
        functor: FunctorApp,
        occurrence: Option<usize>,
    },
}

/// Per-callable lattice snapshot: maps each callable's `LocalItemId` to the
/// sorted list of `(LocalVarId, CalleeLattice)` entries observed after flow
/// analysis.
pub type LatticeStates = FxHashMap<LocalItemId, Vec<(LocalVarId, CalleeLattice)>>;

/// A defunctionalization diagnostic paired with the FIR package that owns its
/// source label.
#[derive(Clone, Debug)]
pub(crate) struct OwnedError {
    pub package: PackageId,
    pub error: Error,
}

/// Output of the analysis phase.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    /// Callable parameters with arrow types found in HOF declarations.
    pub callable_params: Vec<CallableParam>,
    /// Call sites where HOFs are invoked with concrete callable arguments.
    pub call_sites: Vec<CallSite>,
    /// Direct calls whose callee resolves to a concrete callable value.
    pub direct_call_sites: Vec<DirectCallSite>,
    /// Direct calls whose `Var(Res::Local)` callee resolved to `Dynamic`
    /// (over-defined), recorded so the driver can emit a `DynamicCallable`
    /// diagnostic with the call-site span. `Bottom` callees are excluded to
    /// avoid spurious errors on intermediate fixpoint iterations.
    pub unresolved_direct_call_sites: Vec<StoreExprId>,
    /// Per-callable lattice states for all callable-typed local variables
    /// after flow analysis.
    pub lattice_states: LatticeStates,
}

/// Errors that can occur during defunctionalization.
///
/// # Severity
///
/// All variants are fatal to the FIR transform pipeline except
/// [`Error::ExcessiveSpecializations`], which is emitted as a warning. Use
/// [`Error::is_warning`] to partition diagnostics by severity.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// Emitted when a callable argument cannot be statically resolved to a
    /// concrete set of callables, typically because the number of conditional
    /// branches exceeds `MULTI_CAP`, a conditional has mismatched Multi
    /// variants, or a mutable callable variable is reassigned in a loop.
    ///
    /// This diagnostic is also emitted when a captured compound literal — a
    /// struct, tuple, or array — cannot be safely rebuilt in the caller's
    /// scope. For example, a captured struct field whose value comes from an
    /// operation call cannot be duplicated or reordered out of the scope that
    /// produced it. Declining such a closure to a dynamic call site keeps the
    /// original dispatch and produces this recoverable diagnostic instead of
    /// generating incorrect code, which would be a hard error on the base
    /// profile.
    #[error("callable argument could not be resolved statically")]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.DynamicCallable"))]
    #[diagnostic(help("ensure all callable arguments are known at compile time"))]
    DynamicCallable(#[label] Span),

    /// Emitted when a higher-order function forwards two or more distinct
    /// arrays of callables through a single call. The callables are statically
    /// resolved, but the combined removal models only one forwarded callable
    /// array per call; the multiple-array shape would otherwise fall through to
    /// the per-row path and silently collapse each multi-candidate array to a
    /// single member. Failing closed here keeps the transform from emitting
    /// incorrect output for a shape it does not yet support.
    ///
    /// Forwarding two or more callable arrays through one call is always
    /// declined with this diagnostic rather than partially specialized, so this
    /// unsupported shape can never turn into incorrect code.
    #[error("higher-order function forwards more than one callable array, which is not supported")]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.UnsupportedMultipleCallableArrays"))]
    #[diagnostic(help(
        "pass at most one array-of-callables argument to a higher-order function; combine the \
         arrays or specialize the callers so each forwards a single callable array"
    ))]
    UnsupportedMultipleCallableArrays(#[label] Span),

    /// Emitted when the analysis => specialize => rewrite fixpoint loop exits
    /// without eliminating every reachable closure or arrow-typed parameter.
    /// The first field is the iteration count actually reached and the
    /// second is the number of remaining callable values. Suppressed when
    /// any other diagnostic has already fired this pass so the root cause is
    /// surfaced instead of a generic non-convergence report.
    #[error(
        "defunctionalization did not converge within {0} iterations; {1} callable values remain"
    )]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.FixpointNotReached"))]
    #[diagnostic(help("consider reducing the nesting depth of higher-order function chains"))]
    FixpointNotReached(usize, usize, #[label("remaining callable value")] Span),

    /// Warning emitted when a single HOF generates more than the warning
    /// threshold of distinct specializations during a pass. The string is
    /// the HOF name and the second field is the specialization count. This
    /// is the only warning-severity variant; see [`Error::is_warning`].
    #[error(
        "higher-order function `{0}` generated {1} specializations, exceeding the warning threshold"
    )]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.ExcessiveSpecializations"))]
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

impl Error {
    /// Returns `true` when the diagnostic is non-fatal to the FIR transform
    /// pipeline.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::ExcessiveSpecializations(..))
    }
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
