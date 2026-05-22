// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Pre-pass rewrites before collecting call sites for defunctionalization.
//! These rewrites are not strictly necessary for correctness, but they
//! simplify the analysis by eliminating certain patterns of indirection and
//! exposing more direct call sites. They are run before collecting call sites
//! and performing the lattice analysis.
//!
//! # Responsibilities
//!
//! - Run the single-use local promotion that replaces single-use immutable
//!   callable locals with direct references to their initializer (via
//!   [`promote_single_use_callable_locals`]).
//! - Run the adjacent aggregate-alias promotion that replaces
//!   `let pair = aggregate; let (...) = pair;` with direct aggregate
//!   destructuring when `pair` has a callable-typed field and no other uses.
//! - Run the identity-closure peephole that replaces `(args) => f(args)`
//!   closures with direct references to `f` (via
//!   [`identity_closure_peephole`]).
//!

use qsc_fir::fir::{
    Block, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, LocalVarId, Mutability,
    Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind, Res, Stmt, StmtId,
    StmtKind, UnOp,
};
use qsc_fir::ty::Ty;
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

/// Runs pre-pass rewrites before collecting call sites for defunctionalization. See
/// [`promote_single_use_callable_locals`] and [`identity_closure_peephole`] for details.
///
/// Only expressions in `reachable_expr_ids` are scanned for promotion candidates
/// and identity-closure patterns, restricting analysis to entry-reachable code.
pub(super) fn run(store: &mut PackageStore, package_id: PackageId, reachable_expr_ids: &[ExprId]) {
    // Before collecting call sites, runs pre-pass rewrites:
    // 1. Promotes single-use immutable callable locals to direct item references.
    // 2. Promotes single-use aggregate callable aliases into tuple destructuring.
    // 3. Replaces identity closures `(args) => f(args)` with direct references to `f`.
    promote_single_use_callable_locals(store, package_id, reachable_expr_ids);
    promote_adjacent_aggregate_callable_aliases(store, package_id);
    identity_closure_peephole(store, package_id, reachable_expr_ids);
}

/// Promotes an adjacent, single-use aggregate local into a following tuple
/// destructure. This preserves evaluation order because there is no intervening
/// statement between the alias binding and its only use.
fn promote_adjacent_aggregate_callable_aliases(store: &mut PackageStore, package_id: PackageId) {
    let block_ids: Vec<_> = {
        let pkg = store.get(package_id);
        collect_promotion_scopes(pkg)
            .into_iter()
            .flat_map(|scope| scope.seen_blocks.into_iter())
            .collect()
    };

    let pkg = store.get_mut(package_id);
    for block_id in block_ids {
        promote_adjacent_aggregate_callable_aliases_in_block(pkg, block_id);
    }
}

fn promote_adjacent_aggregate_callable_aliases_in_block(pkg: &mut Package, block_id: BlockId) {
    loop {
        let stmt_ids = pkg.get_block(block_id).stmts.clone();
        let mut retained = Vec::with_capacity(stmt_ids.len());
        let mut changed = false;
        let mut index = 0;

        while index < stmt_ids.len() {
            if index + 1 < stmt_ids.len()
                && let Some(init_expr_id) = aggregate_alias_promotion_init(
                    pkg,
                    block_id,
                    stmt_ids[index],
                    stmt_ids[index + 1],
                )
            {
                if let StmtKind::Local(_, _, expr_id) = &mut pkg
                    .stmts
                    .get_mut(stmt_ids[index + 1])
                    .expect("statement should exist")
                    .kind
                {
                    *expr_id = init_expr_id;
                }
                retained.push(stmt_ids[index + 1]);
                changed = true;
                index += 2;
                continue;
            }

            retained.push(stmt_ids[index]);
            index += 1;
        }

        pkg.blocks
            .get_mut(block_id)
            .expect("block should exist")
            .stmts = retained;

        if !changed {
            break;
        }
    }
}

