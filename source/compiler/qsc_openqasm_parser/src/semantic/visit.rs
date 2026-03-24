// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// This is setting the foundation for passes and visitors in the QASM semantic analysis phase.
#![allow(dead_code)]

use qsc_data_structures::span::Span;

use crate::{
    parser::ast::{Ident, Path, PathKind},
    semantic::{
        ast::{
            AliasDeclStmt, Annotation, Array, AssignStmt, BarrierStmt, BinOp, BinaryOpExpr, Block,
            BoxStmt, BreakStmt, BuiltinFunctionCall, CalibrationGrammarStmt, CalibrationStmt, Cast,
            ClassicalDeclarationStmt, ConcatExpr, ContinueStmt, DefCalStmt, DefParameter, DefStmt,
            DelayStmt, DurationofCallExpr, EndStmt, EnumerableSet, Expr, ExprKind, ExprStmt,
            ExternDecl, ForStmt, FunctionCall, GateCall, GateModifierKind, GateOperand,
            GateOperandKind, HardwareQubit, IfStmt, IncludeStmt, Index,
            IndexedClassicalTypeAssignStmt, IndexedExpr, InputDeclaration, LiteralKind,
            MeasureArrowStmt, MeasureExpr, OutputDeclaration, Pragma, Program,
            QuantumGateDefinition, QuantumGateModifier, QubitArrayDeclaration, QubitDeclaration,
            Range, ResetStmt, ReturnStmt, Set, SizeofCallExpr, Stmt, StmtKind, SwitchCase,
            SwitchStmt, TimeUnit, UnaryOp, UnaryOpExpr, Version, WhileLoop,
        },
        symbols::SymbolId,
        types::Type,
    },
};

pub trait Visitor: Sized {
    fn visit_program(&mut self, program: &Program) {
        walk_program(self, program);
    }

    fn visit_version(&mut self, version: &Version) {
        walk_version(self, version);
    }

    fn visit_pragma(&mut self, pragma: &Pragma) {
        walk_pragma(self, pragma);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_annotation(&mut self, annotation: &Annotation) {
        walk_annotation(self, annotation);
    }

    fn visit_alias_decl_stmt(&mut self, stmt: &AliasDeclStmt) {
        walk_alias_decl_stmt(self, stmt);
    }

    fn visit_assign_stmt(&mut self, stmt: &AssignStmt) {
        walk_assign_stmt(self, stmt);
    }

    fn visit_barrier_stmt(&mut self, stmt: &BarrierStmt) {
        walk_barrier_stmt(self, stmt);
    }

    fn visit_box_stmt(&mut self, stmt: &BoxStmt) {
        walk_box_stmt(self, stmt);
    }

    fn visit_block(&mut self, block: &Block) {
        walk_block(self, block);
    }

    fn visit_break_stmt(&mut self, stmt: &BreakStmt) {
        walk_break_stmt(self, stmt);
    }

    fn visit_calibration_stmt(&mut self, stmt: &CalibrationStmt) {
        walk_calibration_stmt(self, stmt);
    }

    fn visit_calibration_grammar_stmt(&mut self, stmt: &CalibrationGrammarStmt) {
        walk_calibration_grammar_stmt(self, stmt);
    }

    fn visit_classical_decl_stmt(&mut self, stmt: &ClassicalDeclarationStmt) {
        walk_classical_decl_stmt(self, stmt);
    }

    fn visit_continue_stmt(&mut self, stmt: &ContinueStmt) {
        walk_continue_stmt(self, stmt);
    }

    fn visit_def_stmt(&mut self, stmt: &DefStmt) {
        walk_def_stmt(self, stmt);
    }

    fn visit_def_param(&mut self, stmt: &DefParameter) {
        walk_def_param(self, stmt);
    }

    fn visit_def_cal_stmt(&mut self, stmt: &DefCalStmt) {
        walk_def_cal_stmt(self, stmt);
    }

    fn visit_delay_stmt(&mut self, stmt: &DelayStmt) {
        walk_delay_stmt(self, stmt);
    }

    fn visit_end_stmt(&mut self, stmt: &EndStmt) {
        walk_end_stmt(self, stmt);
    }

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt) {
        walk_expr_stmt(self, stmt);
    }

