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
    ExprId, ExprKind, Functor, ItemId, ItemKind, LocalItemId, LocalVarId, Package, PackageId,
    PackageLookup, PackageSpan, PackageStore, PatId, Res, StoreExprId, StoreItemId, UnOp,
};
use qsc_fir::ty::Ty;

/// Immutable structural UDT definitions shared by analysis and mutation phases.
/// Types retain their owning package identity even when a callable is cloned
/// into another package. Unresolved definitions remain opaque, not arrow-free.
#[derive(Clone, Debug, Default)]
pub(crate) struct UdtMetadata {
    pure_tys: FxHashMap<StoreItemId, Ty>,
}

impl UdtMetadata {
    pub(super) fn new(store: &PackageStore) -> Self {
        let mut pure_tys = FxHashMap::default();
        for (package_id, package) in store {
            for (item_id, item) in &package.items {
                if let ItemKind::Ty(_, udt) = &item.kind {
                    pure_tys.insert((package_id, item_id).into(), udt.get_pure_ty());
                }
            }
        }
        Self { pure_tys }
    }

    pub(super) fn pure_ty(&self, item: ItemId) -> Option<&Ty> {
        self.pure_tys.get(&(item.package, item.item).into())
    }

    /// Opens only the outer UDT wrappers, preserving nominal child types.
    pub(super) fn underlying_ty<'a>(&'a self, mut ty: &'a Ty) -> &'a Ty {
        while let Ty::Udt(Res::Item(item)) = ty {
            let Some(pure) = self.pure_ty(*item) else {
                break;
            };
            ty = pure;
        }
        ty
    }

    /// Expands structural views without changing any FIR type or item identity.
    /// Frontend rejection of cyclic UDTs makes recursive expansion finite.
    pub(super) fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Udt(Res::Item(item)) => self
                .pure_ty(*item)
                .map_or_else(|| ty.clone(), |pure| self.resolve(pure)),
            Ty::Tuple(items) => Ty::Tuple(items.iter().map(|item| self.resolve(item)).collect()),
            Ty::Array(item) => Ty::Array(Box::new(self.resolve(item))),
            Ty::Arrow(arrow) => {
                let mut arrow = arrow.clone();
                arrow.input = Box::new(self.resolve(&arrow.input));
                arrow.output = Box::new(self.resolve(&arrow.output));
                Ty::Arrow(arrow)
            }
            _ => ty.clone(),
        }
    }

    pub(super) fn contains_arrow(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Udt(Res::Item(item)) => self
                .pure_ty(*item)
                .is_none_or(|pure| self.contains_arrow(pure)),
            Ty::Arrow(_) | Ty::Udt(_) => true,
            Ty::Array(item) => self.contains_arrow(item),
            Ty::Tuple(items) => items.iter().any(|item| self.contains_arrow(item)),
            Ty::Infer(_) | Ty::Param(_) | Ty::Prim(_) | Ty::Err => false,
        }
    }
}

/// A batch of removals in the original structural input coordinates.
/// Only removing an immediate child collapses its parent; an original Unit
/// child is retained even when another child is completely consumed.
#[derive(Clone, Debug)]
pub(super) enum InputRemoval {
    Keep,
    Remove,
    Tuple { children: Vec<Self>, collapse: bool },
}