fn aggregate_alias_promotion_init(
    pkg: &Package,
    block_id: BlockId,
    alias_stmt_id: StmtId,
    use_stmt_id: StmtId,
) -> Option<ExprId> {
    let alias_stmt = pkg.get_stmt(alias_stmt_id);
    let StmtKind::Local(Mutability::Immutable, alias_pat_id, alias_init_expr_id) = alias_stmt.kind
    else {
        return None;
    };
    let alias_pat = pkg.get_pat(alias_pat_id);
    let PatKind::Bind(alias_ident) = &alias_pat.kind else {
        return None;
    };
    if !ty_contains_arrow(&alias_pat.ty) {
        return None;
    }

    let use_stmt = pkg.get_stmt(use_stmt_id);
    let StmtKind::Local(_, use_pat_id, use_expr_id) = use_stmt.kind else {
        return None;
    };
    if !matches!(pkg.get_pat(use_pat_id).kind, PatKind::Tuple(_)) {
        return None;
    }
    if !matches!(pkg.get_expr(use_expr_id).kind, ExprKind::Var(Res::Local(var), _) if var == alias_ident.id)
    {
        return None;
    }

    if local_has_exactly_one_use_in_block(pkg, block_id, alias_ident.id, use_expr_id) {
        Some(alias_init_expr_id)
    } else {
        None
    }
}

fn local_has_exactly_one_use_in_block(
    pkg: &Package,
    block_id: BlockId,
    local_id: LocalVarId,
    expected_use_expr_id: ExprId,
) -> bool {
    let mut use_count = 0;
    let mut saw_expected_use = false;
    crate::walk_utils::for_each_expr_in_block(
        pkg,
        block_id,
        &mut |expr_id, expr| match &expr.kind {
            ExprKind::Var(Res::Local(var), _) if *var == local_id => {
                use_count += 1;
                saw_expected_use |= expr_id == expected_use_expr_id;
            }
            ExprKind::Closure(captures, _) if captures.contains(&local_id) => {
                use_count += 1;
            }
            _ => {}
        },
    );

    use_count == 1 && saw_expected_use
}

fn ty_contains_arrow(ty: &Ty) -> bool {
    match ty {
        Ty::Arrow(_) => true,
        Ty::Tuple(items) => items.iter().any(ty_contains_arrow),
        _ => false,
    }
}

