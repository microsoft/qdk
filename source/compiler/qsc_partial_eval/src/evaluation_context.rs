// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::functors::FunctorApp;
use qsc_eval::{
    Env, PackageSpan, Variable,
    val::{Result, Value},
};
use qsc_fir::fir::{LocalItemId, LocalVarId, PackageId, StoreItemId};
use qsc_rca::{ComputeKind, RuntimeFeatureFlags, ValueKind};
use qsc_rir::rir::{BlockId, Literal, VariableId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::hash_map::Entry, rc::Rc};

use crate::{Error, ScopeDbgContext, is_static_value, map_rir_literal_to_eval_value};

// Bound newly admitted runtime-branch recursion, including helper body evaluation.
// Thirty-two scopes leave stack headroom on native and debug Wasm hosts;
// 512 bodies separately bounds fan-out. Both production boundaries are tested.
/// Maximum active callable depth during runtime-branch recursive expansion.
pub const MAX_RECURSIVE_INLINE_DEPTH: usize = 32;
/// Maximum inline-body work charged to one runtime-branch recursive root.
pub const MAX_RECURSIVE_INLINE_WORK: usize = 512;

/// Struct that keeps track of the active RIR blocks (where RIR instructions are added) and the active scopes (which
/// correspond to the Q#'s program call stack).
pub struct EvaluationContext {
    /// Stack of RIR blocks being populated; the last entry receives new instructions.
    active_blocks: Vec<BlockNode>,
    /// Active evaluation scopes, including the initial non-callable scope and emitted bodies.
    scopes: Vec<Scope>,
    /// Cumulative inline-call charges, including nonrecursive calls. Each scope records
    /// a starting value so independent recursive roots receive separate work budgets.
    inline_work: usize,
    /// Index into `scopes` of the outermost active runtime-branch recursive root.
    /// Activated on recursive re-entry and cleared when that root scope is popped.
    recursive_root: Option<usize>,
    /// Maximum total active callable depth while a recursive root is active,
    /// including inline and newly emitted bodies, but excluding the initial scope.
    max_inline_depth: usize,
    /// Maximum inline-body work per recursive root, shared by its helpers and sibling branches.
    max_inline_work: usize,
}

impl EvaluationContext {
    #[cfg(test)]
    pub fn set_inline_limits(&mut self, depth: usize, work: usize) {
        self.max_inline_depth = depth;
        self.max_inline_work = work;
    }

    /// Creates a new evaluation context.
    pub fn new(package_id: PackageId, initial_block: BlockId) -> Self {
        let entry_callable_scope =
            Scope::new(package_id, None, Vec::new(), None, false, Vec::new());
        Self {
            active_blocks: vec![BlockNode {
                id: initial_block,
                successor: None,
            }],
            scopes: vec![entry_callable_scope],
            inline_work: 0,
            recursive_root: None,
            max_inline_depth: MAX_RECURSIVE_INLINE_DEPTH,
            max_inline_work: MAX_RECURSIVE_INLINE_WORK,
        }
    }

    /// Gets the ID of the current RIR block.
    pub fn get_current_block_id(&self) -> BlockId {
        self.active_blocks.last().expect("no active blocks").id
    }

    /// Gets an immutable reference to the current (call) scope.
    pub fn get_current_scope(&self) -> &Scope {
        self.scopes
            .last()
            .expect("the evaluation context does not have a current scope")
    }

    /// Gets an immutable reference to the caller (call) scope.
    pub fn get_caller_scope(&self) -> Option<&Scope> {
        self.scopes.iter().rev().nth(1)
    }

    /// Gets a mutable reference to the current (call) scope.
    pub fn get_current_scope_mut(&mut self) -> &mut Scope {
        self.scopes
            .last_mut()
            .expect("the evaluation context does not have a current scope")
    }

    /// Pops the currently active block.
    pub fn pop_block_node(&mut self) -> BlockNode {
        self.get_current_scope_mut().active_block_count -= 1;
        self.active_blocks
            .pop()
            .expect("there are no active blocks in the evaluation context")
    }

    /// Pops the currently active (call) scope.
    pub fn pop_scope(&mut self) -> Scope {
        let scope = self
            .scopes
            .pop()
            .expect("there are no scopes in the evaluation context");
        // After the pop, the new length is the removed scope's index. Clear the
        // budget root only when that scope exits, not when one of its descendants exits.
        if self.recursive_root == Some(self.scopes.len()) {
            self.recursive_root = None;
        }
        scope
    }