    fn visit_extern_decl(&mut self, stmt: &ExternDecl) {
        walk_extern_decl(self, stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        walk_for_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        walk_gate_call_stmt(self, stmt);
    }

    fn visit_if_stmt(&mut self, stmt: &IfStmt) {
        walk_if_stmt(self, stmt);
    }

    fn visit_include_stmt(&mut self, stmt: &IncludeStmt) {
        walk_include_stmt(self, stmt);
    }

    fn visit_indexed_classical_type_assign_stmt(&mut self, stmt: &IndexedClassicalTypeAssignStmt) {
        walk_indexed_classical_type_assign_stmt(self, stmt);
    }

    fn visit_input_declaration(&mut self, stmt: &InputDeclaration) {
        walk_input_declaration(self, stmt);
    }

    fn visit_output_declaration(&mut self, stmt: &OutputDeclaration) {
        walk_output_declaration(self, stmt);
    }

    fn visit_measure_arrow_stmt(&mut self, stmt: &MeasureArrowStmt) {
        walk_measure_arrow_stmt(self, stmt);
    }

    fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
        walk_quantum_gate_definition(self, stmt);
    }

    fn visit_qubit_decl(&mut self, stmt: &QubitDeclaration) {
        walk_qubit_decl(self, stmt);
    }

    fn visit_qubit_array_decl(&mut self, stmt: &QubitArrayDeclaration) {
        walk_qubit_array_decl(self, stmt);
    }

    fn visit_reset_stmt(&mut self, stmt: &ResetStmt) {
        walk_reset_stmt(self, stmt);
    }

    fn visit_return_stmt(&mut self, stmt: &ReturnStmt) {
        walk_return_stmt(self, stmt);
    }

    fn visit_switch_stmt(&mut self, stmt: &SwitchStmt) {
        walk_switch_stmt(self, stmt);
    }