/// Promotes single-use immutable callable locals whose initializer is a simple
/// item reference. For example, `let op = H; Apply(op, q)` is rewritten to
/// `Apply(H, q)`, eliminating the indirection before analysis runs.
///
/// # Before
/// ```text
/// let op = H;         // Local(pat, Var(Item(H)))
/// Apply(op, qubit);   // Call(Apply, (Var(Local(op)), qubit))
/// ```
/// # After
/// ```text
/// let op = H;         // binding still present (DCE removes later)
/// Apply(H, qubit);    // Call(Apply, (Var(Item(H)), qubit))
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` at each single-use site from `Var(Local(..))`
///   to `Var(Item(..))` in place.
fn promote_single_use_callable_locals(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) {
    let replacements = {
        let pkg = store.get(package_id);
        collect_single_use_promotions(pkg, reachable_expr_ids)
    };

    if !replacements.is_empty() {
        let pkg = store.get_mut(package_id);
        for (expr_id, new_kind) in replacements {
            pkg.exprs
                .get_mut(expr_id)
                .expect("expression should exist")
                .kind = new_kind;
        }
    }
}

/// Scans immutable local bindings whose initialiser is a simple item reference
/// (`Var(Res::Item(_))`), counts uses within reachable expressions in the same
/// owner scope, and collects replacements for locals that are used exactly once.
fn collect_single_use_promotions(
    pkg: &Package,
    reachable_expr_ids: &[ExprId],
) -> Vec<(ExprId, ExprKind)> {
    let reachable_expr_ids: FxHashSet<_> = reachable_expr_ids.iter().copied().collect();
    collect_promotion_scopes(pkg)
        .iter()
        .flat_map(|scope| collect_single_use_promotions_in_scope(pkg, scope, &reachable_expr_ids))
        .collect()
}

/// Collects single-use callable-local replacements within one owner scope.
fn collect_single_use_promotions_in_scope(
    pkg: &Package,
    scope: &PromotionScope<'_>,
    reachable_expr_ids: &FxHashSet<ExprId>,
) -> Vec<(ExprId, ExprKind)> {
    // find candidate immutable locals whose init is a simple item reference.
    let mut candidates: FxHashMap<LocalVarId, ExprKind> = FxHashMap::default();
    for &stmt_id in &scope.stmts {
        let stmt = pkg.get_stmt(stmt_id);
        if let StmtKind::Local(Mutability::Immutable, pat_id, init_expr_id) = &stmt.kind {
            if !reachable_expr_ids.contains(init_expr_id) {
                continue;
            }
            let pat = pkg.get_pat(*pat_id);
            if let PatKind::Bind(ident) = &pat.kind
                && matches!(pat.ty, Ty::Arrow(_))
            {
                let init_expr = pkg.get_expr(*init_expr_id);
                if let ExprKind::Var(Res::Item(item_id), generic_args) = &init_expr.kind {
                    candidates.insert(
                        ident.id,
                        ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
                    );
                }
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // exclude candidates that are captured by closures (within reachable code).
    for &expr_id in &scope.exprs {
        if !reachable_expr_ids.contains(&expr_id) {
            continue;
        }
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Closure(captures, _) = &expr.kind {
            for var in captures {
                candidates.remove(var);
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // count uses and record use-site expression IDs (within reachable code).
    let mut use_info: FxHashMap<LocalVarId, Vec<ExprId>> =
        candidates.keys().map(|&var| (var, Vec::new())).collect();

    for &expr_id in &scope.exprs {
        if !reachable_expr_ids.contains(&expr_id) {
            continue;
        }
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Var(Res::Local(var), _) = &expr.kind
            && let Some(uses) = use_info.get_mut(var)
        {
            uses.push(expr_id);
        }
    }

    // build replacements for single-use locals.
    let mut replacements = Vec::new();
    for (var, uses) in &use_info {
        if uses.len() == 1 {
            replacements.push((uses[0], candidates[var].clone()));
        }
    }

    replacements
}

/// Builds the owner boundaries used for single-use local promotion.
///
/// Each scope is rooted at either the package entry expression or one callable
/// implementation. Keeping the scopes separate prevents local-use counts from
/// crossing callable and closure ownership boundaries.
fn collect_promotion_scopes(pkg: &Package) -> Vec<PromotionScope<'_>> {
    let mut scopes = Vec::new();

    if let Some(entry_expr_id) = pkg.entry {
        let mut scope = PromotionScope::new(pkg);
        scope.visit_expr(entry_expr_id);
        scopes.push(scope);
    }

    for (_, item) in &pkg.items {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let mut scope = PromotionScope::new(pkg);
        scope.visit_callable_impl(&decl.implementation);
        scopes.push(scope);
    }

    scopes
}

/// FIR visited under one owner boundary for single-use local promotion.
///
/// A promotion scope is the entry expression or one callable implementation,
/// including its explicit specialization bodies. Local declarations in the
/// scope provide promotion candidates, and local references in the scope provide
/// use sites. Closure bodies are not walked through closure expressions here;
/// they are represented by their own callable scopes, while captured locals are
/// detected from the closure expression in the enclosing scope.
///
/// The `seen_*` sets make the traversal idempotent when a block, statement, or
/// expression is reachable from more than one root in the same callable
/// implementation.
struct PromotionScope<'a> {
    /// The package being analyzed.
    pkg: &'a Package,
    /// Statements that can introduce candidate immutable callable locals.
    stmts: Vec<StmtId>,
    /// Expressions whose local references are checked as use sites.
    exprs: Vec<ExprId>,
    /// Blocks already visited in this owner boundary.
    seen_blocks: FxHashSet<BlockId>,
    /// Statements already recorded in this owner boundary.
    seen_stmts: FxHashSet<StmtId>,
    /// Expressions already recorded in this owner boundary.
    seen_exprs: FxHashSet<ExprId>,
}

impl<'a> PromotionScope<'a> {
    fn new(pkg: &'a Package) -> Self {
        Self {
            pkg,
            stmts: Vec::new(),
            exprs: Vec::new(),
            seen_blocks: FxHashSet::default(),
            seen_stmts: FxHashSet::default(),
            seen_exprs: FxHashSet::default(),
        }
    }
}

impl<'a> Visitor<'a> for PromotionScope<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.pkg.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.pkg.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.pkg.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.pkg.get_stmt(id)
    }

    fn visit_block(&mut self, block_id: BlockId) {
        if self.seen_blocks.insert(block_id) {
            visit::walk_block(self, block_id);
        }
    }

    fn visit_stmt(&mut self, stmt_id: StmtId) {
        if self.seen_stmts.insert(stmt_id) {
            self.stmts.push(stmt_id);
            visit::walk_stmt(self, stmt_id);
        }
    }

    fn visit_expr(&mut self, expr_id: ExprId) {
        if self.seen_exprs.insert(expr_id) {
            self.exprs.push(expr_id);
            visit::walk_expr(self, expr_id);
        }
    }

    fn visit_pat(&mut self, _: PatId) {}
}

/// Replaces identity closures `(args) => f(args)` with direct references to
/// the callee in the package's expressions. An identity closure is one whose
/// body is a single call that forwards all actual parameters in order to a
/// callee that is either a global item or a single captured variable.
///
/// # Before
/// ```text
/// Closure([captures], target)   // target body: (args) => callee(args)
/// ```
/// # After (global callee)
/// ```text
/// Var(Item(callee_item))   // closure collapsed to direct item reference
/// ```
/// # After (captured-local callee)
/// ```text
/// Var(Local(outer_var))   // closure collapsed to outer-scope local
/// ```
/// # After (functor-wrapped callee)
/// ```text
/// UnOp(Functor(Adj), Var(Item(callee_item)))   // functor chain preserved
/// ```
///
/// # Mutations
/// - Rewrites `Expr.kind` at each identity-closure site in place.
fn identity_closure_peephole(
    store: &mut PackageStore,
    package_id: PackageId,
    reachable_expr_ids: &[ExprId],
) {
    // Collect replacements using an immutable borrow.
    let replacements = {
        let pkg = store.get(package_id);
        collect_identity_closures(pkg, reachable_expr_ids)
    };

    // Apply replacements using a mutable borrow.
    if !replacements.is_empty() {
        let pkg = store.get_mut(package_id);
        for (expr_id, new_kind) in replacements {
            pkg.exprs
                .get_mut(expr_id)
                .expect("expression should exist")
                .kind = new_kind;
        }
    }
}

