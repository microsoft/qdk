// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::functors::FunctorApp;
use qsc_eval::{
    Env, PackageSpan, Variable,
    val::{Result, Value},
};
use qsc_fir::fir::{ExprId, LocalItemId, LocalVarId, PackageId};
use qsc_lowerer::map_fir_package_to_hir;
use qsc_rca::{RuntimeKind, ValueKind};
use qsc_rir::rir::{BlockId, Literal, VariableId};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

/// Struct that keeps track of the active RIR blocks (where RIR instructions are added) and the active scopes (which
/// correspond to the Q#'s program call stack).
pub struct EvaluationContext {
    active_blocks: Vec<BlockNode>,
    scopes: Vec<Scope>,
    pub current_user_source_block: Vec<qsc_fir::fir::BlockId>,
    pub current_iteration: Option<usize>,

    pub dbg_callable_to_scope: FxHashMap<(LocalItemId), usize>,
}

impl EvaluationContext {
    /// Creates a new evaluation context.
    pub fn new(package_id: PackageId, initial_block: BlockId) -> Self {
        let entry_callable_scope = Scope::new(package_id, None, Vec::new(), None, None);
        Self {
            active_blocks: vec![BlockNode {
                id: initial_block,
                successor: None,
            }],
            scopes: vec![entry_callable_scope],
            current_user_source_block: vec![],
            current_iteration: None,
            dbg_callable_to_scope: Default::default(),
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

    /// Gets an immutable reference to the current (call) scope.
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
        self.scopes
            .pop()
            .expect("there are no scopes in the evaluation context")
    }

    /// Pushes a new active block.
    pub fn push_block_node(&mut self, b: BlockNode) {
        self.active_blocks.push(b);
        self.get_current_scope_mut().active_block_count += 1;
    }

    /// Pushes a new (call) scope.
    pub fn push_scope(&mut self, s: Scope) {
        self.scopes.push(s);
    }

    /// Determines whether we are currently in a dynamic branch context for any scope.
    pub fn is_currently_evaluating_any_branch(&self) -> bool {
        self.scopes
            .iter()
            .any(Scope::is_currently_evaluating_branch)
    }

    pub fn get_current_user_caller(&self) -> Option<PackageSpan> {
        // Yeah, stdlib id is hardcoded that's terrible etc.
        self.scopes.iter().rev().find_map(|scope| {
            scope.caller_expr_span.filter(|s| {
                s.package != map_fir_package_to_hir(PackageId::CORE)
                    && s.package != map_fir_package_to_hir(PackageId::from(1))
            })
        })
    }

    pub fn get_current_user_scope(&self) -> Option<&Scope> {
        self.scopes.iter().rev().find(|scope| {
            scope.package_id != PackageId::CORE && scope.package_id != PackageId::from(1)
        })
    }

    pub fn is_current_scope_user_scope(&self) -> bool {
        let scope = self.get_current_scope();
        scope.package_id != PackageId::CORE && scope.package_id != PackageId::from(1)
    }
}

/// Struct that represents a block node when we intepret an RIR program as a graph.
#[derive(Debug)]
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
    /// The value of the arguments passed to the callable.
    pub args_value_kind: Vec<ValueKind>,
    /// The classical environment of the callable, which holds values corresponding to local variables.
    pub env: Env,
    /// Map that holds the values of local variables.
    hybrid_vars: FxHashMap<LocalVarId, Value>,
    /// Maps variable IDs to static literal values, if any.
    static_vars: FxHashMap<VariableId, Literal>,
    /// Number of currently active blocks (starting from where this scope was created).
    active_block_count: usize,
    /// The current expression being evaluated, if any.
    /// index into `dbg_locations` in `Program`
    pub current_distinct_dbg_location: Option<usize>,
    /// The current expression being evaluated, if any.
    pub current_expr: Option<ExprId>,

    ///
    caller_expr_span: Option<PackageSpan>,
}