    /// Pushes a new active block.
    pub fn push_block_node(&mut self, b: BlockNode) {
        self.active_blocks.push(b);
        self.get_current_scope_mut().active_block_count += 1;
    }

    /// Pushes a new (call) scope.
    pub fn push_scope(&mut self, mut s: Scope) {
        s.inline_work_start = self.inline_work;
        self.scopes.push(s);
    }

    /// Checks the depth limit before entering an inline or newly emitted callable body.
    ///
    /// Only an active recursive root enables this limit; ordinary static expansion is
    /// unaffected. This does not charge work or activate a root. On failure, the diagnostic
    /// points to the proposed call's `span`, and the caller must not enter the new scope.
    pub fn check_expansion_depth(&self, span: PackageSpan) -> std::result::Result<(), Error> {
        // The entry scope has no callable, so the existing scope count is
        // the callable depth after entering the proposed next scope.
        if self.recursive_root.is_some() && self.scopes.len() > self.max_inline_depth {
            return Err(Error::RecursiveExpansionLimitExceeded(
                "depth",
                self.max_inline_depth,
                span,
            ));
        }
        Ok(())
    }

    /// Charges one inline-body entry before the callee scope is pushed.
    ///
    /// Re-entering a matching specialization active in a runtime branch activates its
    /// outermost root, retaining any earlier active root. All subsequent inline entries,
    /// including helpers and sibling branches, share that root's work budget until it exits.
    /// The root's saved work excludes its own entry, so the `>=` check counts that body too.
    ///
    /// Enforces both depth and work limits, reporting failure at the call's `span`.
    /// Emitted runtime calls must use the separate depth check rather than charge inline work.
    pub fn charge_inline_call(
        &mut self,
        callable: StoreItemId,
        functor: FunctorApp,
        span: PackageSpan,
    ) -> std::result::Result<(), Error> {
        // Count every inline entry, even before recursion is detected, so activating
        // a root includes work already performed within it. Never let the counter wrap.
        self.inline_work =
            self.inline_work
                .checked_add(1)
                .ok_or(Error::RecursiveExpansionLimitExceeded(
                    "work",
                    self.max_inline_work,
                    span,
                ))?;
        // Scopes are ordered outermost first. Keep the earliest matching runtime-branch
        // scope, or an already active ancestor, so nested recursion cannot reset its budget.
        if let Some(root) = self.scopes.iter().position(|scope| {
            scope.matches_callable(callable, functor) && scope.is_currently_evaluating_branch()
        }) {
            self.recursive_root = Some(self.recursive_root.map_or(root, |active| active.min(root)));
        }
        // Check after activating the root so the first recursive re-entry is bounded too.
        self.check_expansion_depth(span)?;
        // Subtract the root's entry baseline to exclude unrelated earlier calls.
        // The baseline was saved after charging the root itself; `>=` includes that
        // body in the limit while allowing exactly max_inline_work bodies in total.
        if let Some(root) = self.recursive_root
            && self.inline_work - self.scopes[root].inline_work_start >= self.max_inline_work
        {
            return Err(Error::RecursiveExpansionLimitExceeded(
                "work",
                self.max_inline_work,
                span,
            ));
        }
        Ok(())
    }

    /// Determines whether we are currently in a dynamic branch context for any scope.
    pub fn is_currently_evaluating_any_branch(&self) -> bool {
        self.scopes
            .iter()
            .any(Scope::is_currently_evaluating_branch)
    }

    /// Different control counts still use the same controlled specialization.
    pub fn is_callable_in_dynamic_branch(
        &self,
        callable: StoreItemId,
        functor: FunctorApp,
    ) -> bool {
        self.scopes.iter().any(|scope| {
            scope.matches_callable(callable, functor) && scope.is_currently_evaluating_branch()
        })
    }

    /// Checks whether a known classical input differs from the nearest active invocation.
    pub fn has_changed_static_input(
        &self,
        callable: StoreItemId,
        functor: FunctorApp,
        args: &[Arg],
    ) -> bool {
        let Some(scope) = self
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.matches_callable(callable, functor))
        else {
            return false;
        };
        args.iter().any(|arg| {
            let Arg::Var(id, var) = arg else {
                return false;
            };
            is_static_value(&var.value)
                && !matches!(var.value, Value::Qubit(_))
                && scope
                    .hybrid_vars
                    .get(id)
                    .is_some_and(|previous| match (&var.value, previous) {
                        (Value::Double(value), Value::Double(previous)) => {
                            value.to_bits() != previous.to_bits()
                        }
                        _ => &var.value != previous,
                    })
        })
    }
}

