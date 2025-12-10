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

pub trait MutVisitor: Sized {
    fn visit_program(&mut self, program: &mut Program) {
        walk_program(self, program);
    }

    fn visit_version(&mut self, version: &mut Version) {
        walk_version(self, version);
    }

    fn visit_pragma(&mut self, pragma: &mut Pragma) {
        walk_pragma(self, pragma);
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_annotation(&mut self, annotation: &mut Annotation) {
        walk_annotation(self, annotation);
    }

    fn visit_alias_decl_stmt(&mut self, stmt: &mut AliasDeclStmt) {
        walk_alias_decl_stmt(self, stmt);
    }

    fn visit_assign_stmt(&mut self, stmt: &mut AssignStmt) {
        walk_assign_stmt(self, stmt);
    }

    fn visit_barrier_stmt(&mut self, stmt: &mut BarrierStmt) {
        walk_barrier_stmt(self, stmt);
    }

    fn visit_box_stmt(&mut self, stmt: &mut BoxStmt) {
        walk_box_stmt(self, stmt);
    }

    fn visit_block(&mut self, block: &mut Block) {
        walk_block(self, block);
    }

    fn visit_break_stmt(&mut self, stmt: &mut BreakStmt) {
        walk_break_stmt(self, stmt);
    }

    fn visit_calibration_stmt(&mut self, stmt: &mut CalibrationStmt) {
        walk_calibration_stmt(self, stmt);
    }

    fn visit_calibration_grammar_stmt(&mut self, stmt: &mut CalibrationGrammarStmt) {
        walk_calibration_grammar_stmt(self, stmt);
    }

    fn visit_classical_decl_stmt(&mut self, stmt: &mut ClassicalDeclarationStmt) {
        walk_classical_decl_stmt(self, stmt);
    }

    fn visit_continue_stmt(&mut self, stmt: &mut ContinueStmt) {
        walk_continue_stmt(self, stmt);
    }

    fn visit_def_stmt(&mut self, stmt: &mut DefStmt) {
        walk_def_stmt(self, stmt);
    }

    fn visit_def_param(&mut self, param: &mut DefParameter) {
        walk_def_param(self, param);
    }

    fn visit_def_cal_stmt(&mut self, stmt: &mut DefCalStmt) {
        walk_def_cal_stmt(self, stmt);
    }

    fn visit_delay_stmt(&mut self, stmt: &mut DelayStmt) {
        walk_delay_stmt(self, stmt);
    }

    fn visit_end_stmt(&mut self, stmt: &mut EndStmt) {
        walk_end_stmt(self, stmt);
    }

    fn visit_expr_stmt(&mut self, stmt: &mut ExprStmt) {
        walk_expr_stmt(self, stmt);
    }

    fn visit_extern_decl(&mut self, stmt: &mut ExternDecl) {
        walk_extern_decl(self, stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &mut ForStmt) {
        walk_for_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &mut GateCall) {
        walk_gate_call_stmt(self, stmt);
    }

    fn visit_if_stmt(&mut self, stmt: &mut IfStmt) {
        walk_if_stmt(self, stmt);
    }

    fn visit_include_stmt(&mut self, stmt: &mut IncludeStmt) {
        walk_include_stmt(self, stmt);
    }

    fn visit_indexed_classical_type_assign_stmt(
        &mut self,
        stmt: &mut IndexedClassicalTypeAssignStmt,
    ) {
        walk_indexed_classical_type_assign_stmt(self, stmt);
    }

    fn visit_input_declaration(&mut self, stmt: &mut InputDeclaration) {
        walk_input_declaration(self, stmt);
    }

    fn visit_output_declaration(&mut self, stmt: &mut OutputDeclaration) {
        walk_output_declaration(self, stmt);
    }

    fn visit_measure_arrow_stmt(&mut self, stmt: &mut MeasureArrowStmt) {
        walk_measure_arrow_stmt(self, stmt);
    }

    fn visit_quantum_gate_definition(&mut self, stmt: &mut QuantumGateDefinition) {
        walk_quantum_gate_definition(self, stmt);
    }

    fn visit_qubit_decl(&mut self, stmt: &mut QubitDeclaration) {
        walk_qubit_decl(self, stmt);
    }

    fn visit_qubit_array_decl(&mut self, stmt: &mut QubitArrayDeclaration) {
        walk_qubit_array_decl(self, stmt);
    }

    fn visit_reset_stmt(&mut self, stmt: &mut ResetStmt) {
        walk_reset_stmt(self, stmt);
    }

    fn visit_return_stmt(&mut self, stmt: &mut ReturnStmt) {
        walk_return_stmt(self, stmt);
    }

    fn visit_switch_stmt(&mut self, stmt: &mut SwitchStmt) {
        walk_switch_stmt(self, stmt);
    }

    fn visit_while_loop(&mut self, stmt: &mut WhileLoop) {
        walk_while_loop(self, stmt);
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_expr(self, expr);
    }

    fn visit_unary_op_expr(&mut self, expr: &mut UnaryOpExpr) {
        walk_unary_op_expr(self, expr);
    }

    fn visit_binary_op_expr(&mut self, expr: &mut BinaryOpExpr) {
        walk_binary_op_expr(self, expr);
    }

    fn visit_function_call_expr(&mut self, expr: &mut FunctionCall) {
        walk_function_call_expr(self, expr);
    }

    fn visit_builtin_function_call_expr(&mut self, expr: &mut BuiltinFunctionCall) {
        walk_builtin_function_call_expr(self, expr);
    }

    fn visit_cast_expr(&mut self, expr: &mut Cast) {
        walk_cast_expr(self, expr);
    }

    fn visit_indexed_expr(&mut self, expr: &mut IndexedExpr) {
        walk_indexed_expr(self, expr);
    }

    fn visit_measure_expr(&mut self, expr: &mut MeasureExpr) {
        walk_measure_expr(self, expr);
    }

    fn visit_sizeof_call_expr(&mut self, expr: &mut SizeofCallExpr) {
        walk_sizeof_call_expr(self, expr);
    }

    fn visit_durationof_call_expr(&mut self, expr: &mut DurationofCallExpr) {
        walk_durationof_call_expr(self, expr);
    }

    fn visit_concat_expr(&mut self, expr: &mut ConcatExpr) {
        walk_concat_expr(self, expr);
    }

    fn visit_set(&mut self, set: &mut Set) {
        walk_set(self, set);
    }

    fn visit_range(&mut self, range: &mut Range) {
        walk_range(self, range);
    }

    fn visit_quantum_gate_modifier(&mut self, modifier: &mut QuantumGateModifier) {
        walk_quantum_gate_modifier(self, modifier);
    }

    fn visit_gate_operand(&mut self, operand: &mut GateOperand) {
        walk_gate_operand(self, operand);
    }

    fn visit_hardware_qubit(&mut self, qubit: &mut HardwareQubit) {
        walk_hardware_qubit(self, qubit);
    }

    fn visit_enumerable_set(&mut self, set: &mut EnumerableSet) {
        walk_enumerable_set(self, set);
    }

    fn visit_switch_case(&mut self, case: &mut SwitchCase) {
        walk_switch_case(self, case);
    }

    fn visit_index(&mut self, index: &mut Index) {
        walk_index(self, index);
    }

    fn visit_literal(&mut self, literal: &mut LiteralKind) {
        walk_literal(self, literal);
    }

    fn visit_array(&mut self, array: &mut Array) {
        walk_array(self, array);
    }

    fn visit_path(&mut self, path: &mut Path) {
        walk_path(self, path);
    }

    fn visit_path_kind(&mut self, path: &mut PathKind) {
        walk_path_kind(self, path);
    }

    fn visit_ident(&mut self, ident: &mut Ident) {
        walk_ident(self, ident);
    }

    fn visit_idents(&mut self, ident: &mut [Ident]) {
        walk_idents(self, ident);
    }

    // Terminal nodes that typically don't need further traversal
    fn visit_span(&mut self, _: &mut Span) {}
    fn visit_symbol_id(&mut self, _: &mut SymbolId) {}
    fn visit_bin_op(&mut self, _: &mut BinOp) {}
    fn visit_unary_op(&mut self, _: &mut UnaryOp) {}
    fn visit_time_unit(&mut self, _: &mut TimeUnit) {}
    fn visit_ty(&mut self, _: &mut Type) {}
}

pub fn walk_program(vis: &mut impl MutVisitor, program: &mut Program) {
    program
        .version
        .iter_mut()
        .for_each(|v| vis.visit_version(v));
    program.pragmas.iter_mut().for_each(|p| vis.visit_pragma(p));
    program
        .statements
        .iter_mut()
        .for_each(|s| vis.visit_stmt(s));
}

pub fn walk_version(vis: &mut impl MutVisitor, version: &mut Version) {
    vis.visit_span(&mut version.span);
}

pub fn walk_pragma(vis: &mut impl MutVisitor, pragma: &mut Pragma) {
    vis.visit_span(&mut pragma.span);
    pragma
        .identifier
        .iter_mut()
        .for_each(|id| vis.visit_path_kind(id));
    pragma
        .value_span
        .iter_mut()
        .for_each(|span| vis.visit_span(span));
}

pub fn walk_stmt(vis: &mut impl MutVisitor, stmt: &mut Stmt) {
    vis.visit_span(&mut stmt.span);
    stmt.annotations
        .iter_mut()
        .for_each(|a| vis.visit_annotation(a));
    match stmt.kind.as_mut() {
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

pub fn walk_annotation(vis: &mut impl MutVisitor, annotation: &mut Annotation) {
    vis.visit_span(&mut annotation.span);
    vis.visit_path_kind(&mut annotation.identifier);
    annotation
        .value_span
        .iter_mut()
        .for_each(|span| vis.visit_span(span));
}

pub fn walk_alias_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut AliasDeclStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.exprs.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_assign_stmt(vis: &mut impl MutVisitor, stmt: &mut AssignStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.lhs);
    vis.visit_expr(&mut stmt.rhs);
}

pub fn walk_barrier_stmt(vis: &mut impl MutVisitor, stmt: &mut BarrierStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.qubits
        .iter_mut()
        .for_each(|q| vis.visit_gate_operand(q));
}

pub fn walk_box_stmt(vis: &mut impl MutVisitor, stmt: &mut BoxStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.duration.iter_mut().for_each(|d| vis.visit_expr(d));
    stmt.body.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_block(vis: &mut impl MutVisitor, block: &mut Block) {
    vis.visit_span(&mut block.span);
    block.stmts.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_break_stmt(vis: &mut impl MutVisitor, stmt: &mut BreakStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_calibration_stmt(vis: &mut impl MutVisitor, stmt: &mut CalibrationStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_calibration_grammar_stmt(vis: &mut impl MutVisitor, stmt: &mut CalibrationGrammarStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_classical_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut ClassicalDeclarationStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.ty_span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&mut stmt.init_expr);
}

pub fn walk_continue_stmt(vis: &mut impl MutVisitor, stmt: &mut ContinueStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_def_stmt(vis: &mut impl MutVisitor, stmt: &mut DefStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.params.iter_mut().for_each(|p| vis.visit_def_param(p));
    vis.visit_block(&mut stmt.body);
    vis.visit_span(&mut stmt.return_type_span);
    stmt.return_ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_def_param(vis: &mut impl MutVisitor, param: &mut DefParameter) {
    vis.visit_span(&mut param.span);
    vis.visit_symbol_id(&mut param.symbol_id);
    param
        .ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_def_cal_stmt(vis: &mut impl MutVisitor, stmt: &mut DefCalStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_delay_stmt(vis: &mut impl MutVisitor, stmt: &mut DelayStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.duration);
    stmt.qubits
        .iter_mut()
        .for_each(|q| vis.visit_gate_operand(q));
}

pub fn walk_end_stmt(vis: &mut impl MutVisitor, stmt: &mut EndStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_expr_stmt(vis: &mut impl MutVisitor, stmt: &mut ExprStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.expr);
}

pub fn walk_extern_decl(vis: &mut impl MutVisitor, stmt: &mut ExternDecl) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
    stmt.return_ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_for_stmt(vis: &mut impl MutVisitor, stmt: &mut ForStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.loop_variable);
    stmt.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
    vis.visit_enumerable_set(&mut stmt.set_declaration);
    vis.visit_stmt(&mut stmt.body);
}

pub fn walk_gate_call_stmt(vis: &mut impl MutVisitor, stmt: &mut GateCall) {
    vis.visit_span(&mut stmt.span);
    stmt.modifiers
        .iter_mut()
        .for_each(|m| vis.visit_quantum_gate_modifier(m));
    vis.visit_symbol_id(&mut stmt.symbol_id);
    vis.visit_span(&mut stmt.gate_name_span);
    stmt.args.iter_mut().for_each(|a| vis.visit_expr(a));
    stmt.qubits
        .iter_mut()
        .for_each(|q| vis.visit_gate_operand(q));
    stmt.duration.iter_mut().for_each(|d| vis.visit_expr(d));
}

pub fn walk_if_stmt(vis: &mut impl MutVisitor, stmt: &mut IfStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.condition);
    vis.visit_stmt(&mut stmt.if_body);
    stmt.else_body.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_include_stmt(vis: &mut impl MutVisitor, stmt: &mut IncludeStmt) {
    vis.visit_span(&mut stmt.span);
}

pub fn walk_indexed_classical_type_assign_stmt(
    vis: &mut impl MutVisitor,
    stmt: &mut IndexedClassicalTypeAssignStmt,
) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.lhs);
    stmt.indices.iter_mut().for_each(|i| vis.visit_index(i));
    vis.visit_expr(&mut stmt.rhs);
}

pub fn walk_input_declaration(vis: &mut impl MutVisitor, stmt: &mut InputDeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_output_declaration(vis: &mut impl MutVisitor, stmt: &mut OutputDeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.ty_span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&mut stmt.init_expr);
}

pub fn walk_measure_arrow_stmt(vis: &mut impl MutVisitor, stmt: &mut MeasureArrowStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_measure_expr(&mut stmt.measurement);
    stmt.target.iter_mut().for_each(|t| vis.visit_expr(t));
}

pub fn walk_quantum_gate_definition(vis: &mut impl MutVisitor, stmt: &mut QuantumGateDefinition) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.name_span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    stmt.params.iter_mut().for_each(|p| vis.visit_symbol_id(p));
    stmt.qubits.iter_mut().for_each(|q| vis.visit_symbol_id(q));
    vis.visit_block(&mut stmt.body);
}

pub fn walk_qubit_decl(vis: &mut impl MutVisitor, stmt: &mut QubitDeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
}

pub fn walk_qubit_array_decl(vis: &mut impl MutVisitor, stmt: &mut QubitArrayDeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_symbol_id(&mut stmt.symbol_id);
    vis.visit_expr(&mut stmt.size);
    vis.visit_span(&mut stmt.size_span);
}

pub fn walk_reset_stmt(vis: &mut impl MutVisitor, stmt: &mut ResetStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.reset_token_span);
    vis.visit_gate_operand(&mut stmt.operand);
}

pub fn walk_return_stmt(vis: &mut impl MutVisitor, stmt: &mut ReturnStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.expr.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_switch_stmt(vis: &mut impl MutVisitor, stmt: &mut SwitchStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.target);
    stmt.cases.iter_mut().for_each(|c| vis.visit_switch_case(c));
    stmt.default.iter_mut().for_each(|d| vis.visit_block(d));
}

pub fn walk_while_loop(vis: &mut impl MutVisitor, stmt: &mut WhileLoop) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.condition);
    vis.visit_stmt(&mut stmt.body);
}

pub fn walk_expr(vis: &mut impl MutVisitor, expr: &mut Expr) {
    vis.visit_span(&mut expr.span);
    expr.const_value
        .iter_mut()
        .for_each(|v| vis.visit_literal(v));
    vis.visit_ty(&mut expr.ty);
    match expr.kind.as_mut() {
        ExprKind::CapturedIdent(id) | ExprKind::Ident(id) => vis.visit_symbol_id(id),
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

pub fn walk_unary_op_expr(vis: &mut impl MutVisitor, expr: &mut UnaryOpExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_unary_op(&mut expr.op);
    vis.visit_expr(&mut expr.expr);
}

pub fn walk_binary_op_expr(vis: &mut impl MutVisitor, expr: &mut BinaryOpExpr) {
    vis.visit_bin_op(&mut expr.op);
    vis.visit_expr(&mut expr.lhs);
    vis.visit_expr(&mut expr.rhs);
}

pub fn walk_function_call_expr(vis: &mut impl MutVisitor, expr: &mut FunctionCall) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.fn_name_span);
    vis.visit_symbol_id(&mut expr.symbol_id);
    expr.args.iter_mut().for_each(|a| vis.visit_expr(a));
}

pub fn walk_builtin_function_call_expr(vis: &mut impl MutVisitor, expr: &mut BuiltinFunctionCall) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.fn_name_span);
    vis.visit_ty(&mut expr.function_ty);
    expr.args.iter_mut().for_each(|a| vis.visit_expr(a));
}