impl InputRemoval {
    pub(super) fn new(ty: &Ty, paths: &[&[usize]]) -> Option<Self> {
        if paths.iter().any(|path| path.is_empty()) {
            return Some(Self::Remove);
        }
        if paths.is_empty() {
            return Some(Self::Keep);
        }
        let Ty::Tuple(fields) = ty else {
            return None;
        };
        if paths.iter().any(|path| path[0] >= fields.len()) {
            return None;
        }
        let children: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let paths: Vec<_> = paths
                    .iter()
                    .filter_map(|path| (path[0] == index).then_some(&path[1..]))
                    .collect();
                Self::new(field, &paths)
            })
            .collect::<Option<_>>()?;
        let removed = children.iter().filter(|child| child.is_removed()).count();
        Some(Self::Tuple {
            children,
            collapse: removed > 0 && fields.len() - removed == 1,
        })
    }

    pub(super) fn is_removed(&self) -> bool {
        matches!(self, Self::Remove)
    }

    pub(super) fn consumes_original(&self) -> bool {
        match self {
            Self::Keep => false,
            Self::Remove => true,
            Self::Tuple { children, .. } => {
                !children.is_empty() && children.iter().all(Self::consumes_original)
            }
        }
    }

    /// Applies structural removals without erasing untouched nominal children.
    pub(super) fn reduced_ty(&self, ty: &Ty, udts: &UdtMetadata) -> Ty {
        match self {
            Self::Keep => ty.clone(),
            Self::Remove => Ty::UNIT,
            Self::Tuple { children, collapse } => {
                let Ty::Tuple(fields) = udts.underlying_ty(ty) else {
                    unreachable!("removal layout was built from this tuple type");
                };
                let mut remaining: Vec<_> = children
                    .iter()
                    .zip(fields)
                    .filter(|(child, _)| !child.is_removed())
                    .map(|(child, field)| child.reduced_ty(field, udts))
                    .collect();
                if *collapse {
                    remaining.pop().expect("one surviving child")
                } else {
                    Ty::Tuple(remaining)
                }
            }
        }
    }

    pub(super) fn rebase(&self, path: &[usize]) -> Option<Vec<usize>> {
        match self {
            Self::Keep => Some(path.to_vec()),
            Self::Remove => None,
            Self::Tuple { children, collapse } => {
                let Some((&index, tail)) = path.split_first() else {
                    return Some(Vec::new());
                };
                let suffix = children.get(index)?.rebase(tail)?;
                let mut rebased = Vec::new();
                if !collapse {
                    rebased.push(
                        children[..index]
                            .iter()
                            .filter(|child| !child.is_removed())
                            .count(),
                    );
                }
                rebased.extend(suffix);
                Some(rebased)
            }
        }
    }
}

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
    /// The callable-bearing parameter type: an arrow or an array of arrows.
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

/// One resolved candidate, or a dynamic placeholder, for a HOF argument slot.
#[derive(Clone, Debug)]
pub struct CallSite {
    /// The Call expression.
    pub call_expr_id: ExprId,
    /// The package owning this call expression and its argument operands.
    /// Rewriting happens in this package, which may differ from the entry
    /// package. A deduplicated specialization can live in another package;
    /// its `StoreItemId` independently identifies that target's owner.
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
    /// Materialized operands captured by a same-package lifted lambda.
    ///
    /// These values belong to this call occurrence rather than the global
    /// callable target: separate factory invocations share the lifted item but
    /// can retain different partial-application operands.
    pub captures: Vec<CapturedVar>,
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

/// The local-variable identity domain that owns a capture operand.
///
/// Item IDs are local to the package being analyzed or rewritten. The enclosing
/// call site's `call_pkg_id` supplies that package context; these scopes must
/// not be compared or transported across packages without rebinding.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum CaptureScope {
    /// Locals of a callable not tracked as a HOF specialization, including
    /// lifted lambda parameters.
    Callable(LocalItemId),
    /// Locals of the package's entry expression, outside any callable item.
    #[default]
    Entry,
    /// Locals allocated for a specialized HOF clone. This domain is distinct
    /// from `Callable` even when the numeric item and local IDs coincide.
    CloneScope(LocalItemId),
}

/// A local variable qualified by its owning identity domain within one package.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScopedLocal {
    pub var: LocalVarId,
    pub scope: CaptureScope,
}

impl ScopedLocal {
    #[must_use]
    pub fn new(var: LocalVarId, scope: CaptureScope) -> Self {
        Self { var, scope }
    }
}