/// Scans reachable expressions and collects `(ExprId, replacement ExprKind)` pairs
/// for identity closures.
fn collect_identity_closures(
    pkg: &Package,
    reachable_expr_ids: &[ExprId],
) -> Vec<(ExprId, ExprKind)> {
    let mut replacements = Vec::new();

    for &expr_id in reachable_expr_ids {
        let expr = pkg.get_expr(expr_id);
        if let ExprKind::Closure(captures, target) = &expr.kind {
            replacements.extend(check_identity_closure(pkg, expr_id, captures, *target));
        }
    }

    replacements
}

/// Checks whether a closure is an identity wrapper `(args) => f(args)` or a
/// functor-wrapped identity `(args) => Adjoint f(args)` /
/// `(args) => Controlled f(args)`, and returns expression replacements that
/// collapse the closure to a direct reference (optionally functor-applied).
fn check_identity_closure(
    pkg: &Package,
    closure_expr_id: ExprId,
    captures: &[LocalVarId],
    target: qsc_fir::fir::LocalItemId,
) -> Vec<(ExprId, ExprKind)> {
    // Get the closure's callable declaration.
    let Some(item) = pkg.items.get(target) else {
        return Vec::new();
    };
    let ItemKind::Callable(decl) = &item.kind else {
        return Vec::new();
    };

    // Only handle Spec implementations (not Intrinsic).
    let body_block_id = match &decl.implementation {
        CallableImpl::Spec(spec_impl) => spec_impl.body.block,
        _ => return Vec::new(),
    };

    let block = pkg.get_block(body_block_id);

    // Body must have exactly one statement.
    if block.stmts.len() != 1 {
        return Vec::new();
    }

    let stmt = pkg.get_stmt(block.stmts[0]);
    let call_expr_id = match &stmt.kind {
        StmtKind::Semi(e) | StmtKind::Expr(e) => *e,
        _ => return Vec::new(),
    };

    let call_expr = pkg.get_expr(call_expr_id);
    let (callee_id, args_id) = match &call_expr.kind {
        ExprKind::Call(callee, args) => (*callee, *args),
        _ => return Vec::new(),
    };

    // Parse the callable's input pattern to separate capture params from actual params.
    let Some(all_param_vars) = extract_flat_param_vars(pkg, decl.input) else {
        return Vec::new();
    };
    let num_captures = captures.len();
    if all_param_vars.len() < num_captures {
        return Vec::new();
    }
    let capture_param_vars = &all_param_vars[..num_captures];
    let actual_param_vars = &all_param_vars[num_captures..];

    // Must have at least one actual parameter to be a meaningful identity wrapper.
    if actual_param_vars.is_empty() {
        return Vec::new();
    }

    // Verify that args forward all actual params in order.
    if !args_forward_params_in_order(pkg, args_id, actual_param_vars) {
        return Vec::new();
    }

    // Ensure no capture params appear in the arguments.
    if captures_appear_in_args(pkg, args_id, capture_param_vars) {
        return Vec::new();
    }

    // Determine the replacement based on the callee expression.
    let callee_expr = pkg.get_expr(callee_id);
    match &callee_expr.kind {
        // Callee is a captured local variable — replace with the enclosing scope's var.
        ExprKind::Var(Res::Local(var), _) => {
            let Some(capture_idx) = capture_param_vars.iter().position(|&v| v == *var) else {
                return Vec::new();
            };
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Local(captures[capture_idx]), Vec::new()),
            )]
        }
        // Callee is a global item — replace with the global reference.
        ExprKind::Var(Res::Item(item_id), generic_args) => {
            vec![(
                closure_expr_id,
                ExprKind::Var(Res::Item(*item_id), generic_args.clone()),
            )]
        }
        // Callee is a functor-wrapped expression — replace closure with the functor
        // application and rewrite the inner expression to reference the enclosing scope.
        ExprKind::UnOp(UnOp::Functor(functor), inner_id) => {
            let inner_expr = pkg.get_expr(*inner_id);
            match &inner_expr.kind {
                ExprKind::Var(Res::Local(var), _) => {
                    let Some(capture_idx) = capture_param_vars.iter().position(|&v| v == *var)
                    else {
                        return Vec::new();
                    };
                    vec![
                        (
                            *inner_id,
                            ExprKind::Var(Res::Local(captures[capture_idx]), Vec::new()),
                        ),
                        (
                            closure_expr_id,
                            ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                        ),
                    ]
                }
                ExprKind::Var(Res::Item(_), _) => {
                    // Inner expression already references the global item; only
                    // the closure expression needs replacing.
                    vec![(
                        closure_expr_id,
                        ExprKind::UnOp(UnOp::Functor(*functor), *inner_id),
                    )]
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Extracts a flat list of `LocalVarId`s from a pattern. Returns `None` if the
/// pattern contains discards that cannot be mapped to individual variables.
fn extract_flat_param_vars(pkg: &Package, pat_id: qsc_fir::fir::PatId) -> Option<Vec<LocalVarId>> {
    let pat = pkg.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => Some(vec![ident.id]),
        PatKind::Tuple(sub_pats) => {
            let mut variables = Vec::new();
            for &sub_pat_id in sub_pats {
                variables.extend(extract_flat_param_vars(pkg, sub_pat_id)?);
            }
            Some(variables)
        }
        PatKind::Discard => None,
    }
}

/// Checks whether the args expression forwards exactly the given parameter
/// variables in order. Handles both single-variable and tuple cases.
fn args_forward_params_in_order(
    pkg: &Package,
    args_id: ExprId,
    actual_param_vars: &[LocalVarId],
) -> bool {
    extract_flat_arg_vars(pkg, args_id).is_some_and(|variables| variables == actual_param_vars)
}

/// Extracts a flat list of `LocalVarId`s from an arguments expression. Returns `None`
/// if the expression is not a simple variable or tuple of variables (e.g. if it
/// contains discards, literals, or complex expressions).
fn extract_flat_arg_vars(pkg: &Package, args_id: ExprId) -> Option<Vec<LocalVarId>> {
    let args_expr = pkg.get_expr(args_id);
    match &args_expr.kind {
        ExprKind::Var(Res::Local(var), _) => Some(vec![*var]),
        ExprKind::Tuple(elements) => {
            let mut variables = Vec::new();
            for &element_id in elements {
                variables.extend(extract_flat_arg_vars(pkg, element_id)?);
            }
            Some(variables)
        }
        _ => None,
    }
}

/// Returns `true` if any of the capture parameter variables appear in the
/// arguments expression.
fn captures_appear_in_args(
    pkg: &Package,
    args_id: ExprId,
    capture_param_vars: &[LocalVarId],
) -> bool {
    if capture_param_vars.is_empty() {
        return false;
    }
    match extract_flat_arg_vars(pkg, args_id) {
        Some(variables) => variables
            .iter()
            .any(|variable| capture_param_vars.contains(variable)),
        _ => true, // Conservatively assume captures may be used in complex expressions.
    }
}