pub fn walk_cast_expr(vis: &mut impl MutVisitor, expr: &mut Cast) {
    vis.visit_span(&mut expr.span);
    vis.visit_ty(&mut expr.ty);
    expr.ty_exprs
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
    vis.visit_expr(&mut expr.expr);
}

pub fn walk_indexed_expr(vis: &mut impl MutVisitor, expr: &mut IndexedExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_expr(&mut expr.collection);
    vis.visit_index(&mut expr.index);
}

pub fn walk_measure_expr(vis: &mut impl MutVisitor, expr: &mut MeasureExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.measure_token_span);
    vis.visit_gate_operand(&mut expr.operand);
}

pub fn walk_sizeof_call_expr(vis: &mut impl MutVisitor, expr: &mut SizeofCallExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.fn_name_span);
    vis.visit_expr(&mut expr.array);
    vis.visit_expr(&mut expr.dim);
}

pub fn walk_durationof_call_expr(vis: &mut impl MutVisitor, expr: &mut DurationofCallExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.fn_name_span);
    vis.visit_block(&mut expr.scope);
}

pub fn walk_concat_expr(vis: &mut impl MutVisitor, expr: &mut ConcatExpr) {
    vis.visit_span(&mut expr.span);
    expr.operands
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_set(vis: &mut impl MutVisitor, set: &mut Set) {
    vis.visit_span(&mut set.span);
    set.values.iter_mut().for_each(|v| vis.visit_expr(v));
}

pub fn walk_range(vis: &mut impl MutVisitor, range: &mut Range) {
    vis.visit_span(&mut range.span);
    range.start.iter_mut().for_each(|s| vis.visit_expr(s));
    range.end.iter_mut().for_each(|e| vis.visit_expr(e));
    range.step.iter_mut().for_each(|s| vis.visit_expr(s));
}

pub fn walk_quantum_gate_modifier(vis: &mut impl MutVisitor, modifier: &mut QuantumGateModifier) {
    vis.visit_span(&mut modifier.span);
    vis.visit_span(&mut modifier.modifier_keyword_span);
    match &mut modifier.kind {
        GateModifierKind::Inv => {}
        GateModifierKind::Pow(expr)
        | GateModifierKind::Ctrl(expr)
        | GateModifierKind::NegCtrl(expr) => vis.visit_expr(expr),
    }
}

pub fn walk_gate_operand(vis: &mut impl MutVisitor, operand: &mut GateOperand) {
    vis.visit_span(&mut operand.span);
    match &mut operand.kind {
        GateOperandKind::Expr(expr) => vis.visit_expr(expr),
        GateOperandKind::HardwareQubit(qubit) => vis.visit_hardware_qubit(qubit),
        GateOperandKind::Err => {}
    }
}

pub fn walk_hardware_qubit(vis: &mut impl MutVisitor, qubit: &mut HardwareQubit) {
    vis.visit_span(&mut qubit.span);
}

pub fn walk_enumerable_set(vis: &mut impl MutVisitor, set: &mut EnumerableSet) {
    match set {
        EnumerableSet::Set(set) => vis.visit_set(set),
        EnumerableSet::Range(range) => vis.visit_range(range),
        EnumerableSet::Expr(expr) => vis.visit_expr(expr),
    }
}

pub fn walk_switch_case(vis: &mut impl MutVisitor, case: &mut SwitchCase) {
    vis.visit_span(&mut case.span);
    case.labels.iter_mut().for_each(|l| vis.visit_expr(l));
    vis.visit_block(&mut case.block);
}

pub fn walk_index(vis: &mut impl MutVisitor, index: &mut Index) {
    match index {
        Index::Expr(expr) => vis.visit_expr(expr),
        Index::Range(range) => vis.visit_range(range),
    }
}

pub fn walk_literal(vis: &mut impl MutVisitor, literal: &mut LiteralKind) {
    match literal {
        LiteralKind::Array(array) => vis.visit_array(array),
        LiteralKind::Duration(duration) => {
            vis.visit_time_unit(&mut duration.unit);
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

pub fn walk_array(vis: &mut impl MutVisitor, array: &mut Array) {
    array.data.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_path(vis: &mut impl MutVisitor, path: &mut Path) {
    vis.visit_span(&mut path.span);
    if let Some(ref mut parts) = path.segments {
        vis.visit_idents(parts);
    }
    vis.visit_ident(&mut path.name);
}

pub fn walk_path_kind(vis: &mut impl MutVisitor, path: &mut PathKind) {
    match path {
        PathKind::Ok(path) => vis.visit_path(path),
        PathKind::Err(Some(incomplete_path)) => {
            vis.visit_span(&mut incomplete_path.span);

            for ref mut ident in &mut incomplete_path.segments {
                vis.visit_ident(ident);
            }
        }
        PathKind::Err(None) => {}
    }
}

pub fn walk_ident(vis: &mut impl MutVisitor, ident: &mut Ident) {
    vis.visit_span(&mut ident.span);
}

pub fn walk_idents(vis: &mut impl MutVisitor, idents: &mut [Ident]) {
    for ref mut ident in idents {
        vis.visit_ident(ident);
    }
}