/// A variable captured by a closure.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedVar {
    /// The captured local variable and the allocator domain that owns it.
    pub local: ScopedLocal,
    /// The type of the captured variable.
    pub ty: Ty,
    /// Capture-free callable identity, retained even when operand evaluation
    /// materializes the value into a temporary.
    pub static_callable: Option<(ItemId, FunctorApp)>,
    /// An optional initializer expression to reuse when the original local is
    /// scoped to a block that rewrite will erase.
    pub expr: Option<ExprId>,
    /// Substitutions for producer-local references in `expr`. Nested
    /// substitutions preserve each intervening caller's environment instead of
    /// interpreting equal numeric local IDs as bindings in the same scope.
    pub caller_substitutions: Vec<CaptureSubstitution>,
}

/// Replaces one local in a capture expression with an operand from its caller.
///
/// The replacement expression has its own substitution environment. Keeping
/// these environments separate preserves transformations such as `Make(n+1)`
/// when the returned closure crosses another function boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureSubstitution {
    /// The local referenced by the containing expression.
    pub local: LocalVarId,
    /// Its replacement expression, in that expression's own local scope.
    pub expr: ExprId,
    /// Rebindings for locals within the replacement, not the containing expression.
    pub substitutions: Vec<CaptureSubstitution>,
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
    /// Multiple known candidates from conditional branches or callable arrays,
    /// bounded by `MULTI_CAP` in the analysis joins and candidate collection.
    ///
    /// Each entry is `(callable, guards)`, where `guards` is a left-associated
    /// conjunction stored outermost-first; the entry is selected when every
    /// guard is true. Conditional joins place the default arm last. Array
    /// candidates and unconditional joins can instead have several empty guard
    /// lists; rewrite must then recover an index discriminator.
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
    /// - `Multi(s1)` vs `Multi(s2)`: if the full ordered chains, including
    ///   guards, are identical, keep `s1` unchanged.
    ///   Otherwise merge: prepend `condition` onto every `s1` guard list, keep
    ///   `s2` guards as-is, and concatenate `s1`-then-`s2` **without**
    ///   deduplicating by callable identity — the same callable under
    ///   `condition` (s1) and `!condition` (s2) is two distinct dispatch arms.
    ///   Preserve every empty-guard entry too: indexed alternatives are physical
    ///   positions, not competing defaults. Dropping one could make an ambiguous
    ///   indexed selection look like a complete conditional decision tree.
    ///
    /// A joined `Multi` is not necessarily a complete dispatch tree. Rewrite
    /// must still prove its guard tree or recover an index discriminator.
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
            // Identical ordered candidates and guards can stay byte-stable.
            // Callable identity alone cannot establish equivalent selections.
            (Self::Multi(mut s1), Self::Multi(s2)) => {
                if s1 == s2 {
                    Self::Multi(s1)
                } else {
                    for (_, guards) in &mut s1 {
                        guards.insert(0, condition);
                    }
                    s1.extend(s2);
                    if s1.len() > MULTI_CAP {
                        Self::Dynamic
                    } else {
                        Self::Multi(s1)
                    }
                }
            }
            (Self::Dynamic, _) | (_, Self::Dynamic) => Self::Dynamic,
        }
    }
}

/// Deduplication key for specializations. Two call sites that share the same
/// `SpecKey` remove the same callable positions and can reuse the same generated
/// dispatch callable.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpecKey {
    /// The HOF being specialized.
    pub hof_id: StoreItemId,
    /// Removed `(top_level_param, field_path)` positions, aligned with
    /// `concrete_args`. Repeated positions preserve callable-array element order.
    pub param_positions: Vec<(usize, Vec<usize>)>,
    /// Hashable representations of the concrete callable arguments.
    pub concrete_args: Vec<ConcreteCallableKey>,
}