impl Scope {
    /// Creates a new call scope.
    pub fn new(
        package_id: PackageId,
        callable: Option<(LocalItemId, FunctorApp)>,
        args: Vec<Arg>,
        ctls_arg: Option<Arg>,
        caller_expr_span: Option<PackageSpan>,
    ) -> Self {
        // Create the environment for the classical evaluator.
        // A default classical evaluator environment is created with one scope. However, we need to push an additional
        // scope to the environment to be able to detect whether the classical evaluator has returned from the call
        // scope.
        const CLASSICAL_EVALUATOR_CALL_SCOPE_ID: usize = 1;
        let mut env = Env::default();
        env.push_scope(CLASSICAL_EVALUATOR_CALL_SCOPE_ID);

        // Determine the runtime kind (static or dynamic) of the arguments.
        let args_value_kind: Vec<ValueKind> = args
            .iter()
            .map(|arg| {
                let value = match arg {
                    Arg::Discard(value) => value,
                    Arg::Var(_, var) => &var.value,
                };
                map_eval_value_to_value_kind(value)
            })
            .collect();

        let mut hybrid_vars = FxHashMap::default();

        // Bind the control qubits to both the hybrid and classical maps.
        if let Some(Arg::Var(local_var_id, var)) = ctls_arg {
            hybrid_vars.insert(local_var_id, var.value.clone());
            env.bind_variable_in_top_frame(local_var_id, var);
        }

        // Add the values to both the classical environment and the hybrid variables depending on whether the value is
        // static or dynamic.
        let arg_runtime_kind_tuple = args.into_iter().zip(args_value_kind.iter());
        for (arg, _) in arg_runtime_kind_tuple {
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
            args_value_kind,
            env,
            active_block_count: 1,
            hybrid_vars,
            static_vars: FxHashMap::default(),
            caller_expr_span,
            current_distinct_dbg_location: None,
            current_expr: None,
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
    /// Converts the argument into its underlying value.
    pub fn into_value(self) -> Value {
        match self {
            Self::Discard(value) => value,
            Self::Var(_, var) => var.value,
        }
    }
}

/// Represents the possible control flow options that an evaluation can have.
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
}

fn map_eval_value_to_value_kind(value: &Value) -> ValueKind {
    fn map_array_eval_value_to_value_kind(elements: &[Value]) -> ValueKind {
        let mut content_runtime_kind = RuntimeKind::Static;
        for element in elements {
            let element_value_kind = map_eval_value_to_value_kind(element);
            if element_value_kind.is_dynamic() {
                content_runtime_kind = RuntimeKind::Dynamic;
                break;
            }
        }

        // The runtime capabilities check pass disallows dynamically-sized arrays for all targets for which we generate
        // QIR. Because of this, we assume that during partial evaluation all arrays are statically-sized.
        ValueKind::Array(content_runtime_kind, RuntimeKind::Static)
    }

    fn map_tuple_eval_value_to_value_kind(elements: &[Value]) -> ValueKind {
        let mut runtime_kind = RuntimeKind::Static;
        for element in elements {
            let element_value_kind = map_eval_value_to_value_kind(element);
            if element_value_kind.is_dynamic() {
                runtime_kind = RuntimeKind::Dynamic;
                break;
            }
        }
        ValueKind::Element(runtime_kind)
    }

    match value {
        Value::Array(elements) => map_array_eval_value_to_value_kind(elements),
        Value::Tuple(elements, _) => map_tuple_eval_value_to_value_kind(elements),
        Value::Result(Result::Id(_) | Result::Loss) | Value::Var(_) => {
            ValueKind::Element(RuntimeKind::Dynamic)
        }
        Value::BigInt(_)
        | Value::Bool(_)
        | Value::Closure(_)
        | Value::Double(_)
        | Value::Global(_, _)
        | Value::Int(_)
        | Value::Pauli(_)
        | Value::Qubit(_)
        | Value::Range(_)
        | Value::Result(Result::Val(_))
        | Value::String(_) => ValueKind::Element(RuntimeKind::Static),
    }
}
