// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::functors::FunctorApp;
use qsc_eval::{val::Value, Env};
use qsc_fir::fir::{ExprId, LocalItemId, LocalVarId, PackageId};
use qsc_rca::ValueKind;
use qsc_rir::rir::BlockId;
use rustc_hash::FxHashMap;

pub struct EvaluationContext {
    active_blocks: Vec<BlockNode>,
    scopes: Vec<Scope>,
}

impl EvaluationContext {
    pub fn new(package_id: PackageId, initial_block: BlockId) -> Self {
        let entry_callable_scope = Scope::new(package_id, None, Vec::new());
        Self {
            active_blocks: vec![BlockNode {
                id: initial_block,
                next: None,
            }],
            scopes: vec![entry_callable_scope],
        }
    }

    pub fn get_current_block_id(&self) -> BlockId {
        self.active_blocks.last().expect("no active blocks").id
    }

    pub fn get_current_scope(&self) -> &Scope {
        self.scopes
            .last()
            .expect("the evaluation context does not have a current scope")
    }

    pub fn get_current_scope_mut(&mut self) -> &mut Scope {
        self.scopes
            .last_mut()
            .expect("the evaluation context does not have a current scope")
    }

    pub fn pop_block_node(&mut self) -> BlockNode {
        self.active_blocks
            .pop()
            .expect("there are no active blocks in the evaluation context")
    }

    pub fn pop_scope(&mut self) -> Scope {
        self.scopes
            .pop()
            .expect("there are no scopes in the evaluation context")
    }

    pub fn push_block_node(&mut self, b: BlockNode) {
        self.active_blocks.push(b);
    }

    pub fn push_scope(&mut self, s: Scope) {
        self.scopes.push(s);
    }
}

pub struct BlockNode {
    pub id: BlockId,
    pub next: Option<BlockId>,
}

pub struct Scope {
    pub package_id: PackageId,
    pub callable: Option<(LocalItemId, FunctorApp)>,
    pub args_runtime_properties: Vec<ValueKind>,
    pub env: Env,
    last_expr: Option<ExprId>,
    hybrid_exprs: FxHashMap<ExprId, Value>,
    hybrid_vars: FxHashMap<LocalVarId, Value>,
}

impl Scope {
    pub fn new(
        package_id: PackageId,
        callable: Option<(LocalItemId, FunctorApp)>,
        args_runtime_properties: Vec<ValueKind>,
    ) -> Self {
        Self {
            package_id,
            callable,
            args_runtime_properties,
            env: Env::default(),
            last_expr: None,
            hybrid_exprs: FxHashMap::default(),
            hybrid_vars: FxHashMap::default(),
        }
    }

    // Potential candidate for removal if only the last expression value is needed.
    pub fn _get_expr_value(&self, expr_id: ExprId) -> &Value {
        self.hybrid_exprs
            .get(&expr_id)
            .expect("expression value does not exist")
    }

    pub fn get_local_var_value(&self, local_var_id: LocalVarId) -> &Value {
        self.hybrid_vars
            .get(&local_var_id)
            .expect("local variable value does not exist")
    }

    pub fn insert_expr_value(&mut self, expr_id: ExprId, value: Value) {
        self.last_expr = Some(expr_id);
        self.hybrid_exprs.insert(expr_id, value);
    }

    pub fn insert_local_var_value(&mut self, local_var_id: LocalVarId, value: Value) {
        self.hybrid_vars.insert(local_var_id, value);
    }

    pub fn clear_last_expr(&mut self) {
        self.last_expr = None;
    }

    pub fn last_expr_value(&self) -> Value {
        self.last_expr
            .and_then(|expr_id| self.hybrid_exprs.get(&expr_id))
            .map_or_else(Value::unit, Clone::clone)
    }
}