/// Hashable callable identity, including callable captures embedded into code.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConcreteCallableKey {
    /// A direct global callable reference.
    Global {
        item_id: ItemId,
        functor: FunctorApp,
    },
    /// A closure keyed by target, functor, occurrence and embedded callable identity.
    ///
    /// Runtime captures are omitted; an embedded capture changes generated code
    /// and therefore participates in identity. Only capture-free identities are
    /// embedded, so recursive environments cannot expand the key indefinitely.
    ///
    /// The target is package-qualified (`StoreItemId`) so that closures with
    /// the same package-local id in different packages do not collide.
    Closure {
        target: StoreItemId,
        functor: FunctorApp,
        occurrence: Option<usize>,
        embedded: Option<(ItemId, FunctorApp)>,
    },
}

/// Per-callable lattice snapshot: maps each callable's `LocalItemId` to the
/// sorted list of `(LocalVarId, CalleeLattice)` entries observed after flow
/// analysis.
pub type LatticeStates = FxHashMap<LocalItemId, Vec<(LocalVarId, CalleeLattice)>>;

/// Output of the analysis phase.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    /// Package-qualified UDT layouts captured before specialization mutates FIR.
    pub(super) udt_metadata: UdtMetadata,
    /// Callable parameters with arrow types found in HOF declarations.
    pub callable_params: Vec<CallableParam>,
    /// HOF argument candidates, including dynamic placeholders for diagnostics.
    pub call_sites: Vec<CallSite>,
    /// Direct calls whose callee resolves to a concrete callable value.
    pub direct_call_sites: Vec<DirectCallSite>,
    /// Direct calls with unresolved callees or inadmissible capture operands,
    /// recorded for call-site `DynamicCallable` diagnostics. Calls through the
    /// owning HOF's unresolved parameter and `Bottom` callees are excluded to
    /// avoid diagnosing transient forwarding states.
    pub unresolved_direct_call_sites: Vec<StoreExprId>,
    /// Non-bottom lattice snapshots for entry-package callable locals, used
    /// for diagnostics and tests.
    pub lattice_states: LatticeStates,
}