/// Struct that represents a block node when we interpret an RIR program as a graph.
pub struct BlockNode {
    /// The ID of the block.
    pub id: BlockId,
    /// The block to jump to (if any) once all instructions to the block have been added.
    pub successor: Option<BlockId>,
}

/// A call scope.
pub struct Scope {
    /// The package ID of the callable.
    pub package_id: PackageId,
    /// The ID and functor information of the callable.
    pub callable: Option<(LocalItemId, FunctorApp)>,
    /// The compute kind of the arguments passed to the callable.
    pub args_compute_kind: Vec<ComputeKind>,
    /// Whether the callable is being invoked from inside a parallel expression.
    pub in_parallel: bool,
    /// The classical environment of the callable, which holds values corresponding to local variables.
    pub env: Env,
    /// Map that holds the values of local variables.
    hybrid_vars: FxHashMap<LocalVarId, Value>,
    /// Maps variable IDs to static literal values, if any.
    static_vars: FxHashMap<VariableId, Literal>,
    /// Set of mutable local variables in this scope. Used to decide which variables are tracked across
    /// the partial eval <-> eval boundary (hybrid map <-> classical env).
    mutable_vars: FxHashSet<LocalVarId>,
    /// Number of currently active blocks (starting from where this scope was created).
    active_block_count: usize,
    /// Debug context, used for generating debug metadata.
    pub(crate) dbg_context: ScopeDbgContext,
    /// Any locally constant arrays associated with this scope, along with the variable ID
    /// they are assigned (which happens after the scope is created).
    pub(crate) arrays: Vec<(Rc<Vec<Value>>, Option<VariableId>)>,
    inline_work_start: usize,
}

impl Scope {
    fn matches_callable(&self, callable: StoreItemId, functor: FunctorApp) -> bool {
        self.package_id == callable.package
            && self.callable.is_some_and(|(item, active)| {
                item == callable.item
                    && active.adjoint == functor.adjoint
                    && (active.controlled > 0) == (functor.controlled > 0)
            })
    }

    /// Creates a new call scope.
    pub fn new(
        package_id: PackageId,
        callable: Option<(LocalItemId, FunctorApp)>,
        args: Vec<Arg>,
        ctls_arg: Option<Arg>,
        in_parallel: bool,
        arrays: Vec<Rc<Vec<Value>>>,
    ) -> Self {
        // Create the environment for the classical evaluator.
        // A default classical evaluator environment is created with one scope. However, we need to push an additional
        // scope to the environment to be able to detect whether the classical evaluator has returned from the call
        // scope.
        const CLASSICAL_EVALUATOR_CALL_SCOPE_ID: usize = 1;
        let mut env = Env::default();
        env.push_scope(CLASSICAL_EVALUATOR_CALL_SCOPE_ID);

        // Determine the compute kind of the arguments, assuming empty feature flags.
        let args_compute_kind: Vec<ComputeKind> = args
            .iter()
            .map(|arg| {
                let value = match arg {
                    Arg::Discard(value) => value,
                    Arg::Var(_, var) => &var.value,
                };
                if is_static_value(value) {
                    // When values are statically known, treat them as such so the new scope's
                    // application generator set is seeded properly.
                    ComputeKind::Static
                } else {
                    ComputeKind::Dynamic {
                        runtime_features: RuntimeFeatureFlags::empty(),
                        value_kind: map_eval_value_to_value_kind(value),
                    }
                }
            })
            .collect();

        let mut hybrid_vars = FxHashMap::default();

        // Bind the control qubits to both the hybrid and classical maps.
        if let Some(Arg::Var(local_var_id, var)) = ctls_arg {
            hybrid_vars.insert(local_var_id, var.value.clone());
            env.bind_variable_in_top_frame(local_var_id, var);
        }

        // Add the values to both environments.
        for arg in args {
            let Arg::Var(local_var_id, var) = arg else {
                continue;
            };
            hybrid_vars.insert(local_var_id, var.value.clone());
            env.bind_variable_in_top_frame(local_var_id, var);
        }

        // Add the dynamic values to the hybrid variables
        Self {
            package_id,
            callable,
            args_compute_kind,
            in_parallel,
            env,
            active_block_count: 1,
            hybrid_vars,
            static_vars: FxHashMap::default(),
            mutable_vars: FxHashSet::default(),
            dbg_context: ScopeDbgContext::default(),
            arrays: arrays.into_iter().map(|array| (array, None)).collect(),
            inline_work_start: 0,
        }
    }