    fn visit_while_loop(&mut self, stmt: &WhileLoop) {
        walk_while_loop(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_unary_op_expr(&mut self, expr: &UnaryOpExpr) {
        walk_unary_op_expr(self, expr);
    }

    fn visit_binary_op_expr(&mut self, expr: &BinaryOpExpr) {
        walk_binary_op_expr(self, expr);
    }

    fn visit_function_call_expr(&mut self, expr: &FunctionCall) {
        walk_function_call_expr(self, expr);
    }

    fn visit_builtin_function_call_expr(&mut self, expr: &BuiltinFunctionCall) {
        walk_builtin_function_call_expr(self, expr);
    }

    fn visit_cast_expr(&mut self, expr: &Cast) {
        walk_cast_expr(self, expr);
    }

    fn visit_indexed_expr(&mut self, expr: &IndexedExpr) {
        walk_indexed_expr(self, expr);
    }

    fn visit_measure_expr(&mut self, expr: &MeasureExpr) {
        walk_measure_expr(self, expr);
    }

    fn visit_sizeof_call_expr(&mut self, expr: &SizeofCallExpr) {
        walk_sizeof_call_expr(self, expr);
    }

    fn visit_concat_expr(&mut self, expr: &ConcatExpr) {
        walk_concat_expr(self, expr);
    }

    fn visit_durationof_call_expr(&mut self, expr: &DurationofCallExpr) {
        walk_durationof_call_expr(self, expr);
    }

    fn visit_set(&mut self, set: &Set) {
        walk_set(self, set);
    }

    fn visit_range(&mut self, range: &Range) {
        walk_range(self, range);
    }

    fn visit_quantum_gate_modifier(&mut self, modifier: &QuantumGateModifier) {
        walk_quantum_gate_modifier(self, modifier);
    }

    fn visit_gate_operand(&mut self, operand: &GateOperand) {
        walk_gate_operand(self, operand);
    }

    fn visit_hardware_qubit(&mut self, qubit: &HardwareQubit) {
        walk_hardware_qubit(self, qubit);
    }

    fn visit_enumerable_set(&mut self, set: &EnumerableSet) {
        walk_enumerable_set(self, set);
    }

    fn visit_switch_case(&mut self, case: &SwitchCase) {
        walk_switch_case(self, case);
    }

    fn visit_index(&mut self, index: &Index) {
        walk_index(self, index);
    }

    fn visit_literal(&mut self, literal: &LiteralKind) {
        walk_literal(self, literal);
    }

    fn visit_array(&mut self, array: &Array) {
        walk_array(self, array);
    }

    fn visit_path(&mut self, path: &Path) {
        walk_path(self, path);
    }

    fn visit_path_kind(&mut self, path: &PathKind) {
        walk_path_kind(self, path);
    }

    fn visit_ident(&mut self, _: &Ident) {}

    fn visit_idents(&mut self, idents: &[Ident]) {
        walk_idents(self, idents);
    }

    // Terminal nodes that typically don't need further traversal
    fn visit_span(&mut self, _: Span) {}
    fn visit_symbol_id(&mut self, _: SymbolId) {}
    fn visit_bin_op(&mut self, _: BinOp) {}
    fn visit_unary_op(&mut self, _: UnaryOp) {}
    fn visit_time_unit(&mut self, _: TimeUnit) {}
    fn visit_ty(&mut self, _: &Type) {}
}

pub fn walk_program(vis: &mut impl Visitor, program: &Program) {
    program.version.iter().for_each(|v| vis.visit_version(v));
    program.pragmas.iter().for_each(|p| vis.visit_pragma(p));
    program.statements.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_version(vis: &mut impl Visitor, version: &Version) {
    vis.visit_span(version.span);
}

pub fn walk_pragma(vis: &mut impl Visitor, pragma: &Pragma) {
    vis.visit_span(pragma.span);
    pragma
        .identifier
        .iter()
        .for_each(|id| vis.visit_path_kind(id));
    pragma
        .value_span
        .iter()
        .for_each(|span| vis.visit_span(*span));
}

pub fn walk_stmt(vis: &mut impl Visitor, stmt: &Stmt) {
    vis.visit_span(stmt.span);
    stmt.annotations
        .iter()
        .for_each(|a| vis.visit_annotation(a));
    match stmt.kind.as_ref() {
        StmtKind::Alias(stmt) => vis.visit_alias_decl_stmt(stmt),
        StmtKind::Assign(stmt) => vis.visit_assign_stmt(stmt),
        StmtKind::Barrier(stmt) => vis.visit_barrier_stmt(stmt),
        StmtKind::Box(stmt) => vis.visit_box_stmt(stmt),
        StmtKind::Block(block) => vis.visit_block(block),
        StmtKind::Break(stmt) => vis.visit_break_stmt(stmt),
        StmtKind::Calibration(stmt) => vis.visit_calibration_stmt(stmt),
        StmtKind::CalibrationGrammar(stmt) => vis.visit_calibration_grammar_stmt(stmt),
        StmtKind::ClassicalDecl(stmt) => vis.visit_classical_decl_stmt(stmt),
        StmtKind::Continue(stmt) => vis.visit_continue_stmt(stmt),
        StmtKind::Def(stmt) => vis.visit_def_stmt(stmt),
        StmtKind::DefCal(stmt) => vis.visit_def_cal_stmt(stmt),
        StmtKind::Delay(stmt) => vis.visit_delay_stmt(stmt),
        StmtKind::End(stmt) => vis.visit_end_stmt(stmt),
        StmtKind::ExprStmt(stmt) => vis.visit_expr_stmt(stmt),
        StmtKind::ExternDecl(stmt) => vis.visit_extern_decl(stmt),
        StmtKind::For(stmt) => vis.visit_for_stmt(stmt),
        StmtKind::GateCall(stmt) => vis.visit_gate_call_stmt(stmt),
        StmtKind::If(stmt) => vis.visit_if_stmt(stmt),
        StmtKind::Include(stmt) => vis.visit_include_stmt(stmt),
        StmtKind::IndexedClassicalTypeAssign(stmt) => {
            vis.visit_indexed_classical_type_assign_stmt(stmt);
        }
        StmtKind::InputDeclaration(stmt) => vis.visit_input_declaration(stmt),
        StmtKind::OutputDeclaration(stmt) => vis.visit_output_declaration(stmt),
        StmtKind::MeasureArrow(stmt) => vis.visit_measure_arrow_stmt(stmt),
        StmtKind::Pragma(pragma) => vis.visit_pragma(pragma),
        StmtKind::QuantumGateDefinition(stmt) => vis.visit_quantum_gate_definition(stmt),
        StmtKind::QubitDecl(stmt) => vis.visit_qubit_decl(stmt),
        StmtKind::QubitArrayDecl(stmt) => vis.visit_qubit_array_decl(stmt),
        StmtKind::Reset(stmt) => vis.visit_reset_stmt(stmt),
        StmtKind::Return(stmt) => vis.visit_return_stmt(stmt),
        StmtKind::Switch(stmt) => vis.visit_switch_stmt(stmt),
        StmtKind::WhileLoop(stmt) => vis.visit_while_loop(stmt),
        StmtKind::Err => {}
    }
}

pub fn walk_annotation(vis: &mut impl Visitor, annotation: &Annotation) {
    vis.visit_span(annotation.span);
    vis.visit_path_kind(&annotation.identifier);
    annotation
        .value_span
        .iter()
        .for_each(|span| vis.visit_span(*span));
}

pub fn walk_alias_decl_stmt(vis: &mut impl Visitor, stmt: &AliasDeclStmt) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.exprs.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_assign_stmt(vis: &mut impl Visitor, stmt: &AssignStmt) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.lhs);
    vis.visit_expr(&stmt.rhs);
}

pub fn walk_barrier_stmt(vis: &mut impl Visitor, stmt: &BarrierStmt) {
    vis.visit_span(stmt.span);
    stmt.qubits.iter().for_each(|q| vis.visit_gate_operand(q));
}

pub fn walk_box_stmt(vis: &mut impl Visitor, stmt: &BoxStmt) {
    vis.visit_span(stmt.span);
    stmt.duration.iter().for_each(|d| vis.visit_expr(d));
    stmt.body.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_block(vis: &mut impl Visitor, block: &Block) {
    vis.visit_span(block.span);
    block.stmts.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_break_stmt(vis: &mut impl Visitor, stmt: &BreakStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_calibration_stmt(vis: &mut impl Visitor, stmt: &CalibrationStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_calibration_grammar_stmt(vis: &mut impl Visitor, stmt: &CalibrationGrammarStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_classical_decl_stmt(vis: &mut impl Visitor, stmt: &ClassicalDeclarationStmt) {
    vis.visit_span(stmt.span);
    vis.visit_span(stmt.ty_span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&stmt.init_expr);
}

pub fn walk_continue_stmt(vis: &mut impl Visitor, stmt: &ContinueStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_def_stmt(vis: &mut impl Visitor, stmt: &DefStmt) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.params.iter().for_each(|p| vis.visit_def_param(p));
    vis.visit_block(&stmt.body);
    vis.visit_span(stmt.return_type_span);
    stmt.return_ty_exprs
        .iter()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_def_param(vis: &mut impl Visitor, param: &DefParameter) {
    vis.visit_span(param.span);
    vis.visit_symbol_id(param.symbol_id);
    param.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_def_cal_stmt(vis: &mut impl Visitor, stmt: &DefCalStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_delay_stmt(vis: &mut impl Visitor, stmt: &DelayStmt) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.duration);
    stmt.qubits.iter().for_each(|q| vis.visit_gate_operand(q));
}

pub fn walk_end_stmt(vis: &mut impl Visitor, stmt: &EndStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_expr_stmt(vis: &mut impl Visitor, stmt: &ExprStmt) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.expr);
}

pub fn walk_extern_decl(vis: &mut impl Visitor, stmt: &ExternDecl) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
    stmt.return_ty_exprs
        .iter()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_for_stmt(vis: &mut impl Visitor, stmt: &ForStmt) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.loop_variable);
    stmt.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
    vis.visit_enumerable_set(&stmt.set_declaration);
    vis.visit_stmt(&stmt.body);
}

pub fn walk_gate_call_stmt(vis: &mut impl Visitor, stmt: &GateCall) {
    vis.visit_span(stmt.span);
    stmt.modifiers
        .iter()
        .for_each(|m| vis.visit_quantum_gate_modifier(m));
    vis.visit_symbol_id(stmt.symbol_id);
    vis.visit_span(stmt.gate_name_span);
    stmt.args.iter().for_each(|a| vis.visit_expr(a));
    stmt.qubits.iter().for_each(|q| vis.visit_gate_operand(q));
    stmt.duration.iter().for_each(|d| vis.visit_expr(d));
}

pub fn walk_if_stmt(vis: &mut impl Visitor, stmt: &IfStmt) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.condition);
    vis.visit_stmt(&stmt.if_body);
    stmt.else_body.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_include_stmt(vis: &mut impl Visitor, stmt: &IncludeStmt) {
    vis.visit_span(stmt.span);
}

pub fn walk_indexed_classical_type_assign_stmt(
    vis: &mut impl Visitor,
    stmt: &IndexedClassicalTypeAssignStmt,
) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.lhs);
    stmt.indices.iter().for_each(|i| vis.visit_index(i));
    vis.visit_expr(&stmt.rhs);
}

pub fn walk_input_declaration(vis: &mut impl Visitor, stmt: &InputDeclaration) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_output_declaration(vis: &mut impl Visitor, stmt: &OutputDeclaration) {
    vis.visit_span(stmt.span);
    vis.visit_span(stmt.ty_span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&stmt.init_expr);
}

pub fn walk_measure_arrow_stmt(vis: &mut impl Visitor, stmt: &MeasureArrowStmt) {
    vis.visit_span(stmt.span);
    vis.visit_measure_expr(&stmt.measurement);
    stmt.target.iter().for_each(|t| vis.visit_expr(t));
}

pub fn walk_quantum_gate_definition(vis: &mut impl Visitor, stmt: &QuantumGateDefinition) {
    vis.visit_span(stmt.span);
    vis.visit_span(stmt.name_span);
    vis.visit_symbol_id(stmt.symbol_id);
    stmt.params.iter().for_each(|p| vis.visit_symbol_id(*p));
    stmt.qubits.iter().for_each(|q| vis.visit_symbol_id(*q));
    vis.visit_block(&stmt.body);
}

pub fn walk_qubit_decl(vis: &mut impl Visitor, stmt: &QubitDeclaration) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
}