/// Errors that can occur during defunctionalization.
///
/// # Severity
///
/// [`Error::DynamicCallable`] and [`Error::FixpointNotReached`] are deferred
/// to downstream analysis. [`Error::ExcessiveSpecializations`] is a warning;
/// the remaining variants are fatal to the FIR transform pipeline.
/// [`Error::RecursiveSpecialization`] is the fatal counterpart of that warning:
/// it fires when a single HOF's cumulative specialization count across all
/// fixpoint iterations exceeds a hard cap, backstopping any runaway-growth
/// shape the analysis does not otherwise fold into a bounded set. Use
/// [`Error::is_warning`] to partition diagnostics by severity.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    /// Emitted when a callable argument cannot be statically resolved to a
    /// concrete set of callables, for example when the candidate count exceeds
    /// `MULTI_CAP`, a capture cannot be reconstructed in scope, or a mutable
    /// callable variable is reassigned in a loop.
    ///
    /// This diagnostic is also emitted when a captured compound literal — a
    /// constructor or copy-update — cannot be rebuilt in the caller's
    /// scope. For example, a captured struct field whose value comes from an
    /// operation call cannot be duplicated or reordered out of the scope that
    /// produced it. Declining such a closure to a dynamic call site keeps the
    /// original dispatch for downstream analysis instead of inventing capture
    /// operands. The pipeline defers this diagnostic on every target profile;
    /// capability analysis and partial evaluation decide whether the residual
    /// program can generate QIR.
    #[error("callable argument could not be resolved statically")]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.DynamicCallable"))]
    #[diagnostic(help("ensure all callable arguments are known at compile time"))]
    DynamicCallable(#[label] PackageSpan),

    /// Emitted when a higher-order function forwards two or more distinct
    /// arrays of callables through a single call. The callables are statically
    /// resolved, but the combined removal models only one forwarded callable
    /// array per call; the multiple-array shape would otherwise fall through to
    /// the per-row path and silently collapse each multi-candidate array to a
    /// single member. Failing closed here keeps the transform from emitting
    /// incorrect output for a shape it does not yet support.
    ///
    /// The guard recognizes repeated, statically resolved callable-array
    /// positions. It is not a blanket ban on every signature containing two
    /// arrays; unresolved or singleton candidate sets can take other paths.
    #[error("higher-order function forwards more than one callable array, which is not supported")]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.UnsupportedMultipleCallableArrays"))]
    #[diagnostic(help(
        "pass at most one array-of-callables argument to a higher-order function; combine the \
         arrays or specialize the callers so each forwards a single callable array"
    ))]
    UnsupportedMultipleCallableArrays(#[label] PackageSpan),

    /// Emitted when the analysis => specialize => rewrite fixpoint loop exits
    /// without eliminating every reachable closure or arrow-typed parameter.
    /// The first field is the iteration count actually reached and the
    /// second is the number of remaining callable values. Suppressed when
    /// another error has already been recorded, or when unresolved direct
    /// calls can instead receive `DynamicCallable` diagnostics. Warnings do not
    /// suppress it.
    #[error(
        "defunctionalization did not converge within {0} iterations; {1} callable values remain"
    )]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.FixpointNotReached"))]
    #[diagnostic(help("consider reducing the nesting depth of higher-order function chains"))]
    FixpointNotReached(
        usize,
        usize,
        #[label("remaining callable value")] PackageSpan,
    ),

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
        #[label("excessive specializations generated here")] PackageSpan,
    ),

    /// Fatal error emitted when a single HOF's cumulative specialization count,
    /// deduplicated across every fixpoint iteration, exceeds the hard cap. The string
    /// is the HOF name and the second field is the cumulative distinct
    /// specialization count. This is the fatal backstop for the
    /// [`Error::ExcessiveSpecializations`] warning: the warning flags a HOF that
    /// is fanning out in a single pass, while this error fires when growth
    /// persists across iterations far enough to indicate a degenerate recursive
    /// shape that would otherwise consume unbounded resources. Failing closed
    /// here keeps the transform from looping or exhausting memory on such input.
    #[error(
        "higher-order function `{0}` generated {1} cumulative specializations, exceeding the recursion cap"
    )]
    #[diagnostic(code("Qdk.Qsc.Defunctionalize.RecursiveSpecialization"))]
    #[diagnostic(help(
        "reduce the nesting depth or recursion of higher-order function calls so a single function \
         does not require an unbounded number of specializations"
    ))]
    RecursiveSpecialization(
        String,
        usize,
        #[label("recursive specialization budget exceeded here")] PackageSpan,
    ),
}

impl Error {
    /// Returns the package that owns this diagnostic.
    #[must_use]
    pub fn owner(&self) -> PackageId {
        self.package_span().package
    }

    /// Returns the package-qualified source label for this diagnostic.
    #[must_use]
    pub fn package_span(&self) -> PackageSpan {
        match self {
            Self::DynamicCallable(span)
            | Self::UnsupportedMultipleCallableArrays(span)
            | Self::RecursiveSpecialization(_, _, span)
            | Self::FixpointNotReached(_, _, span)
            | Self::ExcessiveSpecializations(_, _, span) => *span,
        }
    }

    /// Returns `true` for diagnostics surfaced on the pipeline's warning
    /// channel. Deferred convergence diagnostics are classified separately.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::ExcessiveSpecializations(..))
    }

    /// Returns `true` when the diagnostic reports a defunctionalization
    /// convergence failure that may be safely deferred to downstream analysis.
    ///
    /// `FixpointNotReached` and `DynamicCallable` report dispatch that this
    /// pass cannot resolve. The pipeline retains its structural checks for the
    /// residual FIR; resource counting and partial evaluation then resolve or
    /// reject its callable behavior. These diagnostics are suppressed rather
    /// than surfaced on the warning channel.
    ///
    /// The remaining resource and unsupported-shape backstops are not
    /// deferrable: they signal a shape the transform cannot lower and must stay
    /// on their existing fatal or warning paths.
    #[must_use]
    pub fn is_deferrable(&self) -> bool {
        matches!(
            self,
            Self::FixpointNotReached(..) | Self::DynamicCallable(..)
        )
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
