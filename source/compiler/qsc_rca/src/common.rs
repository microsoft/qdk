// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indenter::Indented;
use qsc_data_structures::functors::FunctorApp;
use qsc_fir::{
    extensions::{InputParam, InputParamIndex},
    fir::{
        Block, BlockId, Expr, ExprId, ExprKind, Functor, ItemId, LocalItemId, LocalVarId, Package,
        PackageId, PackageLookup, Pat, PatId, Res, Stmt, StmtId, StoreItemId, UnOp,
    },
    ty::{FunctorSetValue, Ty},
    visit::{Visitor, walk_expr, walk_stmt},
};
use rustc_hash::FxHashMap;
use std::{
    cmp::max,
    fmt::{Debug, Formatter},
};

/// A representation of a local symbol.
#[derive(Clone, Debug)]
pub struct Local {
    pub var: LocalVarId,
    pub kind: LocalKind,
}

/// Kinds of local symbols.
#[derive(Clone, Debug)]
pub enum LocalKind {
    /// An input parameter with its associated index.
    InputParam(InputParamIndex),
    /// A specialization input (i.e. control qubits).
    SpecInput,
    /// An immutable binding with the expression associated to it.
    Immutable(ExprId),
    /// A mutable binding.
    Mutable,
}

pub trait LocalsLookup {
    fn find(&self, local_var_id: LocalVarId) -> Option<&Local>;
}

impl LocalsLookup for FxHashMap<LocalVarId, Local> {
    fn find(&self, local_var_id: LocalVarId) -> Option<&Local> {
        self.get(&local_var_id)
    }
}