pub fn walk_qubit_array_decl(vis: &mut impl Visitor, stmt: &QubitArrayDeclaration) {
    vis.visit_span(stmt.span);
    vis.visit_symbol_id(stmt.symbol_id);
    vis.visit_expr(&stmt.size);
    vis.visit_span(stmt.size_span);
}

pub fn walk_reset_stmt(vis: &mut impl Visitor, stmt: &ResetStmt) {
    vis.visit_span(stmt.span);
    vis.visit_span(stmt.reset_token_span);
    vis.visit_gate_operand(&stmt.operand);
}

pub fn walk_return_stmt(vis: &mut impl Visitor, stmt: &ReturnStmt) {
    vis.visit_span(stmt.span);
    stmt.expr.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_switch_stmt(vis: &mut impl Visitor, stmt: &SwitchStmt) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.target);
    stmt.cases.iter().for_each(|c| vis.visit_switch_case(c));
    stmt.default.iter().for_each(|d| vis.visit_block(d));
}

pub fn walk_while_loop(vis: &mut impl Visitor, stmt: &WhileLoop) {
    vis.visit_span(stmt.span);
    vis.visit_expr(&stmt.condition);
    vis.visit_stmt(&stmt.body);
}

pub fn walk_expr(vis: &mut impl Visitor, expr: &Expr) {
    vis.visit_span(expr.span);
    expr.const_value.iter().for_each(|v| vis.visit_literal(v));
    vis.visit_ty(&expr.ty);
    match expr.kind.as_ref() {
        ExprKind::CapturedIdent(id) | ExprKind::Ident(id) => vis.visit_symbol_id(*id),
        ExprKind::UnaryOp(expr) => vis.visit_unary_op_expr(expr),
        ExprKind::BinaryOp(expr) => vis.visit_binary_op_expr(expr),
        ExprKind::Lit(lit) => vis.visit_literal(lit),
        ExprKind::FunctionCall(call) => vis.visit_function_call_expr(call),
        ExprKind::BuiltinFunctionCall(call) => vis.visit_builtin_function_call_expr(call),
        ExprKind::Cast(cast) => vis.visit_cast_expr(cast),
        ExprKind::IndexedExpr(expr) => vis.visit_indexed_expr(expr),
        ExprKind::Paren(expr) => vis.visit_expr(expr),
        ExprKind::Measure(expr) => vis.visit_measure_expr(expr),
        ExprKind::SizeofCall(call) => vis.visit_sizeof_call_expr(call),
        ExprKind::DurationofCall(call) => vis.visit_durationof_call_expr(call),
        ExprKind::Concat(concat) => vis.visit_concat_expr(concat),
        ExprKind::Err => {}
    }
}

