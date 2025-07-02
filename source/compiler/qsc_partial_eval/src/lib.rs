// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The Q# partial evaluator residualizes a Q# program, producing RIR from FIR.
//! It does this by evaluating all purely classical expressions and generating RIR instructions for expressions that are
//! not purely classical.

#[cfg(test)]
mod tests;

mod evaluation_context;
mod management;

use core::panic;
use evaluation_context::{Arg, BlockNode, EvalControlFlow, EvaluationContext, Scope};
use management::{QuantumIntrinsicsChecker, ResourceManager};
use miette::Diagnostic;
use qsc_data_structures::{functors::FunctorApp, span::Span, target::TargetCapabilityFlags};
use qsc_eval::{
    self, Error as EvalError, ErrorBehavior, PackageSpan, State, StepAction, StepResult, Variable,
    are_ctls_unique, exec_graph_section,
    intrinsic::qubit_relabel,
    output::GenericReceiver,
    resolve_closure,
    val::{
        self, Value, Var, VarTy, index_array, slice_array, update_functor_app, update_index_range,
        update_index_single,
    },
};
use qsc_fir::{
    fir::{
        self, BinOp, Block, BlockId, CallableDecl, CallableImpl, ExecGraph, Expr, ExprId, ExprKind,
        Global, Ident, LocalVarId, Mutability, PackageId, PackageStore, PackageStoreLookup, Pat,
        PatId, PatKind, Res, SpecDecl, SpecImpl, Stmt, StmtId, StmtKind, StoreBlockId, StoreExprId,
        StoreItemId, StorePatId, StoreStmtId, UnOp,
    },
    ty::{Prim, Ty},
};
use qsc_lowerer::map_fir_package_to_hir;
use qsc_rca::{
    ComputeKind, ComputePropertiesLookup, ItemComputeProperties, PackageStoreComputeProperties,
    QuantumProperties, RuntimeFeatureFlags,
    errors::{
        Error as CapabilityError, generate_errors_from_runtime_features,
        get_missing_runtime_features,
    },
};
use qsc_rir::{
    builder,
    rir::{
        self, Callable, CallableId, CallableType, ConditionCode, FcmpConditionCode, Instruction,
        Literal, Operand, Program, VariableId,
    },
};
use rustc_hash::FxHashMap;
use std::{collections::hash_map::Entry, rc::Rc, result::Result};
use thiserror::Error;

/// Partially evaluates a program with the specified entry expression.
pub fn partially_evaluate(
    package_store: &PackageStore,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
    capabilities: TargetCapabilityFlags,
) -> Result<Program, Error> {
    let partial_evaluator =
        PartialEvaluator::new(package_store, compute_properties, entry, capabilities);
    partial_evaluator.eval()
}

/// Partially evaluates a callable with the specified arguments.
pub fn partially_evaluate_call(
    package_store: &PackageStore,
    compute_properties: &PackageStoreComputeProperties,
    callable: StoreItemId,
    args: Value,
    capabilities: TargetCapabilityFlags,
) -> Result<Program, Error> {
    let partial_evaluator = PartialEvaluator::new_from_package_id(
        package_store,
        compute_properties,
        callable.package,
        capabilities,
    );
    partial_evaluator.invoke(callable, args)
}

/// A partial evaluation error.
#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    CapabilityError(CapabilityError),

    #[error("cannot use a dynamic value returned from a runtime-resolved callable")]
    #[diagnostic(code("Qsc.PartialEval.UnexpectedDynamicValue"))]
    #[diagnostic(help("try invoking the desired callable directly"))]
    UnexpectedDynamicValue(#[label] PackageSpan),

    #[error("cannot use a dynamic value of type `{0}` returned from intrinsic callable")]
    #[diagnostic(code("Qsc.PartialEval.UnexpectedDynamicIntrsinicReturnType"))]
    UnexpectedDynamicIntrinsicReturnType(String, #[label] PackageSpan),

    #[error("partial evaluation failed with error: {0}")]
    #[diagnostic(code("Qsc.PartialEval.EvaluationFailed"))]
    EvaluationFailed(String, #[label] PackageSpan),

    #[error("unsupported Result literal in output")]
    #[diagnostic(help(
        "Result literals `One` and `Zero` cannot be included in generated QIR output recording."
    ))]
    #[diagnostic(code("Qsc.PartialEval.OutputResultLiteral"))]
    OutputResultLiteral(#[label] PackageSpan),

    #[error("an unexpected error occurred related to: {0}")]
    #[diagnostic(code("Qsc.PartialEval.Unexpected"))]
    #[diagnostic(help(
        "this is probably a bug, please consider reporting this as an issue to the development team"
    ))]
    Unexpected(String, #[label] PackageSpan),

    #[error("failed to evaluate: {0} is not supported")]
    #[diagnostic(code("Qsc.PartialEval.Unimplemented"))]
    Unimplemented(String, #[label] PackageSpan),

    #[error("unsupported call into test callable")]
    #[diagnostic(code("Qsc.PartialEval.UnsupportedTestCallable"))]
    #[diagnostic(help(
        "callables with the `@Test` annotation should not be called from non-test code."
    ))]
    UnsupportedTestCallable(#[label] PackageSpan),

    #[error("unsupported use of simulation-only intrinsic `{0}`")]
    #[diagnostic(code("Qsc.PartialEval.UnsupportedSimulationIntrinsic"))]
    UnsupportedSimulationIntrinsic(String, #[label] PackageSpan),
}

impl From<EvalError> for Error {
    fn from(e: EvalError) -> Self {
        Error::EvaluationFailed(e.to_string(), *e.span())
    }
}

impl Error {
    #[must_use]
    pub fn span(&self) -> Option<PackageSpan> {
        match self {
            Self::CapabilityError(_) => None,
            Self::UnexpectedDynamicValue(span)
            | Self::UnexpectedDynamicIntrinsicReturnType(_, span)
            | Self::EvaluationFailed(_, span)
            | Self::OutputResultLiteral(span)
            | Self::Unexpected(_, span)
            | Self::Unimplemented(_, span)
            | Self::UnsupportedTestCallable(span)
            | Self::UnsupportedSimulationIntrinsic(_, span) => Some(*span),
        }
    }
}

/// An entry to the program to be partially evaluated.
pub struct ProgramEntry {
    /// The execution graph that corresponds to the entry expression.
    pub exec_graph: ExecGraph,
    /// The entry expression unique identifier within a package store.
    pub expr: fir::StoreExprId,
}

struct PartialEvaluator<'a> {
    package_store: &'a PackageStore,
    compute_properties: &'a PackageStoreComputeProperties,
    resource_manager: ResourceManager,
    backend: QuantumIntrinsicsChecker,
    callables_map: FxHashMap<Rc<str>, CallableId>,
    eval_context: EvaluationContext,
    program: Program,
    entry: Option<&'a ProgramEntry>,
}

impl<'a> PartialEvaluator<'a> {
    fn new(
        package_store: &'a PackageStore,
        compute_properties: &'a PackageStoreComputeProperties,
        entry: &'a ProgramEntry,
        capabilities: TargetCapabilityFlags,
    ) -> Self {
        Self::new_internal(
            package_store,
            compute_properties,
            capabilities,
            Some(entry),
            None,
        )
    }