    /// Gets the static literal value for the given variable.
    pub fn get_static_value(&self, var_id: VariableId) -> Option<&Literal> {
        self.static_vars.get(&var_id)
    }

    /// Removes the static literal value for the given variable.
    pub fn remove_static_value(&mut self, var_id: VariableId) {
        self.static_vars.remove(&var_id);
    }

    /// Clones the static literal value mappings, which allows callers to cache the mappings across branches.
    pub fn clone_static_var_mappings(&self) -> FxHashMap<VariableId, Literal> {
        self.static_vars.clone()
    }

    /// Sets the static literal value mappings to the given mapping, overwriting any existing mappings.
    pub fn set_static_var_mappings(&mut self, static_vars: FxHashMap<VariableId, Literal>) {
        self.static_vars = static_vars;
    }

    /// Keeps only the static literal value mappings that are also present in the provided other mapping.
    pub fn keep_matching_static_var_mappings(
        &mut self,
        other_mappings: &FxHashMap<VariableId, Literal>,
    ) {
        self.static_vars
            .retain(|var_id, lit| Some(&*lit) == other_mappings.get(var_id));
    }

    /// Gets the value of a hybrid local variable.
    pub fn get_classical_local_value(&self, local_var_id: LocalVarId) -> &Value {
        &self
            .env
            .get(local_var_id)
            .expect("local classcial variable value does not exist")
            .value
    }

    /// Updates the classical local variable values based on the current hybrid local variable values.
    pub fn update_classical_locals_from_hybrid_locals(&mut self) {
        for local_var_id in &self.mutable_vars {
            if self.env.get(*local_var_id).is_none() {
                continue;
            }
            let hybrid_value = self
                .hybrid_vars
                .get(local_var_id)
                .expect("hybrid value should exist");
            let update_value = match hybrid_value {
                Value::Var(hybrid_var) => {
                    // Check to see if there is a static literal value currently tracked for this variable,
                    // and if so, use it to update the classical local variable value instead.
                    if let Some(literal) = self.get_static_value(hybrid_var.id.into())
                        && let Some(literal_val) =
                            map_rir_literal_to_eval_value(*literal, hybrid_var.ty)
                    {
                        literal_val
                    } else {
                        hybrid_value.clone()
                    }
                }
                _ => hybrid_value.clone(),
            };
            if let Some(classical_var) = self.env.get_mut(*local_var_id) {
                classical_var.value = update_value;
            }
        }
    }

    /// Collects the local variable values that have been updated based on the current hybrid local variable values,
    /// allowing callers to choose how to update those in tracked mapping themselves.
    pub fn collect_updated_local_values(&mut self) -> Vec<(LocalVarId, Value)> {
        let mut updated_values = Vec::new();
        for local_var_id in &self.mutable_vars {
            let hybrid_value = self
                .hybrid_vars
                .get(local_var_id)
                .expect("hybrid value should exist");
            if let Value::Var(hybrid_var) = hybrid_value
                && let Some(literal) = self.get_static_value(hybrid_var.id.into())
                && let Some(literal_val) = map_rir_literal_to_eval_value(*literal, hybrid_var.ty)
            {
                // If there is a tracked static literal value for this hybrid variable, use that to check for changes
                // against the classical local variable value instead of the variable itself.
                if let Some(classical_value) = self.env.get(*local_var_id).map(|var| &var.value)
                    && classical_value != &literal_val
                {
                    updated_values.push((*local_var_id, classical_value.clone()));
                }
            } else if let Some(classical_value) = self.env.get(*local_var_id).map(|var| &var.value)
                && classical_value != hybrid_value
            {
                updated_values.push((*local_var_id, classical_value.clone()));
            }
        }
        updated_values
    }

    /// Gets the value of a hybrid local variable.
    pub fn get_hybrid_local_value(&self, local_var_id: LocalVarId) -> &Value {
        self.hybrid_vars
            .get(&local_var_id)
            .expect("local hybrid variable value does not exist")
    }