pub fn walk_unary_op_expr(vis: &mut impl Visitor, expr: &UnaryOpExpr) {
    vis.visit_span(expr.span);
    vis.visit_unary_op(expr.op);
    vis.visit_expr(&expr.expr);
}

pub fn walk_binary_op_expr(vis: &mut impl Visitor, expr: &BinaryOpExpr) {
    vis.visit_bin_op(expr.op);
    vis.visit_expr(&expr.lhs);
    vis.visit_expr(&expr.rhs);
}

pub fn walk_function_call_expr(vis: &mut impl Visitor, expr: &FunctionCall) {
    vis.visit_span(expr.span);
    vis.visit_span(expr.fn_name_span);
    vis.visit_symbol_id(expr.symbol_id);
    expr.args.iter().for_each(|a| vis.visit_expr(a));
}

pub fn walk_builtin_function_call_expr(vis: &mut impl Visitor, expr: &BuiltinFunctionCall) {
    vis.visit_span(expr.span);
    vis.visit_span(expr.fn_name_span);
    vis.visit_ty(&expr.function_ty);
    expr.args.iter().for_each(|a| vis.visit_expr(a));
}

pub fn walk_cast_expr(vis: &mut impl Visitor, expr: &Cast) {
    vis.visit_span(expr.span);
    vis.visit_ty(&expr.ty);
    expr.ty_exprs.iter().for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&expr.expr);
}