    fn new_from_package_id(
        package_store: &'a PackageStore,
        compute_properties: &'a PackageStoreComputeProperties,
        package_id: PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Self {
        Self::new_internal(
            package_store,
            compute_properties,
            capabilities,
            None,
            Some(package_id),
        )
    }

    fn new_internal(
        package_store: &'a PackageStore,
        compute_properties: &'a PackageStoreComputeProperties,
        capabilities: TargetCapabilityFlags,
        entry: Option<&'a ProgramEntry>,
        package_id: Option<PackageId>,
    ) -> Self {
        // Create the entry-point callable.
        let mut resource_manager = ResourceManager::default();
        let mut program = Program::new();
        program.config.capabilities = capabilities;
        let entry_block_id = resource_manager.next_block();
        program.blocks.insert(entry_block_id, rir::Block::default());
        let entry_point_id = resource_manager.next_callable();
        let entry_point = rir::Callable {
            name: "main".into(),
            input_type: Vec::new(),
            output_type: Some(rir::Ty::Integer),
            body: Some(entry_block_id),
            call_type: CallableType::Regular,
        };
        program.callables.insert(entry_point_id, entry_point);
        program.entry = entry_point_id;

        // Initialize the evaluation context and create a new partial evaluator.
        let context = EvaluationContext::new(
            package_id.unwrap_or_else(|| {
                entry
                    .expect("program entry should be provided when package id is None")
                    .expr
                    .package
            }),
            entry_block_id,
        );
        Self {
            package_store,
            compute_properties,
            eval_context: context,
            resource_manager,
            backend: QuantumIntrinsicsChecker::default(),
            callables_map: FxHashMap::default(),
            program,
            entry,
        }
    }

    fn bind_value_to_pat(&mut self, mutability: Mutability, pat_id: PatId, value: Value) {
        let pat = self.get_pat(pat_id);
        match &pat.kind {
            PatKind::Bind(ident) => {
                self.bind_value_to_ident(mutability, ident, value);
            }
            PatKind::Tuple(pats) => {
                let tuple = value.unwrap_tuple();
                assert!(pats.len() == tuple.len());
                for (pat_id, value) in pats.iter().zip(tuple.iter()) {
                    self.bind_value_to_pat(mutability, *pat_id, value.clone());
                }
            }
            PatKind::Discard => {
                // Nothing to bind to.
            }
        }
    }

    fn bind_value_to_ident(&mut self, mutability: Mutability, ident: &Ident, value: Value) {
        // We do slightly different things depending on the mutability of the identifier.
        match mutability {
            Mutability::Mutable => self.bind_value_to_mutable_ident(ident, value),
            Mutability::Immutable => {
                let current_scope = self.eval_context.get_current_scope();
                if matches!(value, Value::Var(var) if current_scope.get_static_value(var.id.into()).is_none())
                {
                    // An immutable identifier is being bound to a dynamic value, so treat the identifier as mutable.
                    // This allows it to represent a point-in-time copy of the mutable value during evaluation.
                    self.bind_value_to_mutable_ident(ident, value);
                } else {
                    // The value is static, so bind it to the classical map.
                    self.bind_value_to_immutable_ident(ident, value);
                }
            }
        }
    }

    fn bind_value_to_immutable_ident(&mut self, ident: &Ident, value: Value) {
        // If the value is not a variable, bind it to the classical map.
        if !matches!(value, Value::Var(_)) {
            self.bind_value_in_classical_map(ident, &value);
        }

        // Always bind the value to the hybrid map.
        self.bind_value_in_hybrid_map(ident, value);
    }

    fn bind_value_to_mutable_ident(&mut self, ident: &Ident, value: Value) {
        // If the value is not a variable, bind it to the classical map.
        if !matches!(value, Value::Var(_)) {
            self.bind_value_in_classical_map(ident, &value);
        }

        // Always bind the value to the hybrid map but do it differently depending of the value type.
        if let Some((var_id, literal)) = self.try_create_mutable_variable(ident.id, &value) {
            // If the variable maps to a know static literal, track that mapping.
            if let Some(literal) = literal {
                self.eval_context
                    .get_current_scope_mut()
                    .insert_static_var_mapping(var_id, literal);
            }
        } else {
            self.bind_value_in_hybrid_map(ident, value);
        }
    }

    fn bind_value_in_classical_map(&mut self, ident: &Ident, value: &Value) {
        // Create a variable and bind it to the classical environment.
        let var = Variable {
            name: ident.name.clone(),
            value: value.clone(),
            span: ident.span,
        };
        let scope = self.eval_context.get_current_scope_mut();
        scope.env.bind_variable_in_top_frame(ident.id, var);
    }

    fn bind_value_in_hybrid_map(&mut self, ident: &Ident, value: Value) {
        // Insert the value into the hybrid vars map.
        self.eval_context
            .get_current_scope_mut()
            .insert_hybrid_local_value(ident.id, value);
    }

    fn create_intrinsic_callable(
        &self,
        store_item_id: StoreItemId,
        callable_decl: &CallableDecl,
        call_type: CallableType,
    ) -> Callable {
        let callable_package = self.package_store.get(store_item_id.package);
        let name = callable_decl.name.name.to_string();
        let input_type: Vec<rir::Ty> = callable_package
            .derive_callable_input_params(callable_decl)
            .iter()
            .map(|input_param| map_fir_type_to_rir_type(&input_param.ty))
            .collect();
        let output_type = if callable_decl.output == Ty::UNIT {
            None
        } else {
            Some(map_fir_type_to_rir_type(&callable_decl.output))
        };
        let body = None;
        let call_type = if name.eq("__quantum__qis__reset__body") {
            CallableType::Reset
        } else {
            call_type
        };
        Callable {
            name,
            input_type,
            output_type,
            body,
            call_type,
        }
    }

    fn create_program_block(&mut self) -> rir::BlockId {
        let block_id = self.resource_manager.next_block();
        self.program.blocks.insert(block_id, rir::Block::default());
        block_id
    }

    fn entry_expr_output_span(&self) -> PackageSpan {
        let expr = self.get_expr(
            self.entry
                .expect("should have entry when getting entry expr span")
                .expr
                .expr,
        );
        let local_span = match &expr.kind {
            // Special handling for compiler generated entry expressions that come from the `@EntryPoint`
            // attributed callable.
            ExprKind::Call(callee, _) if expr.span == Span::default() => {
                self.get_expr(*callee).span
            }
            _ => expr.span,
        };
        let hir_package_id = map_fir_package_to_hir(
            self.entry
                .expect("should have entry when getting entry expr span")
                .expr
                .package,
        );
        PackageSpan {
            package: hir_package_id,
            span: local_span,
        }
    }

    fn extract_program(
        mut self,
        ret_val: Value,
        output_ty: &Ty,
        output_span: PackageSpan,
    ) -> Result<Program, Error> {
        let output_recording: Vec<Instruction> = self
            .generate_output_recording_instructions(ret_val, output_ty)
            .map_err(|()| Error::OutputResultLiteral(output_span))?;

        // Insert the return expression and return the generated program.
        let current_block = self.get_current_rir_block_mut();
        current_block.0.extend(output_recording);
        current_block.0.push(Instruction::Return);

        // Set the number of qubits and results used by the program.
        self.program.num_qubits = self
            .resource_manager
            .qubit_count()
            .try_into()
            .expect("qubits count should fit into a u32");
        self.program.num_results = self
            .resource_manager
            .result_register_count()
            .try_into()
            .expect("results count should fit into a u32");

        Ok(self.program)
    }

    fn eval(mut self) -> Result<Program, Error> {
        // Evaluate the entry-point expression.
        let ret_val = self
            .try_eval_expr(
                self.entry
                    .expect("should have program entry on call to eval")
                    .expr
                    .expr,
            )?
            .into_value();
        let output_ty = &self
            .get_expr(
                self.entry
                    .expect("should have program entry on call to eval")
                    .expr
                    .expr,
            )
            .ty;
        let output_span = self.entry_expr_output_span();
        self.extract_program(ret_val, output_ty, output_span)
    }

    fn invoke(mut self, callable: StoreItemId, args: Value) -> Result<Program, Error> {
        // Evaluate the callalbe.
        let ret_val = self.eval_global_call(callable, args)?.into_value();
        let global = self
            .package_store
            .get_global(callable)
            .expect("global not present");
        let Global::Callable(callable_decl) = global else {
            // Instruction generation for UDTs is not supported.
            panic!("global is not a callable");
        };
        let output_ty = &callable_decl.output;
        self.extract_program(
            ret_val,
            output_ty,
            PackageSpan {
                package: map_fir_package_to_hir(callable.package),
                span: callable_decl.span,
            },
        )
    }

    fn eval_array_update_index(
        &mut self,
        array: &[Value],
        index_expr_id: ExprId,
        update_expr_id: ExprId,
    ) -> Result<Value, Error> {
        // Try to evaluate the index and update expressions to get their value, short-circuiting execution if any of the
        // expressions is a return.
        let index_expr_package_span = self.get_expr_package_span(index_expr_id);
        let index_control_flow = self.try_eval_expr(index_expr_id)?;
        let EvalControlFlow::Continue(index_value) = index_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in index expression".to_string(),
                index_expr_package_span,
            ));
        };
        let update_control_flow = self.try_eval_expr(update_expr_id)?;
        let EvalControlFlow::Continue(update_value) = update_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in update expression".to_string(),
                self.get_expr_package_span(update_expr_id),
            ));
        };

        // Set the value at the specified index or range.
        let update_result = match index_value {
            Value::Int(index) => {
                update_index_single(array, index, update_value, index_expr_package_span)
            }
            Value::Range(range) => update_index_range(
                array,
                range.start,
                range.step,
                range.end,
                update_value,
                index_expr_package_span,
            ),
            _ => panic!("invalid kind of value for index"),
        };
        let updated_array = update_result.map_err(Error::from)?;
        Ok(updated_array)
    }

    fn eval_bin_op(
        &mut self,
        bin_op: BinOp,
        lhs_value: Value,
        rhs_expr_id: ExprId,
        lhs_span: PackageSpan,         // For diagnostic purposes only.
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        // Evaluate the binary operation differently depending on the LHS value variant.
        match lhs_value {
            Value::Array(lhs_array) => self.eval_bin_op_with_lhs_array_operand(
                bin_op,
                &lhs_array,
                rhs_expr_id,
                bin_op_expr_span,
            ),
            Value::Result(lhs_result) => self.eval_bin_op_with_lhs_result_operand(
                bin_op,
                lhs_result,
                rhs_expr_id,
                bin_op_expr_span,
            ),
            Value::Bool(lhs_bool) => {
                self.eval_bin_op_with_lhs_classical_bool_operand(bin_op, lhs_bool, rhs_expr_id)
            }
            Value::Int(lhs_int) => {
                let lhs_operand = Operand::Literal(Literal::Integer(lhs_int));
                self.eval_bin_op_with_lhs_integer_operand(
                    bin_op,
                    lhs_operand,
                    rhs_expr_id,
                    bin_op_expr_span,
                )
            }
            Value::Double(lhs_double) => {
                let lhs_operand = Operand::Literal(Literal::Double(lhs_double));
                self.eval_bin_op_with_lhs_double_operand(
                    bin_op,
                    lhs_operand,
                    rhs_expr_id,
                    bin_op_expr_span,
                )
            }
            Value::Var(lhs_eval_var) => {
                self.eval_bin_op_with_lhs_var(bin_op, lhs_eval_var, rhs_expr_id, bin_op_expr_span)
            }
            _ => Err(Error::Unexpected(
                format!("unsupported LHS value: {lhs_value}"),
                lhs_span,
            )),
        }
    }

    fn eval_bin_op_with_lhs_array_operand(
        &mut self,
        bin_op: BinOp,
        lhs_array: &Rc<Vec<Value>>,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        // Check that the binary operation is currently supported.
        if matches!(bin_op, BinOp::Eq | BinOp::Neq) {
            return Err(Error::Unimplemented(
                "array comparison".to_string(),
                bin_op_expr_span,
            ));
        }

        // The only possible binary operation with array operands at this point is addition.
        assert!(
            matches!(bin_op, BinOp::Add),
            "expected array addition operation, got {bin_op:?}"
        );

        // Try to evaluate the RHS array expression to get its value.
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let Value::Array(rhs_array) = rhs_value else {
            panic!("expected array value from RHS expression");
        };

        // Concatenate the arrays.
        let concatenated_array: Vec<Value> =
            lhs_array.iter().chain(rhs_array.iter()).cloned().collect();
        let array_value = Value::Array(concatenated_array.into());
        Ok(EvalControlFlow::Continue(array_value))
    }

    fn eval_bin_op_with_lhs_result_operand(
        &mut self,
        bin_op: BinOp,
        lhs_result: val::Result,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let Value::Result(rhs_result) = rhs_value else {
            panic!("expected result value from RHS expression");
        };

        // Even though to get to this path, an expression would have to be categorized as hybrid by RCA, it is
        // possible that the expression is in fact purely classical.
        // This can happen in cases where a data structure such an array, tuple or UDT contains a mix of static and
        // dynamic values. In such instances, RCA identifies all the contents of the data structure as dynamic even if
        // some values are static.
        // Here we handle this case and if both operands are purely classical we evaluate them.
        if let (val::Result::Val(lhs_result_value), val::Result::Val(rhs_result_value)) =
            (lhs_result, rhs_result)
        {
            let bool_value = match bin_op {
                BinOp::Eq => lhs_result_value == rhs_result_value,
                BinOp::Neq => lhs_result_value != rhs_result_value,
                _ => {
                    return Err(Error::Unexpected(
                        format!("invalid binary operator for Result operands: {bin_op:?})"),
                        bin_op_expr_span,
                    ));
                }
            };
            return Ok(EvalControlFlow::Continue(Value::Bool(bool_value)));
        }

        // Get the operands to use when generating the binary operation instruction.
        let lhs_operand = self.eval_result_as_bool_operand(lhs_result);
        let rhs_operand = self.eval_result_as_bool_operand(rhs_result);

        // Create a variable to store the result of the expression.
        let variable_id = self.resource_manager.next_var();
        let rir_variable = rir::Variable {
            variable_id,
            ty: rir::Ty::Boolean, // Binary operations between results are always Boolean.
        };

        // Create the binary operation instruction and add it to the current block.
        let condition_code = match bin_op {
            BinOp::Eq => ConditionCode::Eq,
            BinOp::Neq => ConditionCode::Ne,
            _ => {
                return Err(Error::Unexpected(
                    format!("invalid binary operator for Result operands: {bin_op:?})"),
                    bin_op_expr_span,
                ));
            }
        };

        let instruction = match (bin_op, lhs_operand, rhs_operand) {
            (BinOp::Eq, Operand::Literal(Literal::Bool(true)), operand)
            | (BinOp::Eq, operand, Operand::Literal(Literal::Bool(true)))
            | (BinOp::Neq, Operand::Literal(Literal::Bool(false)), operand)
            | (BinOp::Neq, operand, Operand::Literal(Literal::Bool(false))) => {
                // One of the operands is a literal so we just need a store instruction.
                Instruction::Store(operand, rir_variable)
            }
            // Both operators are non-literals so we need the comparison instruction.
            _ => Instruction::Icmp(condition_code, lhs_operand, rhs_operand, rir_variable),
        };
        self.get_current_rir_block_mut().0.push(instruction);

        // Return the variable as a value.
        let value = Value::Var(map_rir_var_to_eval_var(rir_variable).map_err(|()| {
            Error::Unexpected(
                format!("{} type in binop", rir_variable.ty),
                bin_op_expr_span,
            )
        })?);
        Ok(EvalControlFlow::Continue(value))
    }

    fn eval_bin_op_with_lhs_classical_bool_operand(
        &mut self,
        bin_op: BinOp,
        lhs_bool: bool,
        rhs_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        let value = match (bin_op, lhs_bool) {
            // Handle short-circuiting for logical AND and logical OR.
            (BinOp::AndL, false) => Value::Bool(false),
            (BinOp::OrL, true) => Value::Bool(true),
            // Cases for which just returning the RHS value is sufficient.
            (BinOp::AndL | BinOp::Eq, true) | (BinOp::OrL | BinOp::Neq, false) => {
                // Try to evaluate the RHS expression to get its value.
                let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
                let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
                    return Err(Error::Unexpected(
                        "embedded return in RHS expression".to_string(),
                        self.get_expr_package_span(rhs_expr_id),
                    ));
                };
                rhs_value
            }
            // The other possible cases.
            (BinOp::Eq | BinOp::Neq, _) => {
                // Try to evaluate the RHS expression to get its value.
                let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
                let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
                    return Err(Error::Unexpected(
                        "embedded return in RHS expression".to_string(),
                        self.get_expr_package_span(rhs_expr_id),
                    ));
                };

                // Create the operands.
                let lhs_operand = Operand::Literal(Literal::Bool(lhs_bool));
                let rhs_operand = self.map_eval_value_to_rir_operand(&rhs_value);

                // If both operands are literals, evaluate the binary operation and return its value.
                if let (Operand::Literal(lhs_literal), Operand::Literal(rhs_literal)) =
                    (lhs_operand, rhs_operand)
                {
                    let value = eval_bin_op_with_bool_literals(bin_op, lhs_literal, rhs_literal);
                    return Ok(EvalControlFlow::Continue(value));
                }

                // Generate the specific instruction depending on the operand.
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable {
                    variable_id: bin_op_variable_id,
                    ty: rir::Ty::Boolean,
                };
                let bin_op_ins = match bin_op {
                    BinOp::AndL => {
                        Instruction::LogicalAnd(lhs_operand, rhs_operand, bin_op_rir_variable)
                    }
                    BinOp::OrL => {
                        Instruction::LogicalOr(lhs_operand, rhs_operand, bin_op_rir_variable)
                    }
                    BinOp::Eq => Instruction::Icmp(
                        ConditionCode::Eq,
                        lhs_operand,
                        rhs_operand,
                        bin_op_rir_variable,
                    ),
                    BinOp::Neq => Instruction::Icmp(
                        ConditionCode::Ne,
                        lhs_operand,
                        rhs_operand,
                        bin_op_rir_variable,
                    ),
                    _ => panic!("unsupported binary operation for bools: {bin_op:?}"),
                };
                self.get_current_rir_block_mut().0.push(bin_op_ins);
                Value::Var(map_rir_var_to_eval_var(bin_op_rir_variable).map_err(|()| {
                    Error::Unexpected(
                        format!("{} type in binop", bin_op_rir_variable.ty),
                        self.get_expr_package_span(rhs_expr_id),
                    )
                })?)
            }
            _ => panic!("unsupported binary operation for bools: {bin_op:?}"),
        };
        Ok(EvalControlFlow::Continue(value))
    }

    fn eval_bin_op_with_lhs_dynamic_bool_operand(
        &mut self,
        bin_op: BinOp,
        lhs_eval_var: Var,
        rhs_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        let result_var = match bin_op {
            BinOp::Eq | BinOp::Neq => {
                self.eval_comparison_bool_bin_op(bin_op, lhs_eval_var, rhs_expr_id)?
            }
            BinOp::AndL => {
                // Logical AND Boolean operations short-circuit on false.
                let lhs_rir_var = map_eval_var_to_rir_var(lhs_eval_var);
                self.eval_logical_bool_bin_op(false, lhs_rir_var, rhs_expr_id)?
            }
            BinOp::OrL => {
                // Logical OR Boolean operations short-circuit on true.
                let lhs_rir_var = map_eval_var_to_rir_var(lhs_eval_var);
                self.eval_logical_bool_bin_op(true, lhs_rir_var, rhs_expr_id)?
            }
            _ => panic!("invalid Boolean operator {bin_op:?}"),
        };
        Ok(EvalControlFlow::Continue(Value::Var(result_var)))
    }

    fn eval_comparison_bool_bin_op(
        &mut self,
        bin_op: BinOp,
        lhs_eval_var: Var,
        rhs_expr_id: ExprId,
    ) -> Result<Var, Error> {
        // Try to evaluate the RHS expression to get its value and create a RHS operand.
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let rhs_operand = self.map_eval_value_to_rir_operand(&rhs_value);

        // Get the comparison result depending on the operator and the RHS value.
        let result_var = match (bin_op, rhs_operand) {
            // If the RHS value is a literal, depending on the operand, the result of the Boolean comparison is just the
            // LHS value.
            (BinOp::Neq, Operand::Literal(Literal::Bool(false)))
            | (BinOp::Eq, Operand::Literal(Literal::Bool(true))) => lhs_eval_var,
            // In other cases we have to actually generate the comparison instruction.
            (BinOp::Eq | BinOp::Neq, _) => {
                let rir_variable = rir::Variable::new_boolean(self.resource_manager.next_var());
                let lhs_operand = Operand::Variable(map_eval_var_to_rir_var(lhs_eval_var));
                let condition_code = match bin_op {
                    BinOp::Eq => ConditionCode::Eq,
                    BinOp::Neq => ConditionCode::Ne,
                    _ => panic!("invalid Boolean comparison operator {bin_op:?}"),
                };
                let cmp_inst =
                    Instruction::Icmp(condition_code, lhs_operand, rhs_operand, rir_variable);
                self.get_current_rir_block_mut().0.push(cmp_inst);
                map_rir_var_to_eval_var(rir_variable).map_err(|()| {
                    Error::Unexpected(
                        format!("{} type in comparison binop", rir_variable.ty),
                        self.get_expr_package_span(rhs_expr_id),
                    )
                })?
            }
            (_, _) => panic!("invalid Boolean comparison operator {bin_op:?}"),
        };
        Ok(result_var)
    }

    fn eval_logical_bool_bin_op(
        &mut self,
        short_circuit_on_true: bool,
        lhs_rir_var: rir::Variable,
        rhs_expr_id: ExprId,
    ) -> Result<Var, Error> {
        // Create the variable where we will store the result of the Boolean operation and store a default value in it,
        // which will only be changed inside the conditional block where the RHS expression is evaluated.
        let result_var_id = self.resource_manager.next_var();
        let result_rir_var = rir::Variable {
            variable_id: result_var_id,
            ty: rir::Ty::Boolean,
        };
        let init_var_ins = Instruction::Store(
            Operand::Literal(Literal::Bool(short_circuit_on_true)),
            result_rir_var,
        );
        self.get_current_rir_block_mut().0.push(init_var_ins);

        // Pop the current block and insert the continuation block.
        let current_block_node = self.eval_context.pop_block_node();
        let continuation_block_id = self.create_program_block();
        let continuation_block_node = BlockNode {
            id: continuation_block_id,
            successor: current_block_node.successor,
        };
        self.eval_context.push_block_node(continuation_block_node);

        // Now insert the conditional block.
        let rhs_eval_block_id = self.create_program_block();
        let rhs_eval_block_node = BlockNode {
            id: rhs_eval_block_id,
            successor: Some(continuation_block_id),
        };
        self.eval_context.push_block_node(rhs_eval_block_node);

        // Evaluate the RHS expression
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let rhs_operand = self.map_eval_value_to_rir_operand(&rhs_value);

        // Store the RHS value into the the variable that represents the result of the Boolean operation.
        let store_ins = Instruction::Store(rhs_operand, result_rir_var);
        self.get_current_rir_block_mut().0.push(store_ins);
        let jump_ins = Instruction::Jump(continuation_block_id);
        self.get_current_rir_block_mut().0.push(jump_ins);
        let _ = self.eval_context.pop_block_node();

        // Now that we have constructed both the conditional and continuation blocks, insert the jump instruction and
        // return the variable that stores the result of the Boolean operation.
        // The branching blocks depend on whether we short-circuit on true or false.
        let (true_block_id, false_block_id) = if short_circuit_on_true {
            (continuation_block_id, rhs_eval_block_id)
        } else {
            (rhs_eval_block_id, continuation_block_id)
        };
        let branch_ins = Instruction::Branch(lhs_rir_var, true_block_id, false_block_id);
        self.get_program_block_mut(current_block_node.id)
            .0
            .push(branch_ins);
        let result_eval_var = map_rir_var_to_eval_var(result_rir_var).map_err(|()| {
            Error::Unexpected(
                format!("{} type in logical binop", result_rir_var.ty),
                self.get_expr_package_span(rhs_expr_id),
            )
        })?;
        Ok(result_eval_var)
    }

    fn eval_bin_op_with_lhs_double_operand(
        &mut self,
        bin_op: BinOp,
        lhs_operand: Operand,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        assert!(
            matches!(lhs_operand.get_type(), rir::Ty::Double),
            "LHS is expected to be of double type"
        );

        // Try to evaluate the RHS expression to get its value and construct its operand.
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let rhs_operand = self.map_eval_value_to_rir_operand(&rhs_value);
        assert!(
            matches!(rhs_operand.get_type(), rir::Ty::Double),
            "LHS value is expected to be of double type"
        );

        // If both operands are literals, evaluate the binary operation and return its value.
        if let (Operand::Literal(lhs_literal), Operand::Literal(rhs_literal)) =
            (lhs_operand, rhs_operand)
        {
            let value = eval_bin_op_with_double_literals(
                bin_op,
                lhs_literal,
                rhs_literal,
                bin_op_expr_span,
            )?;
            return Ok(EvalControlFlow::Continue(value));
        }

        // Generate the instructions.
        let bin_op_rir_variable = self
            .generate_instructions_for_binary_operation_with_double_operands(
                bin_op,
                lhs_operand,
                rhs_operand,
                bin_op_expr_span,
            )?;
        let value = Value::Var(map_rir_var_to_eval_var(bin_op_rir_variable).map_err(|()| {
            Error::Unexpected(
                format!("{} type in binop", bin_op_rir_variable.ty),
                bin_op_expr_span,
            )
        })?);
        Ok(EvalControlFlow::Continue(value))
    }

    fn eval_bin_op_with_lhs_integer_operand(
        &mut self,
        bin_op: BinOp,
        lhs_operand: Operand,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        assert!(
            matches!(lhs_operand.get_type(), rir::Ty::Integer),
            "LHS is expected to be of integer type"
        );

        // Try to evaluate the RHS expression to get its value and construct its operand.
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in RHS expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };
        let rhs_operand = self.map_eval_value_to_rir_operand(&rhs_value);
        assert!(
            matches!(rhs_operand.get_type(), rir::Ty::Integer),
            "LHS value is expected to be of integer type"
        );

        // If both operands are literals, evaluate the binary operation and return its value.
        if let (Operand::Literal(lhs_literal), Operand::Literal(rhs_literal)) =
            (lhs_operand, rhs_operand)
        {
            let value = eval_bin_op_with_integer_literals(
                bin_op,
                lhs_literal,
                rhs_literal,
                bin_op_expr_span,
            )?;
            return Ok(EvalControlFlow::Continue(value));
        }

        // Generate the instructions.
        let bin_op_rir_variable = self
            .generate_instructions_for_binary_operation_with_integer_operands(
                bin_op,
                lhs_operand,
                rhs_operand,
                bin_op_expr_span,
            )?;
        let value = Value::Var(map_rir_var_to_eval_var(bin_op_rir_variable).map_err(|()| {
            Error::Unexpected(
                format!("{} type in binop", bin_op_rir_variable.ty),
                bin_op_expr_span,
            )
        })?);
        Ok(EvalControlFlow::Continue(value))
    }

    fn eval_bin_op_with_lhs_var(
        &mut self,
        bin_op: BinOp,
        lhs_eval_var: Var,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        match lhs_eval_var.ty {
            VarTy::Boolean => {
                self.eval_bin_op_with_lhs_dynamic_bool_operand(bin_op, lhs_eval_var, rhs_expr_id)
            }
            VarTy::Integer => {
                let lhs_rir_var = map_eval_var_to_rir_var(lhs_eval_var);
                let lhs_operand = Operand::Variable(lhs_rir_var);
                self.eval_bin_op_with_lhs_integer_operand(
                    bin_op,
                    lhs_operand,
                    rhs_expr_id,
                    bin_op_expr_span,
                )
            }
            VarTy::Double => {
                let lhs_rir_var = map_eval_var_to_rir_var(lhs_eval_var);
                let lhs_operand = Operand::Variable(lhs_rir_var);
                self.eval_bin_op_with_lhs_double_operand(
                    bin_op,
                    lhs_operand,
                    rhs_expr_id,
                    bin_op_expr_span,
                )
            }
        }
    }

    fn eval_classical_expr(&mut self, expr_id: ExprId) -> Result<EvalControlFlow, Error> {
        let current_package_id = self.get_current_package_id();
        let store_expr_id = StoreExprId::from((current_package_id, expr_id));
        let expr = self.package_store.get_expr(store_expr_id);
        let scope_exec_graph = self.get_current_scope_exec_graph().clone();
        let scope = self.eval_context.get_current_scope_mut();
        let exec_graph = exec_graph_section(&scope_exec_graph, expr.exec_graph_range.clone());
        let mut state = State::new(
            current_package_id,
            exec_graph,
            None,
            ErrorBehavior::FailOnError,
        );
        let classical_result = state.eval(
            self.package_store,
            &mut scope.env,
            &mut self.backend,
            &mut GenericReceiver::new(&mut std::io::sink()),
            &[],
            StepAction::Continue,
        );
        let eval_result = match classical_result {
            Ok(step_result) => {
                let StepResult::Return(value) = step_result else {
                    panic!("evaluating a classical expression should always return a value");
                };

                // Figure out the control flow kind.
                let scope = self.eval_context.get_current_scope();
                let eval_control_flow = if scope.has_classical_evaluator_returned() {
                    EvalControlFlow::Return(value)
                } else {
                    EvalControlFlow::Continue(value)
                };
                Ok(eval_control_flow)
            }
            Err((error, _)) => Err(Error::from(error)),
        };

        // If this was an assign expression, update the bindings in the hybrid side to keep them in sync and to insert
        // store instructions for variables of type `Bool`, `Int` or `Double`.
        if let Ok(EvalControlFlow::Continue(_)) = eval_result {
            let expr = self.get_expr(expr_id);
            if let ExprKind::Assign(lhs_expr_id, _)
            | ExprKind::AssignField(lhs_expr_id, _, _)
            | ExprKind::AssignIndex(lhs_expr_id, _, _)
            | ExprKind::AssignOp(_, lhs_expr_id, _) = &expr.kind
            {
                self.update_hybrid_bindings_from_classical_bindings(*lhs_expr_id)?;
            }
        }

        eval_result
    }

    fn eval_hybrid_expr(&mut self, expr_id: ExprId) -> Result<EvalControlFlow, Error> {
        let expr = self.get_expr(expr_id);
        let expr_package_span = self.get_expr_package_span(expr_id);
        match &expr.kind {
            ExprKind::Array(exprs) => self.eval_expr_array(exprs),
            ExprKind::ArrayLit(_) => Err(Error::Unexpected(
                "array literal should have been classically evaluated".to_string(),
                expr_package_span,
            )),
            ExprKind::ArrayRepeat(value_expr_id, size_expr_id) => {
                self.eval_expr_array_repeat(*value_expr_id, *size_expr_id)
            }
            ExprKind::Assign(lhs_expr_id, rhs_expr_id) => {
                self.eval_expr_assign(*lhs_expr_id, *rhs_expr_id)
            }
            ExprKind::AssignField(_, _, _) => Err(Error::Unexpected(
                "assigning a dynamic value to a field of a user-defined type is invalid"
                    .to_string(),
                expr_package_span,
            )),
            ExprKind::AssignIndex(array_expr_id, index_expr_id, replace_expr_id) => {
                self.eval_expr_assign_index(*array_expr_id, *index_expr_id, *replace_expr_id)
            }
            ExprKind::AssignOp(bin_op, lhs_expr_id, rhs_expr_id) => {
                self.eval_expr_assign_op(*bin_op, *lhs_expr_id, *rhs_expr_id, expr_package_span)
            }
            ExprKind::BinOp(bin_op, lhs_expr_id, rhs_expr_id) => {
                self.eval_expr_bin_op(*bin_op, *lhs_expr_id, *rhs_expr_id, expr_package_span)
            }
            ExprKind::Block(block_id) => self.try_eval_block(*block_id),
            ExprKind::Call(callee_expr_id, args_expr_id) => {
                self.eval_expr_call(expr_id, *callee_expr_id, *args_expr_id)
            }
            ExprKind::Closure(args, callable) => {
                let closure = resolve_closure(
                    &self.eval_context.get_current_scope().env,
                    self.get_current_package_id(),
                    expr.span,
                    args,
                    *callable,
                )
                .map_err(Error::from)?;
                Ok(EvalControlFlow::Continue(closure))
            }
            ExprKind::Fail(_) => Err(Error::Unexpected(
                "using a dynamic value in a fail statement is invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::Field(_, _) => Err(Error::Unexpected(
                "accessing a field of a dynamic user-defined type is invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::Hole => Err(Error::Unexpected(
                "hole expressions are not expected during partial evaluation".to_string(),
                expr_package_span,
            )),
            ExprKind::If(condition_expr_id, body_expr_id, otherwise_expr_id) => self.eval_expr_if(
                expr_id,
                *condition_expr_id,
                *body_expr_id,
                *otherwise_expr_id,
            ),
            ExprKind::Index(array_expr_id, index_expr_id) => {
                self.eval_expr_index(*array_expr_id, *index_expr_id)
            }
            ExprKind::Lit(_) => Err(Error::Unexpected(
                "literal should have been classically evaluated".to_string(),
                expr_package_span,
            )),
            ExprKind::Range(_, _, _) => Err(Error::Unexpected(
                "dynamic ranges are invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::Return(expr_id) => self.eval_expr_return(*expr_id),
            ExprKind::Struct(..) => Err(Error::Unexpected(
                "instruction generation for struct constructor expressions is invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::String(_) => Err(Error::Unexpected(
                "dynamic strings are invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::Tuple(exprs) => self.eval_expr_tuple(exprs),
            ExprKind::UnOp(un_op, value_expr_id) => {
                self.eval_expr_unary(*un_op, *value_expr_id, expr_package_span)
            }
            ExprKind::UpdateField(_, _, _) => Err(Error::Unexpected(
                "updating a field of a dynamic user-defined type is invalid".to_string(),
                expr_package_span,
            )),
            ExprKind::UpdateIndex(array_expr_id, index_expr_id, update_expr_id) => {
                self.eval_expr_update_index(*array_expr_id, *index_expr_id, *update_expr_id)
            }
            ExprKind::Var(res, _) => Ok(EvalControlFlow::Continue(self.eval_expr_var(res))),
            ExprKind::While(condition_expr_id, body_block_id) => {
                self.eval_expr_while(*condition_expr_id, *body_block_id)
            }
        }
    }

    fn eval_expr_array_repeat(
        &mut self,
        value_expr_id: ExprId,
        size_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        // Try to evaluate both the value and size expressions to get their value, short-circuiting execution if any of the
        // expressions is a return.
        let value_control_flow = self.try_eval_expr(value_expr_id)?;
        let EvalControlFlow::Continue(value) = value_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in array".to_string(),
                self.get_expr_package_span(value_expr_id),
            ));
        };
        let size_control_flow = self.try_eval_expr(size_expr_id)?;
        let EvalControlFlow::Continue(size) = size_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in array size".to_string(),
                self.get_expr_package_span(size_expr_id),
            ));
        };

        // We assume the size of the array is a classical value because otherwise it would have been rejected before
        // getting to the partial evaluation stage.
        let size = size.unwrap_int();
        let values = vec![value; TryFrom::try_from(size).expect("could not convert size value")];
        Ok(EvalControlFlow::Continue(Value::Array(values.into())))
    }

    fn eval_expr_assign(
        &mut self,
        lhs_expr_id: ExprId,
        rhs_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        let rhs_control_flow = self.try_eval_expr(rhs_expr_id)?;
        let EvalControlFlow::Continue(rhs_value) = rhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in assign expression".to_string(),
                self.get_expr_package_span(rhs_expr_id),
            ));
        };

        self.update_bindings(lhs_expr_id, rhs_value)?;
        Ok(EvalControlFlow::Continue(Value::unit()))
    }

    fn eval_expr_assign_index(
        &mut self,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        update_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        // Get the value of the array to use it as the basis to perform the update.
        let array_expr = self.get_expr(array_expr_id);
        let ExprKind::Var(Res::Local(array_loc_id), _) = &array_expr.kind else {
            panic!("array expression in assign index expression is expected to be a variable");
        };
        let array = self
            .eval_context
            .get_current_scope()
            .get_classical_local_value(*array_loc_id)
            .clone()
            .unwrap_array();

        // Evaluate the updated array and update the corresponding bindings.
        let new_array_value =
            self.eval_array_update_index(&array, index_expr_id, update_expr_id)?;
        self.update_bindings(array_expr_id, new_array_value)?;
        Ok(EvalControlFlow::Continue(Value::unit()))
    }

    fn eval_expr_assign_op(
        &mut self,
        bin_op: BinOp,
        lhs_expr_id: ExprId,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        // Consider optimization of array in-place operations instead of re-using the general binary operation
        // evaluation.
        let lhs_expr = self.get_expr(lhs_expr_id);
        let lhs_expr_package_span = self.get_expr_package_span(lhs_expr_id);
        let lhs_value = if matches!(lhs_expr.ty, Ty::Array(_)) {
            let ExprKind::Var(Res::Local(lhs_loc_id), _) = &lhs_expr.kind else {
                panic!("array expression in assign op expression is expected to be a variable");
            };
            self.eval_context
                .get_current_scope()
                .get_classical_local_value(*lhs_loc_id)
                .clone()
        } else {
            let lhs_control_flow = self.try_eval_expr(lhs_expr_id)?;
            if lhs_control_flow.is_return() {
                return Err(Error::Unexpected(
                    "embedded return in assign op LHS expression".to_string(),
                    lhs_expr_package_span,
                ));
            }
            lhs_control_flow.into_value()
        };
        let bin_op_control_flow = self.eval_bin_op(
            bin_op,
            lhs_value,
            rhs_expr_id,
            lhs_expr_package_span,
            bin_op_expr_span,
        )?;
        let EvalControlFlow::Continue(bin_op_value) = bin_op_control_flow else {
            panic!(
                "evaluating a binary operation is expected to result in an error or a continue, but never in a return"
            );
        };
        self.update_bindings(lhs_expr_id, bin_op_value)?;
        Ok(EvalControlFlow::Continue(Value::unit()))
    }

    #[allow(clippy::similar_names)]
    fn eval_expr_bin_op(
        &mut self,
        bin_op: BinOp,
        lhs_expr_id: ExprId,
        rhs_expr_id: ExprId,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        // Try to evaluate the LHS expression and get its value, short-circuiting execution if it is a return.
        let lhs_control_flow = self.try_eval_expr(lhs_expr_id)?;
        let EvalControlFlow::Continue(lhs_value) = lhs_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in binary operation".to_string(),
                self.get_expr_package_span(lhs_expr_id),
            ));
        };

        // Now that we have a LHS value, evaluate the binary operation, which will properly consider short-circuiting
        // logic in the case of Boolean operations.
        let lhs_span = self.get_expr_package_span(lhs_expr_id);
        self.eval_bin_op(bin_op, lhs_value, rhs_expr_id, lhs_span, bin_op_expr_span)
    }

    fn eval_expr_call(
        &mut self,
        call_expr_id: ExprId,
        callee_expr_id: ExprId,
        args_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        let args_span = self.get_expr_package_span(args_expr_id);
        let (callee_control_flow, args_control_flow) =
            self.try_eval_callee_and_args(callee_expr_id, args_expr_id)?;

        // Get the callable.
        let (store_item_id, functor_app, fixed_args) = match callee_control_flow.into_value() {
            Value::Closure(inner) => (inner.id, inner.functor, Some(inner.fixed_args)),
            Value::Global(id, functor) => (id, functor, None),
            _ => panic!("value is not callable"),
        };
        let global = self
            .package_store
            .get_global(store_item_id)
            .expect("global not present");
        let Global::Callable(callable_decl) = global else {
            // Instruction generation for UDTs is not supported.
            panic!("global is not a callable");
        };

        self.reject_test_callables(callee_expr_id, callable_decl)?;

        // Set up the scope for the call, which allows additional error checking if the callable was
        // previously unresolved.
        let spec_decl = if let CallableImpl::Spec(spec_impl) = &callable_decl.implementation {
            Some(get_spec_decl(spec_impl, functor_app))
        } else {
            None
        };

        let args_value = args_control_flow.into_value();
        let ctls = if let Some(Some(ctls_pat_id)) = spec_decl.map(|spec_decl| spec_decl.input) {
            assert!(
                functor_app.controlled > 0,
                "control qubits count was expected to be greater than zero"
            );
            Some((
                StorePatId::from((store_item_id.package, ctls_pat_id)),
                functor_app.controlled,
            ))
        } else {
            assert!(
                functor_app.controlled == 0,
                "control qubits count was expected to be zero"
            );
            None
        };
        let (args, ctls_arg) = self.resolve_args(
            (store_item_id.package, callable_decl.input).into(),
            args_value.clone(),
            Some(args_span),
            ctls,
            fixed_args,
        )?;
        let call_scope = Scope::new(
            store_item_id.package,
            Some((store_item_id.item, functor_app)),
            args,
            ctls_arg,
        );

        self.check_unresolved_call_capabilities(call_expr_id, callee_expr_id, &call_scope)?;

        // We generate instructions differently depending on whether we are calling an intrinsic or a specialization
        // with an implementation.
        let value = match spec_decl {
            None => {
                let callee_expr_span = self.get_expr_package_span(callee_expr_id);
                self.eval_expr_call_to_intrinsic(
                    store_item_id,
                    callable_decl,
                    args_value,
                    args_span,
                    callee_expr_span,
                )?
            }
            Some(spec_decl) => {
                self.eval_expr_call_to_spec(call_scope, store_item_id, functor_app, spec_decl)?
            }
        };
        Ok(EvalControlFlow::Continue(value))
    }

    fn reject_test_callables(
        &mut self,
        callee_expr_id: ExprId,
        callable_decl: &CallableDecl,
    ) -> Result<(), Error> {
        // If the callable has the test attribute, it's not safe to generate QIR, so we return an error.
        if callable_decl
            .attrs
            .iter()
            .any(|attr| attr == &fir::Attr::Test)
        {
            Err(Error::UnsupportedTestCallable(
                self.get_expr_package_span(callee_expr_id),
            ))
        } else {
            // If the callable is not a test, we can proceed with generating QIR.
            Ok(())
        }
    }

    fn check_unresolved_call_capabilities(
        &mut self,
        call_expr_id: ExprId,
        callee_expr_id: ExprId,
        call_scope: &Scope,
    ) -> Result<(), Error> {
        // If the call has the unresolved flag, it tells us that RCA could not perform static analysis on this call site.
        // Now that we are in evaluation, we have a distinct callable resolved and can perform runtime capability check
        // ahead of performing the actual call and return the appropriate capabilities error if this call is not supported
        // by the target.
        if self.is_unresolved_callee_expr(callee_expr_id) {
            let call_compute_kind = self.get_call_compute_kind(call_scope);
            if let ComputeKind::Quantum(QuantumProperties {
                runtime_features,
                value_kind,
            }) = call_compute_kind
            {
                let missing_features = get_missing_runtime_features(
                    runtime_features,
                    self.program.config.capabilities,
                ) & !RuntimeFeatureFlags::CallToUnresolvedCallee;
                if !missing_features.is_empty() {
                    if let Some(error) = generate_errors_from_runtime_features(
                        missing_features,
                        self.get_expr(call_expr_id).span,
                    )
                    .drain(..)
                    .next()
                    {
                        return Err(Error::CapabilityError(error));
                    }
                }

                // If the call produces a dynamic value, we treat it as an error because we know that later
                // analysis has not taken that dynamism into account and further partial evaluation may fail
                // when it encounters that value.
                if value_kind.is_dynamic() {
                    return Err(Error::UnexpectedDynamicValue(
                        self.get_expr_package_span(call_expr_id),
                    ));
                }
            }
        }
        Ok(())
    }

    fn eval_global_call(
        &mut self,
        store_item_id: StoreItemId,
        args: Value,
    ) -> Result<EvalControlFlow, Error> {
        let global = self
            .package_store
            .get_global(store_item_id)
            .expect("global not present");
        let Global::Callable(callable_decl) = global else {
            // Instruction generation for UDTs is not supported.
            panic!("global is not a callable");
        };

        // Set up the scope for the call, which allows additional error checking if the callable was
        // previously unresolved.
        let spec_decl = if let CallableImpl::Spec(spec_impl) = &callable_decl.implementation {
            get_spec_decl(spec_impl, FunctorApp::default())
        } else {
            panic!("global call to intrinsic function not supported");
        };

        let (args, ctls_arg) = self.resolve_args(
            (store_item_id.package, callable_decl.input).into(),
            args,
            None,
            None,
            None,
        )?;
        let call_scope = Scope::new(
            store_item_id.package,
            Some((store_item_id.item, FunctorApp::default())),
            args,
            ctls_arg,
        );

        // We generate instructions differently depending on whether we are calling an intrinsic or a specialization
        // with an implementation.
        let value = self.eval_expr_call_to_spec(
            call_scope,
            store_item_id,
            FunctorApp::default(),
            spec_decl,
        )?;
        Ok(EvalControlFlow::Continue(value))
    }

    fn try_eval_callee_and_args(
        &mut self,
        callee_expr_id: ExprId,
        args_expr_id: ExprId,
    ) -> Result<(EvalControlFlow, EvalControlFlow), Error> {
        let callee_control_flow = self.try_eval_expr(callee_expr_id)?;
        if callee_control_flow.is_return() {
            return Err(Error::Unexpected(
                "embedded return in callee".to_string(),
                self.get_expr_package_span(callee_expr_id),
            ));
        }
        let args_control_flow = self.try_eval_expr(args_expr_id)?;
        if args_control_flow.is_return() {
            return Err(Error::Unexpected(
                "embedded return in call arguments".to_string(),
                self.get_expr_package_span(args_expr_id),
            ));
        }
        Ok((callee_control_flow, args_control_flow))
    }

    fn eval_expr_call_to_intrinsic(
        &mut self,
        store_item_id: StoreItemId,
        callable_decl: &CallableDecl,
        args_value: Value,
        args_span: PackageSpan,        // For diagnostic purposes only.
        callee_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<Value, Error> {
        // Check if any qubits passed as arguments have been released.
        let qubits = args_value.qubits();
        let qubits_len = qubits.len();
        if qubits_len > 0 {
            let qubits = qubits
                .iter()
                .filter_map(|q| q.try_deref().map(|q| q.0))
                .collect::<Vec<_>>();
            if qubits.len() != qubits_len {
                return if callable_decl.name.name.as_ref() == "__quantum__rt__qubit_release" {
                    Err(EvalError::QubitDoubleRelease(args_span).into())
                } else {
                    Err(EvalError::QubitUsedAfterRelease(args_span).into())
                };
            }
        }

        if callable_decl.attrs.contains(&fir::Attr::Measurement) {
            return Ok(self.measure_qubits(callable_decl, args_value));
        }
        if callable_decl.attrs.contains(&fir::Attr::Reset) {
            return self.eval_expr_call_to_intrinsic_qis(
                store_item_id,
                callable_decl,
                args_value,
                callee_expr_span,
                CallableType::Reset,
            );
        }

        // There are a few special cases regarding intrinsic callables. Identify them and handle them properly.
        match callable_decl.name.name.as_ref() {
            // Qubit allocations and measurements have special handling.
            "__quantum__rt__qubit_allocate" => Ok(self.allocate_qubit()),
            "__quantum__rt__qubit_release" => Ok(self.release_qubit(args_value)),
            "PermuteLabels" => {
                if self.eval_context.is_currently_evaluating_any_branch() {
                    // If we are in a dynamic branch anywhere up the call stack, we cannot support relabel,
                    // as later qubit usage would need to be dynamic on whether the branch was taken.
                    return Err(Error::CapabilityError(CapabilityError::UseOfDynamicQubit(
                        callee_expr_span.span,
                    )));
                }
                qubit_relabel(args_value, args_span, |q0, q1| {
                    self.resource_manager.swap_qubit_ids(q0, q1);
                })
            }
            .map_err(std::convert::Into::into),
            "__quantum__qis__m__body" => Ok(self.measure_qubit(builder::m_decl(), args_value)),
            "__quantum__qis__mresetz__body" => {
                Ok(self.measure_qubit(builder::mresetz_decl(), args_value))
            }
            // The following intrinsic operations and functions are no-ops.
            "BeginEstimateCaching" => Ok(Value::Bool(true)),
            "DumpRegister"
            | "DumpOperation"
            | "AccountForEstimatesInternal"
            | "BeginRepeatEstimatesInternal"
            | "EndRepeatEstimatesInternal"
            | "ApplyIdleNoise"
            | "GlobalPhase" => Ok(Value::unit()),
            "CheckZero" => Err(Error::UnsupportedSimulationIntrinsic(
                "CheckZero".to_string(),
                callee_expr_span,
            )),
            // The following intrinsic functions and operations should never make it past conditional compilation and
            // the capabilities check pass.
            "DrawRandomInt" | "DrawRandomDouble" | "DrawRandomBool" | "Length" => {
                Err(Error::Unexpected(
                    format!(
                        "`{}` is not a supported by partial evaluation",
                        callable_decl.name.name
                    ),
                    callee_expr_span,
                ))
            }
            _ => self.eval_expr_call_to_intrinsic_qis(
                store_item_id,
                callable_decl,
                args_value,
                callee_expr_span,
                CallableType::Regular,
            ),
        }
    }

    fn eval_expr_call_to_intrinsic_qis(
        &mut self,
        store_item_id: StoreItemId,
        callable_decl: &CallableDecl,
        args_value: Value,
        callee_expr_span: PackageSpan,
        call_type: CallableType,
    ) -> Result<Value, Error> {
        // Check if the callable is already in the program, and if not add it.
        let callable = self.create_intrinsic_callable(store_item_id, callable_decl, call_type);
        let output_var = callable.output_type.map(|output_ty| {
            let variable_id = self.resource_manager.next_var();
            rir::Variable {
                variable_id,
                ty: output_ty,
            }
        });

        let callable_id = self.get_or_insert_callable(callable);

        // Resove the call arguments, create the call instruction and insert it to the current block.
        let (args, ctls_arg) = self
            .resolve_args(
                (store_item_id.package, callable_decl.input).into(),
                args_value,
                None,
                None,
                None,
            )
            .expect("no controls to verify");
        assert!(
            ctls_arg.is_none(),
            "intrinsic operations cannot have controls"
        );
        let args_operands = args
            .into_iter()
            .map(|arg| self.map_eval_value_to_rir_operand(&arg.into_value()))
            .collect();

        let instruction = Instruction::Call(callable_id, args_operands, output_var);
        let current_block = self.get_current_rir_block_mut();
        current_block.0.push(instruction);
        let ret_val = match output_var {
            None => Value::unit(),
            Some(output_var) => {
                let rir_var = map_rir_var_to_eval_var(output_var).map_err(|()| {
                    Error::UnexpectedDynamicIntrinsicReturnType(
                        callable_decl.output.to_string(),
                        callee_expr_span,
                    )
                })?;
                Value::Var(rir_var)
            }
        };
        Ok(ret_val)
    }

    fn eval_expr_call_to_spec(
        &mut self,
        call_scope: Scope,
        global_callable_id: StoreItemId,
        functor_app: FunctorApp,
        spec_decl: &SpecDecl,
    ) -> Result<Value, Error> {
        self.eval_context.push_scope(call_scope);
        let block_value = self.try_eval_block(spec_decl.block)?.into_value();
        let popped_scope = self.eval_context.pop_scope();
        assert!(
            popped_scope.package_id == global_callable_id.package,
            "scope package ID mismatch"
        );
        let (popped_callable_id, popped_functor_app) = popped_scope
            .callable
            .expect("callable in scope is not specified");
        assert!(
            popped_callable_id == global_callable_id.item,
            "scope callable ID mismatch"
        );
        assert!(popped_functor_app == functor_app, "scope functor mismatch");
        Ok(block_value)
    }

    fn eval_expr_if(
        &mut self,
        if_expr_id: ExprId,
        condition_expr_id: ExprId,
        body_expr_id: ExprId,
        otherwise_expr_id: Option<ExprId>,
    ) -> Result<EvalControlFlow, Error> {
        // Visit the the condition expression to get its value.
        let condition_control_flow = self.try_eval_expr(condition_expr_id)?;
        if condition_control_flow.is_return() {
            return Err(Error::Unexpected(
                "embedded return in if condition".to_string(),
                self.get_expr_package_span(condition_expr_id),
            ));
        }

        // If the condition value is a Boolean literal, use the value to decide which branch to
        // evaluate.
        let condition_value = condition_control_flow.into_value();
        if let Value::Bool(condition_bool) = condition_value {
            return self.eval_expr_if_with_classical_condition(
                condition_bool,
                body_expr_id,
                otherwise_expr_id,
            );
        }

        // At this point the condition value is not classical, so we need to generate a branching instruction.
        // First, we pop the current block node and generate a new one which the new branches will jump to when their
        // instructions end.
        let current_block_node = self.eval_context.pop_block_node();
        let continuation_block_node_id = self.create_program_block();
        let continuation_block_node = BlockNode {
            id: continuation_block_node_id,
            successor: current_block_node.successor,
        };
        self.eval_context.push_block_node(continuation_block_node);

        // Since the if expression can represent a dynamic value, create a variable to store it if the expression is
        // non-unit.
        let if_expr = self.get_expr(if_expr_id);
        let maybe_if_expr_var = if if_expr.ty == Ty::UNIT {
            None
        } else {
            let variable_id = self.resource_manager.next_var();
            let variable_ty = map_fir_type_to_rir_type(&if_expr.ty);
            Some(rir::Variable {
                variable_id,
                ty: variable_ty,
            })
        };

        // Evaluate the body expression.
        // First, we cache the current static variable mappings so that we can restore them later.
        let cached_mappings = self.clone_current_static_var_map();
        let if_true_block_id =
            self.eval_expr_if_branch(body_expr_id, continuation_block_node_id, maybe_if_expr_var)?;

        // Evaluate the otherwise expression (if any), and determine the block to branch to if the condition is false.
        let if_false_block_id = if let Some(otherwise_expr_id) = otherwise_expr_id {
            // Cache the mappings after the true block so we can compare afterwards.
            let post_if_true_mappings = self.clone_current_static_var_map();
            // Restore the cached mappings from before evaluating the true block.
            self.overwrite_current_static_var_map(cached_mappings);
            let if_false_block_id = self.eval_expr_if_branch(
                otherwise_expr_id,
                continuation_block_node_id,
                maybe_if_expr_var,
            )?;
            // Only keep the static mappings that are the same in both blocks; when they are different,
            // the variable is no longer static across the if expression.
            self.keep_matching_static_var_mappings(&post_if_true_mappings);
            if_false_block_id
        } else {
            // Only keep the static mappings that are the same after the true block as before; when they are different,
            // the variable is no longer static across the if expression.
            self.keep_matching_static_var_mappings(&cached_mappings);

            // Since there is no otherwise block, we branch to the continuation block.
            continuation_block_node_id
        };

        // Finally, we insert the branch instruction.
        let condition_value_var = condition_value.unwrap_var();
        let condition_rir_var = map_eval_var_to_rir_var(condition_value_var);
        let branch_ins =
            Instruction::Branch(condition_rir_var, if_true_block_id, if_false_block_id);
        self.get_program_block_mut(current_block_node.id)
            .0
            .push(branch_ins);

        // Return the value of the if expression.
        let if_expr_value = if let Some(if_expr_var) = maybe_if_expr_var {
            Value::Var(map_rir_var_to_eval_var(if_expr_var).map_err(|()| {
                Error::Unexpected(
                    format!(
                        "dynamic value of type {} in conditional expression",
                        if_expr_var.ty
                    ),
                    self.get_expr_package_span(if_expr_id),
                )
            })?)
        } else {
            Value::unit()
        };
        Ok(EvalControlFlow::Continue(if_expr_value))
    }

    fn eval_expr_if_branch(
        &mut self,
        branch_body_expr_id: ExprId,
        continuation_block_id: rir::BlockId,
        if_expr_var: Option<rir::Variable>,
    ) -> Result<rir::BlockId, Error> {
        // Create the block node that corresponds to the branch body and push it as the active one.
        let block_node_id = self.create_program_block();
        let block_node = BlockNode {
            id: block_node_id,
            successor: Some(continuation_block_id),
        };
        self.eval_context.push_block_node(block_node);

        // Evaluate the branch body expression.
        let body_control = self.try_eval_expr(branch_body_expr_id)?;
        if body_control.is_return() {
            let body_span = self.get_expr_package_span(branch_body_expr_id);
            return Err(Error::Unimplemented("early return".to_string(), body_span));
        }

        // If there is a variable to save the value of the if expression to, add a store instruction.
        if let Some(if_expr_var) = if_expr_var {
            let body_operand = self.map_eval_value_to_rir_operand(&body_control.into_value());
            let store_ins = Instruction::Store(body_operand, if_expr_var);
            self.get_current_rir_block_mut().0.push(store_ins);
        }

        // Finally, jump to the continuation block and pop the current block node.
        let jump_ins = Instruction::Jump(continuation_block_id);
        self.get_current_rir_block_mut().0.push(jump_ins);
        let _ = self.eval_context.pop_block_node();
        Ok(block_node_id)
    }

    fn eval_expr_if_with_classical_condition(
        &mut self,
        condition_bool: bool,
        body_expr_id: ExprId,
        otherwise_expr_id: Option<ExprId>,
    ) -> Result<EvalControlFlow, Error> {
        if condition_bool {
            self.try_eval_expr(body_expr_id)
        } else if let Some(otherwise_expr_id) = otherwise_expr_id {
            self.try_eval_expr(otherwise_expr_id)
        } else {
            // The classical condition evaluated to false, but there is not otherwise block so there is nothing to
            // evaluate.
            // Return unit since it is the only possibility for if expressions with no otherwise block.
            Ok(EvalControlFlow::Continue(Value::unit()))
        }
    }

    fn eval_expr_index(
        &mut self,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        // Get the value of the array expression to use it as the basis to perform a replacement on.
        let array_control_flow = self.try_eval_expr(array_expr_id)?;
        let EvalControlFlow::Continue(array_value) = array_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in index expression".to_string(),
                self.get_expr_package_span(array_expr_id),
            ));
        };

        // Try to evaluate the index and replace expressions to get their value, short-circuiting execution if any of
        // the expressions is a return.
        let index_control_flow = self.try_eval_expr(index_expr_id)?;
        let EvalControlFlow::Continue(index_value) = index_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in index expression".to_string(),
                self.get_expr_package_span(index_expr_id),
            ));
        };

        // Get the value at the specified index.
        let array = array_value.unwrap_array();
        let index_expr = self.get_expr(index_expr_id);
        let hir_package_id = map_fir_package_to_hir(self.get_current_package_id());
        let index_package_span = PackageSpan {
            package: hir_package_id,
            span: index_expr.span,
        };
        let value_result = match index_value {
            Value::Int(index) => index_array(&array, index, index_package_span),
            Value::Range(range) => slice_array(
                &array,
                range.start,
                range.step,
                range.end,
                index_package_span,
            ),
            _ => panic!("invalid kind of value for index"),
        };
        let value = value_result.map_err(Error::from)?;
        Ok(EvalControlFlow::Continue(value))
    }

    fn eval_expr_return(&mut self, expr_id: ExprId) -> Result<EvalControlFlow, Error> {
        let control_flow = self.try_eval_expr(expr_id)?;
        Ok(EvalControlFlow::Return(control_flow.into_value()))
    }

    fn eval_expr_array(&mut self, exprs: &Vec<ExprId>) -> Result<EvalControlFlow, Error> {
        let mut values = Vec::with_capacity(exprs.len());
        for expr_id in exprs {
            let control_flow = self.try_eval_expr(*expr_id)?;
            if control_flow.is_return() {
                return Err(Error::Unexpected(
                    "embedded return in array".to_string(),
                    self.get_expr_package_span(*expr_id),
                ));
            }
            values.push(control_flow.into_value());
        }
        Ok(EvalControlFlow::Continue(Value::Array(values.into())))
    }

    fn eval_expr_tuple(&mut self, exprs: &Vec<ExprId>) -> Result<EvalControlFlow, Error> {
        let mut values = Vec::with_capacity(exprs.len());
        for expr_id in exprs {
            let control_flow = self.try_eval_expr(*expr_id)?;
            if control_flow.is_return() {
                return Err(Error::Unexpected(
                    "embedded return in tuple".to_string(),
                    self.get_expr_package_span(*expr_id),
                ));
            }
            values.push(control_flow.into_value());
        }
        Ok(EvalControlFlow::Continue(Value::Tuple(values.into(), None)))
    }

    fn eval_expr_unary(
        &mut self,
        un_op: UnOp,
        value_expr_id: ExprId,
        unary_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<EvalControlFlow, Error> {
        let value_expr_package_span = self.get_expr_package_span(value_expr_id);
        let value_control_flow = self.try_eval_expr(value_expr_id)?;
        let EvalControlFlow::Continue(value) = value_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in unary operation expression".to_string(),
                value_expr_package_span,
            ));
        };

        // Get the variable type corresponding to the value the unary operator acts upon.
        let Some(eval_variable_type) = try_get_eval_var_type(&value) else {
            return Err(Error::Unexpected(
                format!("invalid type for unary operation value: {value}"),
                value_expr_package_span,
            ));
        };

        // The leading positive operator is a no-op.
        if matches!(un_op, UnOp::Pos) {
            let control_flow = EvalControlFlow::Continue(value);
            return Ok(control_flow);
        }

        // If the variable is a literal, we can evaluate the unary operation directly.
        if !matches!(value, Value::Var(_)) {
            let result = eval_un_op_with_literals(un_op, value);
            return Ok(EvalControlFlow::Continue(result));
        }

        // For all the other supported unary operations we have to generate an instruction, so create a variable to
        // store the result.
        let variable_id = self.resource_manager.next_var();
        let rir_variable_type = map_eval_var_type_to_rir_type(eval_variable_type);
        let rir_variable = rir::Variable {
            variable_id,
            ty: rir_variable_type,
        };

        // Generate the instruction depending on the unary operator.
        let value_operand = self.map_eval_value_to_rir_operand(&value);
        let instruction = match un_op {
            UnOp::Neg => match rir_variable_type {
                rir::Ty::Integer => {
                    let constant = Operand::Literal(Literal::Integer(-1));
                    Instruction::Mul(constant, value_operand, rir_variable)
                }
                rir::Ty::Double => {
                    let constant = Operand::Literal(Literal::Double(-1.0));
                    Instruction::Fmul(constant, value_operand, rir_variable)
                }
                _ => panic!("invalid type for negation operator {rir_variable_type}"),
            },
            UnOp::NotB => {
                assert!(matches!(rir_variable_type, rir::Ty::Integer));
                Instruction::BitwiseNot(value_operand, rir_variable)
            }
            UnOp::NotL => {
                assert!(matches!(rir_variable_type, rir::Ty::Boolean));
                Instruction::LogicalNot(value_operand, rir_variable)
            }
            UnOp::Functor(_) | UnOp::Unwrap => {
                return Err(Error::Unexpected(
                    format!("invalid unary operator: {un_op}"),
                    unary_expr_span,
                ));
            }
            UnOp::Pos => panic!("the leading positive operator should have been a no-op"),
        };

        // Insert the instruction and return the corresponding evaluator variable.
        self.get_current_rir_block_mut().0.push(instruction);
        let eval_variable = map_rir_var_to_eval_var(rir_variable).map_err(|()| {
            Error::Unexpected(
                format!("{} type in unop", rir_variable.ty),
                self.get_expr_package_span(value_expr_id),
            )
        })?;
        Ok(EvalControlFlow::Continue(Value::Var(eval_variable)))
    }

    fn eval_expr_update_index(
        &mut self,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        update_expr_id: ExprId,
    ) -> Result<EvalControlFlow, Error> {
        // Get the value of the array expression to use it as the basis to perform a replacement on.
        let array_control_flow = self.try_eval_expr(array_expr_id)?;
        let EvalControlFlow::Continue(array_value) = array_control_flow else {
            return Err(Error::Unexpected(
                "embedded return in index expression".to_string(),
                self.get_expr_package_span(array_expr_id),
            ));
        };
        let array = array_value.unwrap_array();
        let updated_array = self.eval_array_update_index(&array, index_expr_id, update_expr_id)?;
        Ok(EvalControlFlow::Continue(updated_array))
    }

    fn eval_expr_var(&mut self, res: &Res) -> Value {
        match res {
            Res::Err => panic!("resolution error"),
            Res::Item(item) => Value::Global(
                StoreItemId {
                    package: item.package.unwrap_or(self.get_current_package_id()),
                    item: item.item,
                },
                FunctorApp::default(),
            ),
            Res::Local(local_var_id) => {
                let bound_value = self
                    .eval_context
                    .get_current_scope()
                    .get_hybrid_local_value(*local_var_id);

                // Check whether the bound value is a mutable variable, and if so, return its value directly rather than
                // the variable if it is static at this moment.
                if let Value::Var(var) = bound_value {
                    let current_scope = self.eval_context.get_current_scope();
                    if let Some(literal) = current_scope.get_static_value(var.id.into()) {
                        map_rir_literal_to_eval_value(*literal)
                    } else {
                        bound_value.clone()
                    }
                } else {
                    bound_value.clone()
                }
            }
        }
    }

    fn eval_expr_while(
        &mut self,
        condition_expr_id: ExprId,
        body_block_id: BlockId,
    ) -> Result<EvalControlFlow, Error> {
        // Verify assumptions.
        assert!(
            self.is_classical_expr(condition_expr_id),
            "loop conditions must be purely classical"
        );
        let body_block = self.get_block(body_block_id);
        assert_eq!(
            body_block.ty,
            Ty::UNIT,
            "the type of a loop block is expected to be Unit"
        );

        // Evaluate the block until the loop condition is false.
        let condition_expr_span = self.get_expr_package_span(condition_expr_id);
        let mut condition_control_flow = self.try_eval_expr(condition_expr_id)?;
        if condition_control_flow.is_return() {
            return Err(Error::Unexpected(
                "embedded return in loop condition".to_string(),
                condition_expr_span,
            ));
        }
        let mut condition_boolean = condition_control_flow.into_value().unwrap_bool();
        while condition_boolean {
            // Evaluate the loop block.
            let block_control_flow = self.try_eval_block(body_block_id)?;
            if block_control_flow.is_return() {
                return Ok(block_control_flow);
            }

            // Re-evaluate the condition now that the block evaluation is done
            condition_control_flow = self.try_eval_expr(condition_expr_id)?;
            if condition_control_flow.is_return() {
                return Err(Error::Unexpected(
                    "embedded return in loop condition".to_string(),
                    condition_expr_span,
                ));
            }
            condition_boolean = condition_control_flow.into_value().unwrap_bool();
        }

        // We have evaluated the loop so just return unit as the value of this loop expression.
        Ok(EvalControlFlow::Continue(Value::unit()))
    }

    fn eval_result_as_bool_operand(&mut self, result: val::Result) -> Operand {
        match result {
            val::Result::Id(id) => {
                // If this is a result ID, generate the instruction to read it.
                let result_operand = Operand::Literal(Literal::Result(
                    id.try_into().expect("could not convert result ID to u32"),
                ));
                let read_result_callable_id =
                    self.get_or_insert_callable(builder::read_result_decl());
                let variable_id = self.resource_manager.next_var();
                let variable_ty = rir::Ty::Boolean;
                let variable = rir::Variable {
                    variable_id,
                    ty: variable_ty,
                };
                let instruction = Instruction::Call(
                    read_result_callable_id,
                    vec![result_operand],
                    Some(variable),
                );
                let current_block = self.get_current_rir_block_mut();
                current_block.0.push(instruction);
                Operand::Variable(variable)
            }
            val::Result::Val(bool) => Operand::Literal(Literal::Bool(bool)),
            val::Result::Loss => panic!("loss result should not occur in partial evaluation"),
        }
    }

    fn generate_instructions_for_binary_operation_with_double_operands(
        &mut self,
        bin_op: BinOp,
        lhs_operand: Operand,
        rhs_operand: Operand,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<rir::Variable, Error> {
        let bin_op_variable_id = self.resource_manager.next_var();

        let bin_op_rir_variable = match bin_op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                rir::Variable::new_double(bin_op_variable_id)
            }
            BinOp::Eq | BinOp::Neq | BinOp::Gt | BinOp::Gte | BinOp::Lt | BinOp::Lte => {
                rir::Variable::new_boolean(bin_op_variable_id)
            }
            _ => panic!("unsupported binary operation for double: {bin_op:?}"),
        };

        let bin_op_rir_ins = match bin_op {
            BinOp::Add => Instruction::Fadd(lhs_operand, rhs_operand, bin_op_rir_variable),
            BinOp::Sub => Instruction::Fsub(lhs_operand, rhs_operand, bin_op_rir_variable),
            BinOp::Mul => Instruction::Fmul(lhs_operand, rhs_operand, bin_op_rir_variable),
            BinOp::Div => {
                // Validate that the RHS is not a zero.
                if let Operand::Literal(Literal::Double(0.0)) = rhs_operand {
                    let error = EvalError::DivZero(bin_op_expr_span).into();
                    return Err(error);
                }

                Instruction::Fdiv(lhs_operand, rhs_operand, bin_op_rir_variable)
            }
            BinOp::Eq => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndEqual,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            BinOp::Neq => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndNotEqual,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            BinOp::Gt => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndGreaterThan,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            BinOp::Gte => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndGreaterThanOrEqual,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            BinOp::Lt => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndLessThan,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            BinOp::Lte => Instruction::Fcmp(
                FcmpConditionCode::OrderedAndLessThanOrEqual,
                lhs_operand,
                rhs_operand,
                bin_op_rir_variable,
            ),
            _ => panic!("unsupported binary operation for double: {bin_op:?}"),
        };
        self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
        Ok(bin_op_rir_variable)
    }

    #[allow(clippy::too_many_lines)]
    fn generate_instructions_for_binary_operation_with_integer_operands(
        &mut self,
        bin_op: BinOp,
        lhs_operand: Operand,
        rhs_operand: Operand,
        bin_op_expr_span: PackageSpan, // For diagnostic purposes only.
    ) -> Result<rir::Variable, Error> {
        let rir_variable = match bin_op {
            BinOp::Add => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Add(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Sub => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Sub(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Mul => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Mul(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Div => {
                // Validate that the RHS is not a zero.
                if let Operand::Literal(Literal::Integer(0)) = rhs_operand {
                    let error = EvalError::DivZero(bin_op_expr_span).into();
                    return Err(error);
                }
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Sdiv(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Mod => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Srem(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Exp => {
                // Validate the exponent.
                let Operand::Literal(Literal::Integer(exponent)) = rhs_operand else {
                    let error = Error::Unexpected(
                        "exponent must be a classical integer".to_string(),
                        bin_op_expr_span,
                    );
                    return Err(error);
                };
                if exponent < 0 {
                    let error = EvalError::InvalidNegativeInt(exponent, bin_op_expr_span).into();
                    return Err(error);
                }

                // Generate a series of multiplication instructions that represent the exponentiation.
                let mut current_rir_variable =
                    rir::Variable::new_integer(self.resource_manager.next_var());
                let init_ins =
                    Instruction::Store(Operand::Literal(Literal::Integer(1)), current_rir_variable);
                self.get_current_rir_block_mut().0.push(init_ins);
                for _ in 0..exponent {
                    let mult_variable =
                        rir::Variable::new_integer(self.resource_manager.next_var());
                    let mult_ins = Instruction::Mul(
                        Operand::Variable(current_rir_variable),
                        lhs_operand,
                        mult_variable,
                    );
                    self.get_current_rir_block_mut().0.push(mult_ins);
                    current_rir_variable = mult_variable;
                }
                current_rir_variable
            }
            BinOp::AndB => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::BitwiseAnd(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::OrB => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::BitwiseOr(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::XorB => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::BitwiseXor(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Shl => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Shl(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Shr => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_integer(bin_op_variable_id);
                let bin_op_rir_ins =
                    Instruction::Ashr(lhs_operand, rhs_operand, bin_op_rir_variable);
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Eq => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Eq,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Neq => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Ne,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Gt => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Sgt,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Gte => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Sge,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Lt => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Slt,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            BinOp::Lte => {
                let bin_op_variable_id = self.resource_manager.next_var();
                let bin_op_rir_variable = rir::Variable::new_boolean(bin_op_variable_id);
                let bin_op_rir_ins = Instruction::Icmp(
                    ConditionCode::Sle,
                    lhs_operand,
                    rhs_operand,
                    bin_op_rir_variable,
                );
                self.get_current_rir_block_mut().0.push(bin_op_rir_ins);
                bin_op_rir_variable
            }
            _ => panic!("unsupported binary operation for integers: {bin_op:?}"),
        };
        Ok(rir_variable)
    }

    fn get_block(&self, id: BlockId) -> &'a Block {
        let block_id = StoreBlockId::from((self.get_current_package_id(), id));
        self.package_store.get_block(block_id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        let expr_id = StoreExprId::from((self.get_current_package_id(), id));
        self.package_store.get_expr(expr_id)
    }

    #[allow(clippy::similar_names)]
    fn get_expr_package_span(&self, id: ExprId) -> PackageSpan {
        let fir_package_id = self.get_current_package_id();
        let expr = self.package_store.get_expr((fir_package_id, id).into());
        let hir_package_id = map_fir_package_to_hir(fir_package_id);
        PackageSpan {
            package: hir_package_id,
            span: expr.span,
        }
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        let pat_id = StorePatId::from((self.get_current_package_id(), id));
        self.package_store.get_pat(pat_id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        let stmt_id = StoreStmtId::from((self.get_current_package_id(), id));
        self.package_store.get_stmt(stmt_id)
    }

    fn get_current_package_id(&self) -> PackageId {
        self.eval_context.get_current_scope().package_id
    }

    fn get_current_rir_block_mut(&mut self) -> &mut rir::Block {
        self.get_program_block_mut(self.eval_context.get_current_block_id())
    }

    fn get_current_scope_exec_graph(&self) -> &ExecGraph {
        if let Some(spec_decl) = self.get_current_scope_spec_decl() {
            &spec_decl.exec_graph
        } else {
            &self
                .entry
                .expect("entry expression must be present when not in scope")
                .exec_graph
        }
    }

    fn get_current_scope_spec_decl(&self) -> Option<&SpecDecl> {
        let current_scope = self.eval_context.get_current_scope();
        let (local_item_id, functor_app) = current_scope.callable?;
        let store_item_id = StoreItemId::from((current_scope.package_id, local_item_id));
        let global = self
            .package_store
            .get_global(store_item_id)
            .expect("global does not exist");
        let Global::Callable(callable_decl) = global else {
            panic!("global is not a callable");
        };

        let CallableImpl::Spec(spec_impl) = &callable_decl.implementation else {
            panic!("callable does not implement specializations");
        };

        let spec_decl = get_spec_decl(spec_impl, functor_app);
        Some(spec_decl)
    }

    fn get_expr_compute_kind(&self, expr_id: ExprId) -> ComputeKind {
        let current_package_id = self.get_current_package_id();
        let store_expr_id = StoreExprId::from((current_package_id, expr_id));
        let expr_generator_set = self.compute_properties.get_expr(store_expr_id);
        let callable_scope = self.eval_context.get_current_scope();
        expr_generator_set.generate_application_compute_kind(&callable_scope.args_value_kind)
    }

    fn is_unresolved_callee_expr(&self, expr_id: ExprId) -> bool {
        let current_package_id = self.get_current_package_id();
        let store_expr_id = StoreExprId::from((current_package_id, expr_id));
        self.compute_properties
            .is_unresolved_callee_expr(store_expr_id)
    }

    fn get_call_compute_kind(&self, callable_scope: &Scope) -> ComputeKind {
        let store_item_id = StoreItemId::from((
            callable_scope.package_id,
            callable_scope
                .callable
                .expect("callable should be present")
                .0,
        ));
        let ItemComputeProperties::Callable(callable_compute_properties) =
            self.compute_properties.get_item(store_item_id)
        else {
            panic!("item compute properties not found");
        };
        let callable_generator_set = match &callable_scope.callable {
            Some((_, functor_app)) => match (functor_app.adjoint, functor_app.controlled) {
                (false, 0) => &callable_compute_properties.body,
                (false, _) => callable_compute_properties
                    .ctl
                    .as_ref()
                    .expect("controlled should be supported"),
                (true, 0) => callable_compute_properties
                    .adj
                    .as_ref()
                    .expect("adjoint should be supported"),
                (true, _) => callable_compute_properties
                    .ctl_adj
                    .as_ref()
                    .expect("controlled adjoint should be supported"),
            },
            None => panic!("call compute kind should have callable"),
        };
        callable_generator_set.generate_application_compute_kind(&callable_scope.args_value_kind)
    }

    fn try_create_mutable_variable(
        &mut self,
        local_var_id: LocalVarId,
        value: &Value,
    ) -> Option<(rir::VariableId, Option<Literal>)> {
        // Check if we can create a mutable variable for this value.
        let var_ty = try_get_eval_var_type(value)?;

        // Create an evaluator variable and insert it.
        let var_id = self.resource_manager.next_var();
        let eval_var = Var {
            id: var_id.into(),
            ty: var_ty,
        };
        self.eval_context
            .get_current_scope_mut()
            .insert_hybrid_local_value(local_var_id, Value::Var(eval_var));

        // Insert a store instruction.
        let value_operand = self.map_eval_value_to_rir_operand(value);
        let rir_var = map_eval_var_to_rir_var(eval_var);
        let store_ins = Instruction::Store(value_operand, rir_var);
        self.get_current_rir_block_mut().0.push(store_ins);

        // Create a mutable variable, mapping it to the static value if any.
        let static_value = match value_operand {
            Operand::Literal(literal) => Some(literal),
            Operand::Variable(_) => None,
        };

        Some((var_id, static_value))
    }

    fn get_or_insert_callable(&mut self, callable: Callable) -> CallableId {
        // Check if the callable is already in the program, and if not add it.
        let callable_name = callable.name.clone();
        if let Entry::Vacant(entry) = self.callables_map.entry(callable_name.clone().into()) {
            let callable_id = self.resource_manager.next_callable();
            entry.insert(callable_id);
            self.program.callables.insert(callable_id, callable);
        }

        *self
            .callables_map
            .get(callable_name.as_str())
            .expect("callable not present")
    }

    fn get_program_block_mut(&mut self, id: rir::BlockId) -> &mut rir::Block {
        self.program
            .blocks
            .get_mut(id)
            .expect("program block does not exist")
    }

    fn is_classical_expr(&self, expr_id: ExprId) -> bool {
        let compute_kind = self.get_expr_compute_kind(expr_id);
        matches!(compute_kind, ComputeKind::Classical)
    }

    fn allocate_qubit(&mut self) -> Value {
        let qubit = self.resource_manager.allocate_qubit();
        Value::Qubit(qubit)
    }

    fn measure_qubits(&mut self, callable_decl: &CallableDecl, args_value: Value) -> Value {
        let mut input_type = Vec::new();
        let mut operands = Vec::new();
        let mut results_values = Vec::new();

        match args_value {
            Value::Qubit(qubit) => {
                input_type.push(qsc_rir::rir::Ty::Qubit);
                operands.push(self.map_eval_value_to_rir_operand(&Value::Qubit(qubit)));
            }
            Value::Tuple(values, _) => {
                for value in &*values {
                    let Value::Qubit(qubit) = value else {
                        panic!(
                            "by this point a qsc_pass should have checked that all arguments are Qubits"
                        )
                    };
                    input_type.push(qsc_rir::rir::Ty::Qubit);
                    operands.push(self.map_eval_value_to_rir_operand(&Value::Qubit(qubit.clone())));
                }
            }
            _ => {
                panic!("by this point a qsc_pass should have checked that all arguments are Qubits")
            }
        }

        match &callable_decl.output {
            qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Result) => {
                input_type.push(qsc_rir::rir::Ty::Result);
                let result_value = Value::Result(self.resource_manager.next_result_register());
                let result_operand = self.map_eval_value_to_rir_operand(&result_value);
                operands.push(result_operand);
                results_values.push(result_value);
            }
            qsc_fir::ty::Ty::Tuple(outputs) => {
                for output in outputs {
                    if matches!(output, qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Result)) {
                        input_type.push(qsc_rir::rir::Ty::Result);
                        let result_value =
                            Value::Result(self.resource_manager.next_result_register());
                        let result_operand = self.map_eval_value_to_rir_operand(&result_value);
                        operands.push(result_operand);
                        results_values.push(result_value);
                    } else {
                        panic!(
                            "by this point a qsc_pass should have checked that all outputs are Results"
                        )
                    }
                }
            }
            _ => {
                panic!("by this point a qsc_pass should have checked that all outputs are Results")
            }
        }

        let measurement_callable = Callable {
            name: callable_decl.name.name.to_string(),
            input_type,
            output_type: None,
            body: None,
            call_type: CallableType::Measurement,
        };

        // Check if the callable has already been added to the program and if not do so now.
        let measure_callable_id = self.get_or_insert_callable(measurement_callable);
        let instruction = Instruction::Call(measure_callable_id, operands, None);
        let current_block = self.get_current_rir_block_mut();
        current_block.0.push(instruction);

        match results_values.len() {
            0 => panic!("unexpected unitary measurement"),
            1 => results_values[0].clone(),
            2.. => Value::Tuple(results_values.into(), None),
        }
    }

    fn measure_qubit(&mut self, measure_callable: Callable, args_value: Value) -> Value {
        // Get the qubit and result IDs to use in the qubit measure instruction.
        let qubit = args_value.unwrap_qubit();
        let qubit_value = Value::Qubit(qubit);
        let qubit_operand = self.map_eval_value_to_rir_operand(&qubit_value);
        let result_value = Value::Result(self.resource_manager.next_result_register());
        let result_operand = self.map_eval_value_to_rir_operand(&result_value);

        // Check if the callable has already been added to the program and if not do so now.
        let measure_callable_id = self.get_or_insert_callable(measure_callable);
        let args = vec![qubit_operand, result_operand];
        let instruction = Instruction::Call(measure_callable_id, args, None);
        let current_block = self.get_current_rir_block_mut();
        current_block.0.push(instruction);

        // Return the result value.
        result_value
    }

    fn release_qubit(&mut self, args_value: Value) -> Value {
        let qubit = args_value.unwrap_qubit();
        self.resource_manager.release_qubit(&qubit);

        // The value of a qubit release is unit.
        Value::unit()
    }

    fn resolve_args(
        &self,
        store_pat_id: StorePatId,
        value: Value,
        args_span: Option<PackageSpan>,
        ctls: Option<(StorePatId, u8)>,
        fixed_args: Option<Rc<[Value]>>,
    ) -> Result<(Vec<Arg>, Option<Arg>), Error> {
        let mut value = value;
        let ctls_arg = if let Some((ctls_pat_id, ctls_count)) = ctls {
            let mut ctls = vec![];
            for _ in 0..ctls_count {
                let [c, rest] = &*value.unwrap_tuple() else {
                    panic!("controls + arguments tuple should be arity 2");
                };
                ctls.extend_from_slice(&c.clone().unwrap_array());
                value = rest.clone();
            }
            if !are_ctls_unique(&ctls, &value) {
                let span = args_span.expect("span should be present");
                return Err(EvalError::QubitUniqueness(span).into());
            }
            let ctls_pat = self.package_store.get_pat(ctls_pat_id);
            let ctls_value = Value::Array(ctls.into());
            match &ctls_pat.kind {
                PatKind::Discard => Some(Arg::Discard(ctls_value)),
                PatKind::Bind(ident) => {
                    let variable = Variable {
                        name: ident.name.clone(),
                        value: ctls_value,
                        span: ident.span,
                    };
                    let ctl_arg = Arg::Var(ident.id, variable);
                    Some(ctl_arg)
                }
                PatKind::Tuple(_) => panic!("control qubits pattern is not expected to be a tuple"),
            }
        } else {
            None
        };

        let value = if let Some(fixed_args) = fixed_args {
            let mut fixed_args = fixed_args.to_vec();
            fixed_args.push(value);
            Value::Tuple(fixed_args.into(), None)
        } else {
            value
        };

        let pat = self.package_store.get_pat(store_pat_id);
        let args = match &pat.kind {
            PatKind::Discard => vec![Arg::Discard(value)],
            PatKind::Bind(ident) => {
                let variable = Variable {
                    name: ident.name.clone(),
                    value,
                    span: ident.span,
                };
                vec![Arg::Var(ident.id, variable)]
            }
            PatKind::Tuple(pats) => {
                let values = value.unwrap_tuple();
                assert_eq!(
                    pats.len(),
                    values.len(),
                    "pattern tuple and value tuple have different arity"
                );
                let mut args = Vec::new();
                let pat_value_tuples = pats.iter().zip(values.to_vec());
                for (pat_id, value) in pat_value_tuples {
                    // At this point we should no longer have control qubits so pass None.
                    let (mut element_args, None) = self
                        .resolve_args(
                            (store_pat_id.package, *pat_id).into(),
                            value,
                            None,
                            None,
                            None,
                        )
                        .expect("no controls to verify")
                    else {
                        panic!("no control qubits are expected");
                    };
                    args.append(&mut element_args);
                }
                args
            }
        };
        Ok((args, ctls_arg))
    }

    fn try_eval_block(&mut self, block_id: BlockId) -> Result<EvalControlFlow, Error> {
        let block = self.get_block(block_id);
        let mut return_stmt_id = None;
        let mut last_control_flow = EvalControlFlow::Continue(Value::unit());

        // Iterate through the statements until we hit a return or reach the last statement.
        let mut stmts_iter = block.stmts.iter();
        for stmt_id in stmts_iter.by_ref() {
            last_control_flow = self.try_eval_stmt(*stmt_id)?;
            if last_control_flow.is_return() {
                return_stmt_id = Some(*stmt_id);
                break;
            }
        }

        // While we support multiple returns within a callable, disallow situations in which statements are left
        // unprocessed when we are evaluating a branch within a callable scope.
        let remaining_stmt_count = stmts_iter.count();
        let current_scope = self.eval_context.get_current_scope();
        if remaining_stmt_count > 0 && current_scope.is_currently_evaluating_branch() {
            let return_stmt =
                self.get_stmt(return_stmt_id.expect("a return statement ID must have been set"));
            let hir_package_id = map_fir_package_to_hir(self.get_current_package_id());
            let return_stmt_package_span = PackageSpan {
                package: hir_package_id,
                span: return_stmt.span,
            };
            Err(Error::Unimplemented(
                "early return".to_string(),
                return_stmt_package_span,
            ))
        } else {
            Ok(last_control_flow)
        }
    }

    fn try_eval_expr(&mut self, expr_id: ExprId) -> Result<EvalControlFlow, Error> {
        // An expression is evaluated differently depending on whether it is purely classical or hybrid.
        if self.is_classical_expr(expr_id) {
            self.eval_classical_expr(expr_id)
        } else {
            self.eval_hybrid_expr(expr_id)
        }
    }

    fn try_eval_stmt(&mut self, stmt_id: StmtId) -> Result<EvalControlFlow, Error> {
        let stmt = self.get_stmt(stmt_id);
        match stmt.kind {
            StmtKind::Expr(expr_id) => {
                // Since non-semi expressions are the only ones whose value is non-unit (their value is the same as the
                // value of the expression), they do not need to map their control flow to be unit on continue.
                self.try_eval_expr(expr_id)
            }
            StmtKind::Semi(expr_id) => {
                let control_flow = self.try_eval_expr(expr_id)?;
                match control_flow {
                    EvalControlFlow::Continue(_) => Ok(EvalControlFlow::Continue(Value::unit())),
                    EvalControlFlow::Return(_) => Ok(control_flow),
                }
            }
            StmtKind::Local(mutability, pat_id, expr_id) => {
                let control_flow = self.try_eval_expr(expr_id)?;
                match control_flow {
                    EvalControlFlow::Continue(value) => {
                        self.bind_value_to_pat(mutability, pat_id, value);
                        Ok(EvalControlFlow::Continue(Value::unit()))
                    }
                    EvalControlFlow::Return(_) => Ok(control_flow),
                }
            }
            StmtKind::Item(_) => {
                // Do nothing and return a continue unit value.
                Ok(EvalControlFlow::Continue(Value::unit()))
            }
        }
    }

    fn update_bindings(&mut self, lhs_expr_id: ExprId, rhs_value: Value) -> Result<(), Error> {
        let lhs_expr = self.get_expr(lhs_expr_id);
        match (&lhs_expr.kind, rhs_value) {
            (ExprKind::Hole, _) => {}
            (ExprKind::Var(Res::Local(local_var_id), _), value) => {
                // We update both the hybrid and classical bindings because there are some cases where an expression is
                // classified as classical by RCA, but some elements of the expression are non-classical.
                //
                // For example, the output of the `Length` intrinsic function is only considered non-classical when used
                // on a dynamically-sized array. However, it can be used on arrays that are considered non-classical,
                // such as arrays of Qubits or Results.
                //
                // Since expressions call expressions to the `Length` intrinsic will be offloaded to the evaluator,
                // the evaluator environment also needs to track some non-classical variables.
                self.update_hybrid_local(lhs_expr, *local_var_id, value.clone())?;
                self.update_classical_local(*local_var_id, value);
            }
            (ExprKind::Tuple(exprs), Value::Tuple(values, _)) => {
                for (expr_id, value) in exprs.iter().zip(values.iter()) {
                    self.update_bindings(*expr_id, value.clone())?;
                }
            }
            _ => unreachable!("unassignable pattern should be disallowed by compiler"),
        }
        Ok(())
    }

    fn update_classical_local(&mut self, local_var_id: LocalVarId, value: Value) {
        // Classical values are not updated when we are within a dynamic branch.
        if self
            .eval_context
            .get_current_scope()
            .is_currently_evaluating_branch()
        {
            return;
        }

        // Variable values are not updated on the classical locals either.
        if matches!(value, Value::Var(_)) {
            return;
        }

        // Create a variable and bind it to the classical environment.
        self.eval_context
            .get_current_scope_mut()
            .env
            .update_variable_in_top_frame(local_var_id, value);
    }

    fn update_hybrid_local(
        &mut self,
        local_expr: &Expr,
        local_var_id: LocalVarId,
        value: Value,
    ) -> Result<(), Error> {
        let bound_value = self
            .eval_context
            .get_current_scope()
            .get_hybrid_local_value(local_var_id);
        if let Value::Var(var) = bound_value {
            // Insert a store instruction when the value of a variable is updated.
            let rhs_operand = self.map_eval_value_to_rir_operand(&value);
            let rir_var = map_eval_var_to_rir_var(*var);
            let store_ins = Instruction::Store(rhs_operand, rir_var);
            self.get_current_rir_block_mut().0.push(store_ins);

            // If this is a mutable variable, make sure to update whether it is static or dynamic.
            let current_scope = self.eval_context.get_current_scope_mut();
            match rhs_operand {
                Operand::Literal(literal) => {
                    // The variable maps to a static literal here, so track that literal value.
                    current_scope.insert_static_var_mapping(rir_var.variable_id, literal);
                }
                Operand::Variable(_) => {
                    // The variable is not known to be some literal value, so remove the static mapping.
                    current_scope.remove_static_value(rir_var.variable_id);
                }
            }
        } else {
            // Verify that we are not updating a value that does not have a backing variable from a dynamic branch
            // because it is unsupported.
            if self
                .eval_context
                .get_current_scope()
                .is_currently_evaluating_branch()
            {
                let error_message = format!(
                    "re-assignment within a dynamic branch is unsupported for type {}",
                    local_expr.ty
                );
                let error =
                    Error::Unexpected(error_message, self.get_expr_package_span(local_expr.id));
                return Err(error);
            }
            self.eval_context
                .get_current_scope_mut()
                .update_hybrid_local_value(local_var_id, value);
        }
        Ok(())
    }

    fn update_hybrid_bindings_from_classical_bindings(
        &mut self,
        lhs_expr_id: ExprId,
    ) -> Result<(), Error> {
        let lhs_expr = &self.get_expr(lhs_expr_id);
        match &lhs_expr.kind {
            ExprKind::Hole => {
                // Nothing to bind to.
            }
            ExprKind::Var(Res::Local(local_var_id), _) => {
                let classical_value = self
                    .eval_context
                    .get_current_scope()
                    .get_classical_local_value(*local_var_id)
                    .clone();
                self.update_hybrid_local(lhs_expr, *local_var_id, classical_value)?;
            }
            ExprKind::Tuple(exprs) => {
                for expr_id in exprs {
                    self.update_hybrid_bindings_from_classical_bindings(*expr_id)?;
                }
            }
            _ => unreachable!("unassignable pattern should be disallowed by compiler"),
        }
        Ok(())
    }

    fn generate_output_recording_instructions(
        &mut self,
        ret_val: Value,
        ty: &Ty,
    ) -> Result<Vec<Instruction>, ()> {
        let mut instrs = Vec::new();

        match ret_val {
            Value::Result(val::Result::Val(_)) => return Err(()),

            Value::Array(vals) => self.record_array(ty, &mut instrs, &vals)?,
            Value::Tuple(vals, _) => self.record_tuple(ty, &mut instrs, &vals)?,
            Value::Result(res) => self.record_result(&mut instrs, res),
            Value::Var(var) => self.record_variable(ty, &mut instrs, var),
            Value::Bool(val) => self.record_bool(&mut instrs, val),
            Value::Int(val) => self.record_int(&mut instrs, val),
            Value::Double(val) => self.record_double(&mut instrs, val),

            Value::BigInt(_)
            | Value::Closure(_)
            | Value::Global(_, _)
            | Value::Pauli(_)
            | Value::Qubit(_)
            | Value::Range(_)
            | Value::String(_) => panic!("unsupported value type in output recording"),
        }

        Ok(instrs)
    }

    fn record_int(&mut self, instrs: &mut Vec<Instruction>, val: i64) {
        let int_record_callable_id = self.get_int_record_callable();
        instrs.push(Instruction::Call(
            int_record_callable_id,
            vec![
                Operand::Literal(Literal::Integer(val)),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
    }

    fn record_double(&mut self, instrs: &mut Vec<Instruction>, val: f64) {
        let double_record_callable_id = self.get_double_record_callable();
        instrs.push(Instruction::Call(
            double_record_callable_id,
            vec![
                Operand::Literal(Literal::Double(val)),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
    }

    fn record_bool(&mut self, instrs: &mut Vec<Instruction>, val: bool) {
        let bool_record_callable_id = self.get_bool_record_callable();
        instrs.push(Instruction::Call(
            bool_record_callable_id,
            vec![
                Operand::Literal(Literal::Bool(val)),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
    }

    fn record_variable(&mut self, ty: &Ty, instrs: &mut Vec<Instruction>, var: Var) {
        let record_callable_id = match ty {
            Ty::Prim(Prim::Bool) => self.get_bool_record_callable(),
            Ty::Prim(Prim::Int) => self.get_int_record_callable(),
            Ty::Prim(Prim::Double) => self.get_double_record_callable(),
            _ => panic!("unsupported variable type in output recording"),
        };
        instrs.push(Instruction::Call(
            record_callable_id,
            vec![
                Operand::Variable(map_eval_var_to_rir_var(var)),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
    }

    fn record_result(&mut self, instrs: &mut Vec<Instruction>, res: val::Result) {
        let result_record_callable_id = self.get_result_record_callable();
        instrs.push(Instruction::Call(
            result_record_callable_id,
            vec![
                Operand::Literal(Literal::Result(
                    res.unwrap_id()
                        .try_into()
                        .expect("result id should fit into u32"),
                )),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
    }

    fn record_tuple(
        &mut self,
        ty: &Ty,
        instrs: &mut Vec<Instruction>,
        vals: &Rc<[Value]>,
    ) -> Result<(), ()> {
        let Ty::Tuple(elem_tys) = ty else {
            panic!("expected tuple type for tuple value");
        };
        let tuple_record_callable_id = self.get_tuple_record_callable();
        instrs.push(Instruction::Call(
            tuple_record_callable_id,
            vec![
                Operand::Literal(Literal::Integer(
                    vals.len()
                        .try_into()
                        .expect("tuple length should fit into u32"),
                )),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
        for (val, elem_ty) in vals.iter().zip(elem_tys.iter()) {
            instrs.extend(self.generate_output_recording_instructions(val.clone(), elem_ty)?);
        }

        Ok(())
    }

    fn record_array(
        &mut self,
        ty: &Ty,
        instrs: &mut Vec<Instruction>,
        vals: &Rc<Vec<Value>>,
    ) -> Result<(), ()> {
        let Ty::Array(elem_ty) = ty else {
            panic!("expected array type for array value");
        };
        let array_record_callable_id = self.get_array_record_callable();
        instrs.push(Instruction::Call(
            array_record_callable_id,
            vec![
                Operand::Literal(Literal::Integer(
                    vals.len()
                        .try_into()
                        .expect("array length should fit into u32"),
                )),
                Operand::Literal(Literal::Pointer),
            ],
            None,
        ));
        for val in vals.iter() {
            instrs.extend(self.generate_output_recording_instructions(val.clone(), elem_ty)?);
        }

        Ok(())
    }

    fn get_array_record_callable(&mut self) -> CallableId {
        if let Some(id) = self.callables_map.get("__quantum__rt__array_record_output") {
            return *id;
        }

        let callable = builder::array_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__array_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn get_tuple_record_callable(&mut self) -> CallableId {
        if let Some(id) = self.callables_map.get("__quantum__rt__tuple_record_output") {
            return *id;
        }

        let callable = builder::tuple_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__tuple_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn get_result_record_callable(&mut self) -> CallableId {
        if let Some(id) = self
            .callables_map
            .get("__quantum__rt__result_record_output")
        {
            return *id;
        }

        let callable = builder::result_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__result_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn get_bool_record_callable(&mut self) -> CallableId {
        if let Some(id) = self.callables_map.get("__quantum__rt__bool_record_output") {
            return *id;
        }

        let callable = builder::bool_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__bool_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn get_double_record_callable(&mut self) -> CallableId {
        if let Some(id) = self
            .callables_map
            .get("__quantum__rt__double_record_output")
        {
            return *id;
        }

        let callable = builder::double_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__double_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn get_int_record_callable(&mut self) -> CallableId {
        if let Some(id) = self.callables_map.get("__quantum__rt__int_record_output") {
            return *id;
        }

        let callable = builder::int_record_decl();
        let callable_id = self.resource_manager.next_callable();
        self.callables_map
            .insert("__quantum__rt__int_record_output".into(), callable_id);
        self.program.callables.insert(callable_id, callable);
        callable_id
    }

    fn map_eval_value_to_rir_operand(&self, value: &Value) -> Operand {
        match value {
            Value::Bool(b) => Operand::Literal(Literal::Bool(*b)),
            Value::Double(d) => Operand::Literal(Literal::Double(*d)),
            Value::Int(i) => Operand::Literal(Literal::Integer(*i)),
            Value::Qubit(q) => Operand::Literal(Literal::Qubit(
                self.resource_manager
                    .map_qubit(q)
                    .try_into()
                    .expect("could not convert qubit ID to u32"),
            )),
            Value::Result(r) => match r {
                val::Result::Id(id) => Operand::Literal(Literal::Result(
                    (*id)
                        .try_into()
                        .expect("could not convert result ID to u32"),
                )),
                val::Result::Val(bool) => Operand::Literal(Literal::Bool(*bool)),
                val::Result::Loss => panic!("loss result should not occur in partial evaluation"),
            },
            Value::Var(var) => Operand::Variable(map_eval_var_to_rir_var(*var)),
            _ => panic!("{value} cannot be mapped to a RIR operand"),
        }
    }

    fn clone_current_static_var_map(&self) -> FxHashMap<VariableId, Literal> {
        self.eval_context
            .get_current_scope()
            .clone_static_var_mappings()
    }

    fn overwrite_current_static_var_map(&mut self, static_vars: FxHashMap<VariableId, Literal>) {
        self.eval_context
            .get_current_scope_mut()
            .set_static_var_mappings(static_vars);
    }

    fn keep_matching_static_var_mappings(
        &mut self,
        other_mappings: &FxHashMap<VariableId, Literal>,
    ) {
        self.eval_context
            .get_current_scope_mut()
            .keep_matching_static_var_mappings(other_mappings);
    }
}

fn eval_un_op_with_literals(un_op: UnOp, value: Value) -> Value {
    match un_op {
        UnOp::Neg => match value {
            Value::Int(i) => Value::Int(-i),
            Value::Double(d) => Value::Double(-d),
            Value::BigInt(b) => Value::BigInt(-b),
            _ => panic!("invalid type for negation operator {}", value.type_name()),
        },
        UnOp::NotB => match value {
            Value::Int(i) => Value::Int(!i),
            Value::BigInt(b) => Value::BigInt(!b),
            _ => panic!(
                "invalid type for bitwise negation operator {}",
                value.type_name()
            ),
        },
        UnOp::NotL => match value {
            Value::Bool(b) => Value::Bool(!b),
            _ => panic!(
                "invalid type for logical negation operator {}",
                value.type_name()
            ),
        },
        UnOp::Functor(functor) => match value {
            Value::Closure(inner) => Value::Closure(
                val::Closure {
                    functor: update_functor_app(functor, inner.functor),
                    ..*inner
                }
                .into(),
            ),
            Value::Global(id, app) => Value::Global(id, update_functor_app(functor, app)),
            _ => panic!("value should be callable"),
        },
        UnOp::Pos | UnOp::Unwrap => value,
    }
}

fn eval_bin_op_with_bool_literals(
    bin_op: BinOp,
    lhs_literal: Literal,
    rhs_literal: Literal,
) -> Value {
    let (Literal::Bool(lhs_bool), Literal::Bool(rhs_bool)) = (lhs_literal, rhs_literal) else {
        panic!("at least one literal is not bool: {lhs_literal}, {rhs_literal}");
    };

    let bin_op_result = match bin_op {
        BinOp::Eq => lhs_bool == rhs_bool,
        BinOp::Neq => lhs_bool != rhs_bool,
        BinOp::AndL => lhs_bool && rhs_bool,
        BinOp::OrL => lhs_bool || rhs_bool,
        _ => panic!("invalid bool operator: {bin_op:?}"),
    };
    Value::Bool(bin_op_result)
}

fn eval_bin_op_with_double_literals(
    bin_op: BinOp,
    lhs_literal: Literal,
    rhs_literal: Literal,
    bin_op_expr_span: PackageSpan, // For diagnostic purposes only
) -> Result<Value, Error> {
    fn eval_double_div(lhs: f64, rhs: f64, span: PackageSpan) -> Result<Value, Error> {
        match (lhs, rhs) {
            (_, 0.0) => Err(EvalError::DivZero(span).into()),
            (lhs, rhs) => Ok(Value::Double(lhs / rhs)),
        }
    }

    // Validate that both literals are doubles.
    let (Literal::Double(lhs), Literal::Double(rhs)) = (lhs_literal, rhs_literal) else {
        panic!("at least one literal is not an double: {lhs_literal}, {rhs_literal}");
    };

    match bin_op {
        BinOp::Eq => {
            // matching simulator behavior
            #[allow(clippy::float_cmp)]
            Ok(Value::Bool(lhs == rhs))
        }
        BinOp::Neq => {
            // matching simulator behavior
            #[allow(clippy::float_cmp)]
            Ok(Value::Bool(lhs != rhs))
        }
        BinOp::Gt => Ok(Value::Bool(lhs > rhs)),
        BinOp::Gte => Ok(Value::Bool(lhs >= rhs)),
        BinOp::Lt => Ok(Value::Bool(lhs < rhs)),
        BinOp::Lte => Ok(Value::Bool(lhs <= rhs)),
        BinOp::Add => Ok(Value::Double(lhs + rhs)),
        BinOp::Sub => Ok(Value::Double(lhs - rhs)),
        BinOp::Mul => Ok(Value::Double(lhs * rhs)),
        BinOp::Div => eval_double_div(lhs, rhs, bin_op_expr_span),
        _ => panic!("invalid double operator: {bin_op:?}"),
    }
}

fn eval_bin_op_with_integer_literals(
    bin_op: BinOp,
    lhs_literal: Literal,
    rhs_literal: Literal,
    bin_op_expr_span: PackageSpan, // For diagnostic purposes only
) -> Result<Value, Error> {
    fn eval_integer_div(lhs_int: i64, rhs_int: i64, span: PackageSpan) -> Result<Value, Error> {
        match (lhs_int, rhs_int) {
            (_, 0) => Err(EvalError::DivZero(span).into()),
            (lhs, rhs) => Ok(Value::Int(lhs / rhs)),
        }
    }

    fn eval_integer_mod(lhs_int: i64, rhs_int: i64, span: PackageSpan) -> Result<Value, Error> {
        match (lhs_int, rhs_int) {
            (_, 0) => Err(EvalError::DivZero(span).into()),
            (lhs, rhs) => Ok(Value::Int(lhs % rhs)),
        }
    }

    fn eval_integer_exp(lhs_int: i64, rhs_int: i64, span: PackageSpan) -> Result<Value, Error> {
        let Ok(rhs_int_as_u32) = u32::try_from(rhs_int) else {
            return Err(EvalError::IntTooLarge(rhs_int, span).into());
        };

        Ok(Value::Int(lhs_int.pow(rhs_int_as_u32)))
    }

    // Validate that both literals are integers.
    let (Literal::Integer(lhs_int), Literal::Integer(rhs_int)) = (lhs_literal, rhs_literal) else {
        panic!("at least one literal is not an integer: {lhs_literal}, {rhs_literal}");
    };

    match bin_op {
        BinOp::Eq => Ok(Value::Bool(lhs_int == rhs_int)),
        BinOp::Neq => Ok(Value::Bool(lhs_int != rhs_int)),
        BinOp::Gt => Ok(Value::Bool(lhs_int > rhs_int)),
        BinOp::Gte => Ok(Value::Bool(lhs_int >= rhs_int)),
        BinOp::Lt => Ok(Value::Bool(lhs_int < rhs_int)),
        BinOp::Lte => Ok(Value::Bool(lhs_int <= rhs_int)),
        BinOp::Add => Ok(Value::Int(lhs_int + rhs_int)),
        BinOp::Sub => Ok(Value::Int(lhs_int - rhs_int)),
        BinOp::Mul => Ok(Value::Int(lhs_int * rhs_int)),
        BinOp::Div => eval_integer_div(lhs_int, rhs_int, bin_op_expr_span),
        BinOp::Mod => eval_integer_mod(lhs_int, rhs_int, bin_op_expr_span),
        BinOp::Exp => eval_integer_exp(lhs_int, rhs_int, bin_op_expr_span),
        BinOp::AndB => Ok(Value::Int(lhs_int & rhs_int)),
        BinOp::OrB => Ok(Value::Int(lhs_int | rhs_int)),
        BinOp::XorB => Ok(Value::Int(lhs_int ^ rhs_int)),
        BinOp::Shl => Ok(Value::Int(lhs_int << rhs_int)),
        BinOp::Shr => Ok(Value::Int(lhs_int >> rhs_int)),
        _ => panic!("invalid integer operator: {bin_op:?}"),
    }
}

fn get_spec_decl(spec_impl: &SpecImpl, functor_app: FunctorApp) -> &SpecDecl {
    if !functor_app.adjoint && functor_app.controlled == 0 {
        &spec_impl.body
    } else if functor_app.adjoint && functor_app.controlled == 0 {
        spec_impl
            .adj
            .as_ref()
            .expect("adjoint specialization does not exist")
    } else if !functor_app.adjoint && functor_app.controlled > 0 {
        spec_impl
            .ctl
            .as_ref()
            .expect("controlled specialization does not exist")
    } else {
        spec_impl
            .ctl_adj
            .as_ref()
            .expect("controlled adjoint specialization does not exits")
    }
}

fn map_eval_var_to_rir_var(var: Var) -> rir::Variable {
    rir::Variable {
        variable_id: var.id.into(),
        ty: map_eval_var_type_to_rir_type(var.ty),
    }
}

fn map_eval_var_type_to_rir_type(var_ty: VarTy) -> rir::Ty {
    match var_ty {
        VarTy::Boolean => rir::Ty::Boolean,
        VarTy::Integer => rir::Ty::Integer,
        VarTy::Double => rir::Ty::Double,
    }
}

fn map_fir_type_to_rir_type(ty: &Ty) -> rir::Ty {
    let Ty::Prim(prim) = ty else {
        panic!("only some primitive types are supported");
    };

    match prim {
        Prim::BigInt
        | Prim::Pauli
        | Prim::Range
        | Prim::RangeFrom
        | Prim::RangeFull
        | Prim::RangeTo
        | Prim::String => panic!("{prim:?} is not a supported primitive type"),
        Prim::Bool => rir::Ty::Boolean,
        Prim::Double => rir::Ty::Double,
        Prim::Int => rir::Ty::Integer,
        Prim::Qubit => rir::Ty::Qubit,
        Prim::Result => rir::Ty::Result,
    }
}

fn map_rir_literal_to_eval_value(literal: rir::Literal) -> Value {
    match literal {
        rir::Literal::Bool(b) => Value::Bool(b),
        rir::Literal::Double(d) => Value::Double(d),
        rir::Literal::Integer(i) => Value::Int(i),
        _ => panic!("{literal:?} RIR literal cannot be mapped to evaluator value"),
    }
}

fn map_rir_var_to_eval_var(var: rir::Variable) -> Result<Var, ()> {
    Ok(Var {
        id: var.variable_id.into(),
        ty: map_rir_type_to_eval_var_type(var.ty)?,
    })
}

fn map_rir_type_to_eval_var_type(ty: rir::Ty) -> Result<VarTy, ()> {
    match ty {
        rir::Ty::Boolean => Ok(VarTy::Boolean),
        rir::Ty::Integer => Ok(VarTy::Integer),
        rir::Ty::Double => Ok(VarTy::Double),
        _ => Err(()),
    }
}

fn try_get_eval_var_type(value: &Value) -> Option<VarTy> {
    match value {
        Value::Bool(_) => Some(VarTy::Boolean),
        Value::Int(_) => Some(VarTy::Integer),
        Value::Double(_) => Some(VarTy::Double),
        Value::Var(var) => Some(var.ty),
        _ => None,
    }
}