pub fn initialize_locals_map(input_params: &Vec<InputParam>) -> FxHashMap<LocalVarId, Local> {
    let mut locals_map = FxHashMap::<LocalVarId, Local>::default();
    for param in input_params {
        if let Some(id) = param.var {
            locals_map.insert(
                id,
                Local {
                    var: id,
                    kind: LocalKind::InputParam(param.index),
                },
            );
        }
    }
    locals_map
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LocalSpecId {
    pub callable: LocalItemId,
    pub functor_set_value: FunctorSetValue,
}

impl From<(LocalItemId, FunctorSetValue)> for LocalSpecId {
    fn from(value: (LocalItemId, FunctorSetValue)) -> Self {
        Self {
            callable: value.0,
            functor_set_value: value.1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlobalSpecId {
    pub callable: StoreItemId,
    pub functor_set_value: FunctorSetValue,
}

impl From<(PackageId, LocalSpecId)> for GlobalSpecId {
    fn from(value: (PackageId, LocalSpecId)) -> Self {
        Self {
            callable: (value.0, value.1.callable).into(),
            functor_set_value: value.1.functor_set_value,
        }
    }
}

impl From<(StoreItemId, FunctorSetValue)> for GlobalSpecId {
    fn from(value: (StoreItemId, FunctorSetValue)) -> Self {
        Self {
            callable: value.0,
            functor_set_value: value.1,
        }
    }
}

pub trait FunctorAppExt {
    fn functor_set_value(&self) -> FunctorSetValue;
}

impl FunctorAppExt for FunctorApp {
    fn functor_set_value(&self) -> FunctorSetValue {
        let adjoint = self.adjoint;
        let controlled = self.controlled > 0;
        match (adjoint, controlled) {
            (false, false) => FunctorSetValue::Empty,
            (true, false) => FunctorSetValue::Adj,
            (false, true) => FunctorSetValue::Ctl,
            (true, true) => FunctorSetValue::CtlAdj,
        }
    }
}

pub trait TyExt {
    fn has_type_parameters(&self) -> bool;
}

impl TyExt for Ty {
    fn has_type_parameters(&self) -> bool {
        match self {
            Self::Array(ty) => ty.has_type_parameters(),
            Self::Arrow(arrow) => {
                arrow.input.has_type_parameters() || arrow.output.has_type_parameters()
            }
            Self::Infer(_) | Self::Prim(_) | Self::Udt(_) => false,
            Self::Param(_) => true,
            Self::Tuple(types) => types.iter().any(TyExt::has_type_parameters),
            Self::Err => panic!("unexpected type error"),
        }
    }
}

#[derive(Debug)]
pub struct Callee {
    pub item: StoreItemId,
    pub functor_app: FunctorApp,
}

/// Tries to uniquely resolve the callable specialization referenced in a callee expression.
pub fn try_resolve_callee(
    expr_id: ExprId,
    package_id: PackageId,
    package: &impl PackageLookup,
    locals_map: &impl LocalsLookup,
) -> (Option<Callee>, Option<Vec<LocalVarId>>) {
    // This is a best effort attempt to resolve a callee that currently only supports resolving
    // global callables or locals that eventually resolve to global callables.
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::UnOp(operator, operand_expr_id) => {
            try_resolve_un_op_callee(*operator, *operand_expr_id, package_id, package, locals_map)
        }
        ExprKind::Var(res, _) => match res {
            Res::Item(item_id) => (Some(resolve_item_callee(package_id, *item_id)), None),
            Res::Local(local_var_id) => {
                try_resolve_local_callee(*local_var_id, package_id, package, locals_map)
            }
            Res::Err => panic!("callee resolution should not be an error"),
        },
        ExprKind::Closure(fixed_args, local_item_id) => (
            Some(Callee {
                item: (package_id, *local_item_id).into(),
                functor_app: FunctorApp::default(),
            }),
            Some(fixed_args.clone()),
        ),
        // More complex callee expressions might require evaluation so we don't try to resolve them at compile time.
        _ => (None, None),
    }
}

fn resolve_item_callee(call_package_id: PackageId, item_id: ItemId) -> Callee {
    let package_id = item_id.package.unwrap_or(call_package_id);
    Callee {
        item: (package_id, item_id.item).into(),
        functor_app: FunctorApp::default(),
    }
}

fn try_resolve_local_callee(
    local_var_id: LocalVarId,
    package_id: PackageId,
    package: &impl PackageLookup,
    locals_map: &impl LocalsLookup,
) -> (Option<Callee>, Option<Vec<LocalVarId>>) {
    // This is a best effort attempt to resolve a callee.
    match locals_map.find(local_var_id) {
        Some(local) => match local.kind {
            LocalKind::Immutable(expr_id) => {
                try_resolve_callee(expr_id, package_id, package, locals_map)
            }
            _ => (None, None),
        },
        None => (None, None),
    }
}

fn try_resolve_un_op_callee(
    operator: UnOp,
    expr_id: ExprId,
    package_id: PackageId,
    package: &impl PackageLookup,
    locals_map: &impl LocalsLookup,
) -> (Option<Callee>, Option<Vec<LocalVarId>>) {
    // This is a best effort attempt to resolve a callee.
    let UnOp::Functor(functor_operator) = operator else {
        panic!("unary operator is expected to be a functor for a callee expression")
    };

    let (callee, fixed_args) = try_resolve_callee(expr_id, package_id, package, locals_map);
    (
        callee.map(|callee| {
            let spec_functor = match functor_operator {
                Functor::Adj => FunctorApp {
                    adjoint: !callee.functor_app.adjoint,
                    controlled: callee.functor_app.controlled,
                },
                Functor::Ctl => FunctorApp {
                    adjoint: callee.functor_app.adjoint,
                    controlled: callee.functor_app.controlled + 1,
                },
            };
            Callee {
                item: callee.item,
                functor_app: spec_functor,
            }
        }),
        fixed_args,
    )
}

pub struct AssignmentStmtCounter<'a> {
    package: &'a Package,
    assignment_expr_count: usize,
    assignment_stmt_count: usize,
}

impl<'a> AssignmentStmtCounter<'a> {
    pub fn new(package: &'a Package) -> Self {
        Self {
            package,
            assignment_expr_count: 0,
            assignment_stmt_count: 0,
        }
    }

    pub fn count_in_expr(mut self, expr_id: ExprId) -> usize {
        self.visit_expr(expr_id);
        max(self.assignment_stmt_count, 1)
    }

    pub fn count_in_block(mut self, block_id: BlockId) -> usize {
        self.visit_block(block_id);
        self.assignment_stmt_count
    }
}

impl<'a> Visitor<'a> for AssignmentStmtCounter<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.get_stmt(id)
    }

    fn visit_expr(&mut self, expr_id: ExprId) {
        let expr = self.get_expr(expr_id);
        if matches!(
            expr.kind,
            ExprKind::Assign(_, _)
                | ExprKind::AssignField(_, _, _)
                | ExprKind::AssignIndex(_, _, _)
                | ExprKind::AssignOp(_, _, _)
        ) {
            self.assignment_expr_count += 1;
        }
        walk_expr(self, expr_id);
    }

    fn visit_stmt(&mut self, stmt_id: StmtId) {
        let initial_assigment_expr_count = self.assignment_expr_count;
        walk_stmt(self, stmt_id);
        if self.assignment_expr_count > initial_assigment_expr_count {
            self.assignment_stmt_count += 1;
        }
    }
}

pub fn set_indentation<'a, 'b>(
    indent: Indented<'a, Formatter<'b>>,
    level: usize,
) -> Indented<'a, Formatter<'b>> {
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        _ => unimplemented!("intentation level not supported"),
    }
}