pub fn walk_indexed_expr(vis: &mut impl Visitor, expr: &IndexedExpr) {
    vis.visit_span(expr.span);
    vis.visit_expr(&expr.collection);
    vis.visit_index(&expr.index);
}

pub fn walk_measure_expr(vis: &mut impl Visitor, expr: &MeasureExpr) {
    vis.visit_span(expr.span);
    vis.visit_span(expr.measure_token_span);
    vis.visit_gate_operand(&expr.operand);
}

pub fn walk_sizeof_call_expr(vis: &mut impl Visitor, expr: &SizeofCallExpr) {
    vis.visit_span(expr.span);
    vis.visit_span(expr.fn_name_span);
    vis.visit_expr(&expr.array);
    vis.visit_expr(&expr.dim);
}

pub fn walk_concat_expr(vis: &mut impl Visitor, expr: &ConcatExpr) {
    vis.visit_span(expr.span);
    expr.operands.iter().for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_durationof_call_expr(vis: &mut impl Visitor, expr: &DurationofCallExpr) {
    vis.visit_span(expr.span);
    vis.visit_span(expr.fn_name_span);
    vis.visit_block(&expr.scope);
}

pub fn walk_set(vis: &mut impl Visitor, set: &Set) {
    vis.visit_span(set.span);
    set.values.iter().for_each(|v| vis.visit_expr(v));
}

pub fn walk_range(vis: &mut impl Visitor, range: &Range) {
    vis.visit_span(range.span);
    range.start.iter().for_each(|s| vis.visit_expr(s));
    range.end.iter().for_each(|e| vis.visit_expr(e));
    range.step.iter().for_each(|s| vis.visit_expr(s));
}

pub fn walk_quantum_gate_modifier(vis: &mut impl Visitor, modifier: &QuantumGateModifier) {
    vis.visit_span(modifier.span);
    vis.visit_span(modifier.modifier_keyword_span);
    match &modifier.kind {
        GateModifierKind::Inv => {}
        GateModifierKind::Pow(expr)
        | GateModifierKind::Ctrl(expr)
        | GateModifierKind::NegCtrl(expr) => vis.visit_expr(expr),
    }
}

pub fn walk_gate_operand(vis: &mut impl Visitor, operand: &GateOperand) {
    vis.visit_span(operand.span);
    match &operand.kind {
        GateOperandKind::Expr(expr) => vis.visit_expr(expr),
        GateOperandKind::HardwareQubit(qubit) => vis.visit_hardware_qubit(qubit),
        GateOperandKind::Err => {}
    }
}

pub fn walk_hardware_qubit(vis: &mut impl Visitor, qubit: &HardwareQubit) {
    vis.visit_span(qubit.span);
}

pub fn walk_enumerable_set(vis: &mut impl Visitor, set: &EnumerableSet) {
    match set {
        EnumerableSet::Set(set) => vis.visit_set(set),
        EnumerableSet::Range(range) => vis.visit_range(range),
        EnumerableSet::Expr(expr) => vis.visit_expr(expr),
    }
}

pub fn walk_switch_case(vis: &mut impl Visitor, case: &SwitchCase) {
    vis.visit_span(case.span);
    case.labels.iter().for_each(|l| vis.visit_expr(l));
    vis.visit_block(&case.block);
}

pub fn walk_index(vis: &mut impl Visitor, index: &Index) {
    match index {
        Index::Expr(expr) => vis.visit_expr(expr),
        Index::Range(range) => vis.visit_range(range),
    }
}

pub fn walk_literal(vis: &mut impl Visitor, literal: &LiteralKind) {
    match literal {
        LiteralKind::Array(array) => vis.visit_array(array),
        LiteralKind::Duration(duration) => {
            vis.visit_time_unit(duration.unit);
        }
        // Other literal kinds are terminal and don't need traversal
        LiteralKind::Angle(_)
        | LiteralKind::Bitstring(_, _)
        | LiteralKind::Bool(_)
        | LiteralKind::Float(_)
        | LiteralKind::Complex(_)
        | LiteralKind::Int(_)
        | LiteralKind::BigInt(_)
        | LiteralKind::Bit(_) => {}
    }
}

pub fn walk_array(vis: &mut impl Visitor, array: &Array) {
    array.data.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_path(vis: &mut impl Visitor, path: &Path) {
    if let Some(ref parts) = path.segments {
        vis.visit_idents(parts);
    }
    vis.visit_ident(&path.name);
}

pub fn walk_path_kind(vis: &mut impl Visitor, path: &PathKind) {
    match path {
        PathKind::Ok(path) => vis.visit_path(path),
        PathKind::Err(Some(incomplete_path)) => {
            vis.visit_idents(&incomplete_path.segments);
        }
        PathKind::Err(None) => {}
    }
}

pub fn walk_idents(vis: &mut impl Visitor, idents: &[Ident]) {
    for i in idents {
        vis.visit_ident(i);
    }
}