    // Inserts a value in the hybrid vars map.
    pub fn insert_hybrid_local_value(&mut self, local_var_id: LocalVarId, value: Value) {
        self.hybrid_vars.insert(local_var_id, value);
    }

    // Insert a variable into the mutable variables map.
    pub fn insert_static_var_mapping(&mut self, var_id: VariableId, literal: Literal) {
        self.static_vars.insert(var_id, literal);
    }

    // Inserts a local variable into the set of mutable variables so it will be tracked
    // for updates across evaluation boundaries.
    pub fn insert_mutable_var(&mut self, local_var_id: LocalVarId) {
        self.mutable_vars.insert(local_var_id);
    }

    /// Determines whether we are currently evaluating a branch within the scope.
    pub fn is_currently_evaluating_branch(&self) -> bool {
        self.active_block_count > 1
    }

    /// Determines whether the classical evaluator has returned from the call scope.
    /// This relies on the fact that the classical evaluator pops the scope when it encounters a return, so when this
    /// happens the number of scopes in the environment will be exactly one.
    pub fn has_classical_evaluator_returned(&self) -> bool {
        self.env.len() == 1
    }

    /// Updates the value of a hybrid local variable.
    pub fn update_hybrid_local_value(&mut self, local_var_id: LocalVarId, value: Value) {
        let Entry::Occupied(mut occupied) = self.hybrid_vars.entry(local_var_id) else {
            panic!("local variable to update does not exist");
        };
        occupied.insert(value);
    }
}

/// A call argument.
pub enum Arg {
    Discard(Value),
    Var(LocalVarId, Variable),
}

impl Arg {
    pub fn value(&self) -> &Value {
        match self {
            Self::Discard(value) => value,
            Self::Var(_, var) => &var.value,
        }
    }

    /// Converts the argument into its underlying value.
    pub fn into_value(self) -> Value {
        match self {
            Self::Discard(value) => value,
            Self::Var(_, var) => var.value,
        }
    }
}

/// Represents the possible control flow options that an evaluation can have.
///
/// Note: The `Return` variant is vestigial for the production pipeline.
/// The `return_unify` FIR transform pass eliminates all `ExprKind::Return`
/// nodes before partial evaluation runs. However, partial eval unit tests
/// bypass FIR transforms and evaluate raw FIR, so the `Return` variant
/// and its handling code remain for test compatibility.
pub enum EvalControlFlow {
    Continue(Value),
    Return(Value),
}

impl EvalControlFlow {
    /// Consumes the evaluation control flow and returns its value.
    pub fn into_value(self) -> Value {
        match self {
            EvalControlFlow::Continue(value) | EvalControlFlow::Return(value) => value,
        }
    }

    /// Whether this evaluation control flow is a return.
    pub fn is_return(&self) -> bool {
        match self {
            Self::Continue(_) => false,
            Self::Return(_) => true,
        }
    }

    pub fn map_value<F>(self, f: F) -> Self
    where
        F: FnOnce(Value) -> Value,
    {
        match self {
            Self::Continue(value) => Self::Continue(f(value)),
            Self::Return(value) => Self::Return(f(value)),
        }
    }
}

fn map_eval_value_to_value_kind(value: &Value) -> ValueKind {
    match value {
        Value::Array(elements) => {
            for element in elements.iter() {
                let element_runtime_kind = map_eval_value_to_value_kind(element);
                if element_runtime_kind == ValueKind::Variable {
                    return ValueKind::Variable;
                }
            }

            ValueKind::Constant
        }
        Value::Tuple(elements, _) => {
            for element in elements.iter() {
                let element_runtime_kind = map_eval_value_to_value_kind(element);
                if element_runtime_kind == ValueKind::Variable {
                    return ValueKind::Variable;
                }
            }
            ValueKind::Constant
        }
        Value::Result(Result::Loss) | Value::Var(_) => ValueKind::Variable,
        Value::BigInt(_)
        | Value::Bool(_)
        | Value::Closure(_)
        | Value::Double(_)
        | Value::Global(_, _)
        | Value::Int(_)
        | Value::Pauli(_)
        | Value::Qubit(_)
        | Value::Range(_)
        | Value::Result(Result::Val(_) | Result::Id(_))
        | Value::String(_) => ValueKind::Constant,
    }
}
