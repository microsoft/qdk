// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;

use crate::parser::ast::{ConcatExpr, DefParameter, DefParameterType, DurationofCall, QubitType};

use super::ast::{
    AccessControl, AliasDeclStmt, Annotation, ArrayBaseTypeKind, ArrayReferenceType, ArrayType,
    AssignOpStmt, AssignStmt, BarrierStmt, BinOp, BinaryOpExpr, Block, BoxStmt, BreakStmt,
    CalibrationGrammarStmt, CalibrationStmt, Cast, ClassicalDeclarationStmt, ConstantDeclStmt,
    ContinueStmt, DefCalStmt, DefStmt, DelayStmt, EndStmt, EnumerableSet, Expr, ExprStmt,
    ExternDecl, ExternParameter, ForStmt, FunctionCall, GPhase, GateCall, GateModifierKind,
    GateOperand, GateOperandKind, HardwareQubit, IODeclaration, Ident, IdentOrIndexedIdent, IfStmt,
    IncludeStmt, Index, IndexExpr, IndexList, IndexListItem, IndexedIdent, Lit, LiteralKind,
    MeasureArrowStmt, MeasureExpr, Pragma, Program, QuantumGateDefinition, QuantumGateModifier,
    QubitDeclaration, Range, ResetStmt, ReturnStmt, ScalarType, Set, Stmt, StmtKind, SwitchCase,
    SwitchStmt, TypeDef, UnaryOp, UnaryOpExpr, ValueExpr, Version, WhileLoop,
};

pub trait MutVisitor: Sized {
    fn visit_program(&mut self, program: &mut Program) {
        walk_program(self, program);
    }

    fn visit_block(&mut self, block: &mut Block) {
        walk_block(self, block);
    }

    fn visit_annotation(&mut self, annotation: &mut Annotation) {
        walk_annotation(self, annotation);
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_alias_decl_stmt(&mut self, stmt: &mut AliasDeclStmt) {
        walk_alias_decl_stmt(self, stmt);
    }

    fn visit_assign_stmt(&mut self, stmt: &mut AssignStmt) {
        walk_assign_stmt(self, stmt);
    }

    fn visit_assign_op_stmt(&mut self, stmt: &mut AssignOpStmt) {
        walk_assign_op_stmt(self, stmt);
    }

    fn visit_barrier_stmt(&mut self, stmt: &mut BarrierStmt) {
        walk_barrier_stmt(self, stmt);
    }

    fn visit_box_stmt(&mut self, stmt: &mut BoxStmt) {
        walk_box_stmt(self, stmt);
    }

    fn visit_break_stmt(&mut self, stmt: &mut BreakStmt) {
        walk_break_stmt(self, stmt);
    }

    fn visit_block_stmt(&mut self, stmt: &mut Block) {
        walk_block_stmt(self, stmt);
    }

    fn visit_cal_stmt(&mut self, stmt: &mut CalibrationStmt) {
        walk_cal_stmt(self, stmt);
    }

    fn visit_calibration_grammar_stmt(&mut self, stmt: &mut CalibrationGrammarStmt) {
        walk_calibration_grammar_stmt(self, stmt);
    }

    fn visit_classical_decl_stmt(&mut self, stmt: &mut ClassicalDeclarationStmt) {
        walk_classical_decl_stmt(self, stmt);
    }

    fn visit_const_decl_stmt(&mut self, stmt: &mut ConstantDeclStmt) {
        walk_const_decl_stmt(self, stmt);
    }

    fn visit_continue_stmt(&mut self, stmt: &mut ContinueStmt) {
        walk_continue_stmt(self, stmt);
    }

    fn visit_def_stmt(&mut self, stmt: &mut DefStmt) {
        walk_def_stmt(self, stmt);
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

    fn visit_extern_decl_stmt(&mut self, stmt: &mut ExternDecl) {
        walk_extern_stmt(self, stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &mut ForStmt) {
        walk_for_stmt(self, stmt);
    }

    fn visit_if_stmt(&mut self, stmt: &mut IfStmt) {
        walk_if_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &mut GateCall) {
        walk_gate_call_stmt(self, stmt);
    }

    fn visit_gphase_stmt(&mut self, stmt: &mut GPhase) {
        walk_gphase_stmt(self, stmt);
    }

    fn visit_include_stmt(&mut self, stmt: &mut IncludeStmt) {
        walk_include_stmt(self, stmt);
    }

    fn visit_io_declaration_stmt(&mut self, stmt: &mut IODeclaration) {
        walk_io_declaration_stmt(self, stmt);
    }

    fn visit_measure_stmt(&mut self, stmt: &mut MeasureArrowStmt) {
        walk_measure_stmt(self, stmt);
    }

    fn visit_pragma_stmt(&mut self, stmt: &mut Pragma) {
        walk_pragma_stmt(self, stmt);
    }

    fn visit_quantum_gate_definition_stmt(&mut self, stmt: &mut QuantumGateDefinition) {
        walk_quantum_gate_definition_stmt(self, stmt);
    }

    fn visit_quantum_decl_stmt(&mut self, stmt: &mut QubitDeclaration) {
        walk_quantum_decl_stmt(self, stmt);
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

    fn visit_while_loop_stmt(&mut self, stmt: &mut WhileLoop) {
        walk_while_loop_stmt(self, stmt);
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

    fn visit_lit_expr(&mut self, expr: &mut Lit) {
        walk_lit_expr(self, expr);
    }

    fn visit_function_call_expr(&mut self, expr: &mut FunctionCall) {
        walk_function_call_expr(self, expr);
    }

    fn visit_cast_expr(&mut self, expr: &mut Cast) {
        walk_cast_expr(self, expr);
    }

    fn visit_duration_of_expr(&mut self, expr: &mut DurationofCall) {
        walk_duration_of_expr(self, expr);
    }

    fn visit_index_expr(&mut self, expr: &mut IndexExpr) {
        walk_index_expr(self, expr);
    }

    fn visit_value_expr(&mut self, expr: &mut ValueExpr) {
        walk_value_expr(self, expr);
    }

    fn visit_measure_expr(&mut self, expr: &mut MeasureExpr) {
        walk_measure_expr(self, expr);
    }

    fn visit_concat_expr(&mut self, expr: &mut ConcatExpr) {
        walk_concat_expr(self, expr);
    }

    fn visit_ident_or_indexed_ident(&mut self, ident: &mut IdentOrIndexedIdent) {
        walk_ident_or_indexed_ident(self, ident);
    }

    fn visit_indexed_ident(&mut self, ident: &mut IndexedIdent) {
        walk_indexed_ident(self, ident);
    }

    fn visit_ident(&mut self, ident: &mut Ident) {
        walk_ident(self, ident);
    }

    fn visit_index(&mut self, elem: &mut Index) {
        walk_index(self, elem);
    }

    fn visit_set(&mut self, set: &mut Set) {
        walk_set(self, set);
    }

    fn visit_index_list(&mut self, set: &mut IndexList) {
        walk_index_list(self, set);
    }

    fn visit_index_list_item(&mut self, item: &mut IndexListItem) {
        walk_index_list_item(self, item);
    }

    fn visit_range(&mut self, range: &mut Range) {
        walk_range(self, range);
    }

    fn visit_gate_operand(&mut self, operand: &mut GateOperand) {
        walk_gate_operand(self, operand);
    }

    fn visit_hardware_qubit(&mut self, qubit: &mut HardwareQubit) {
        walk_hardware_qubit(self, qubit);
    }

    fn visit_tydef(&mut self, ty: &mut TypeDef) {
        walk_tydef(self, ty);
    }

    fn visit_array_type(&mut self, ty: &mut ArrayType) {
        walk_array_type(self, ty);
    }

    fn visit_array_ref_type(&mut self, ty: &mut ArrayReferenceType) {
        walk_array_ref_type(self, ty);
    }

    fn visit_array_base_type(&mut self, ty: &mut ArrayBaseTypeKind) {
        walk_array_base_type(self, ty);
    }

    fn visit_scalar_type(&mut self, ty: &mut ScalarType) {
        walk_scalar_type(self, ty);
    }

    fn visit_qubit_type(&mut self, ty: &mut QubitType) {
        walk_qubit_type(self, ty);
    }

    fn visit_def_parameter(&mut self, param: &mut DefParameter) {
        walk_def_parameter(self, param);
    }

    fn visit_extern_parameter(&mut self, param: &mut ExternParameter) {
        walk_extern_parameter(self, param);
    }

    fn visit_enumerable_set(&mut self, set: &mut EnumerableSet) {
        walk_enumerable_set(self, set);
    }

    fn visit_gate_modifier(&mut self, set: &mut QuantumGateModifier) {
        walk_gate_modifier(self, set);
    }

    fn visit_switch_case(&mut self, case: &mut SwitchCase) {
        walk_switch_case(self, case);
    }

    fn visit_access_control(&mut self, _: &mut AccessControl) {}

    fn visit_version(&mut self, _: &mut Version) {}

    fn visit_span(&mut self, _: &mut Span) {}

    fn visit_binop(&mut self, _: &mut BinOp) {}

    fn visit_unary_op(&mut self, _: &mut UnaryOp) {}
}

pub fn walk_program(vis: &mut impl MutVisitor, program: &mut Program) {
    vis.visit_span(&mut program.span);
    program
        .version
        .iter_mut()
        .for_each(|v| vis.visit_version(v));
    program
        .statements
        .iter_mut()
        .for_each(|s| vis.visit_stmt(s));
}

pub fn walk_block(vis: &mut impl MutVisitor, block: &mut Block) {
    vis.visit_span(&mut block.span);
    block.stmts.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_annotation(vis: &mut impl MutVisitor, annotation: &mut Annotation) {
    vis.visit_span(&mut annotation.span);
}

pub fn walk_stmt(vis: &mut impl MutVisitor, stmt: &mut Stmt) {
    vis.visit_span(&mut stmt.span);
    stmt.annotations
        .iter_mut()
        .for_each(|s| vis.visit_annotation(s));
    match &mut *stmt.kind {
        StmtKind::Err => {}
        StmtKind::Alias(alias_decl_stmt) => vis.visit_alias_decl_stmt(alias_decl_stmt),
        StmtKind::Assign(assign_stmt) => vis.visit_assign_stmt(assign_stmt),
        StmtKind::AssignOp(assign_op_stmt) => vis.visit_assign_op_stmt(assign_op_stmt),
        StmtKind::Barrier(barrier_stmt) => vis.visit_barrier_stmt(barrier_stmt),
        StmtKind::Box(box_stmt) => vis.visit_box_stmt(box_stmt),
        StmtKind::Break(break_stmt) => vis.visit_break_stmt(break_stmt),
        StmtKind::Block(block) => vis.visit_block_stmt(block),
        StmtKind::Cal(calibration_stmt) => vis.visit_cal_stmt(calibration_stmt),
        StmtKind::CalibrationGrammar(calibration_grammar_stmt) => {
            vis.visit_calibration_grammar_stmt(calibration_grammar_stmt);
        }
        StmtKind::ClassicalDecl(classical_declaration_stmt) => {
            vis.visit_classical_decl_stmt(classical_declaration_stmt);
        }
        StmtKind::ConstDecl(constant_decl_stmt) => vis.visit_const_decl_stmt(constant_decl_stmt),
        StmtKind::Continue(continue_stmt) => vis.visit_continue_stmt(continue_stmt),
        StmtKind::Def(def_stmt) => vis.visit_def_stmt(def_stmt),
        StmtKind::DefCal(def_cal_stmt) => vis.visit_def_cal_stmt(def_cal_stmt),
        StmtKind::Delay(delay_stmt) => vis.visit_delay_stmt(delay_stmt),
        StmtKind::End(end_stmt) => vis.visit_end_stmt(end_stmt),
        StmtKind::ExprStmt(expr_stmt) => vis.visit_expr_stmt(expr_stmt),
        StmtKind::ExternDecl(extern_decl) => vis.visit_extern_decl_stmt(extern_decl),
        StmtKind::For(for_stmt) => vis.visit_for_stmt(for_stmt),
        StmtKind::If(if_stmt) => vis.visit_if_stmt(if_stmt),
        StmtKind::GateCall(gate_call) => vis.visit_gate_call_stmt(gate_call),
        StmtKind::GPhase(gphase) => vis.visit_gphase_stmt(gphase),
        StmtKind::Include(include_stmt) => vis.visit_include_stmt(include_stmt),
        StmtKind::IODeclaration(iodeclaration) => vis.visit_io_declaration_stmt(iodeclaration),
        StmtKind::Measure(measure_stmt) => vis.visit_measure_stmt(measure_stmt),
        StmtKind::Pragma(pragma) => vis.visit_pragma_stmt(pragma),
        StmtKind::QuantumGateDefinition(quantum_gate_definition) => {
            vis.visit_quantum_gate_definition_stmt(quantum_gate_definition);
        }
        StmtKind::QuantumDecl(qubit_declaration) => vis.visit_quantum_decl_stmt(qubit_declaration),
        StmtKind::Reset(reset_stmt) => vis.visit_reset_stmt(reset_stmt),
        StmtKind::Return(return_stmt) => vis.visit_return_stmt(return_stmt),
        StmtKind::Switch(switch_stmt) => vis.visit_switch_stmt(switch_stmt),
        StmtKind::WhileLoop(while_loop) => vis.visit_while_loop_stmt(while_loop),
    }
}

fn walk_alias_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut AliasDeclStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident_or_indexed_ident(&mut stmt.ident);
    stmt.exprs.iter_mut().for_each(|e| vis.visit_expr(e));
}

fn walk_assign_stmt(vis: &mut impl MutVisitor, stmt: &mut AssignStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident_or_indexed_ident(&mut stmt.lhs);
    vis.visit_value_expr(&mut stmt.rhs);
}

fn walk_assign_op_stmt(vis: &mut impl MutVisitor, stmt: &mut AssignOpStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident_or_indexed_ident(&mut stmt.lhs);
    vis.visit_binop(&mut stmt.op);
    vis.visit_value_expr(&mut stmt.rhs);
}

fn walk_barrier_stmt(vis: &mut impl MutVisitor, stmt: &mut BarrierStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.qubits
        .iter_mut()
        .for_each(|operand| vis.visit_gate_operand(operand));
}

fn walk_box_stmt(vis: &mut impl MutVisitor, stmt: &mut BoxStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.duration.iter_mut().for_each(|d| vis.visit_expr(d));
    stmt.body.iter_mut().for_each(|stmt| vis.visit_stmt(stmt));
}

fn walk_break_stmt(vis: &mut impl MutVisitor, stmt: &mut BreakStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_block_stmt(vis: &mut impl MutVisitor, stmt: &mut Block) {
    vis.visit_span(&mut stmt.span);
    stmt.stmts.iter_mut().for_each(|stmt| vis.visit_stmt(stmt));
}

fn walk_cal_stmt(vis: &mut impl MutVisitor, stmt: &mut CalibrationStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_calibration_grammar_stmt(vis: &mut impl MutVisitor, stmt: &mut CalibrationGrammarStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_classical_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut ClassicalDeclarationStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_tydef(&mut stmt.ty);
    vis.visit_ident(&mut stmt.identifier);
    stmt.init_expr
        .iter_mut()
        .for_each(|e| vis.visit_value_expr(e));
}

fn walk_const_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut ConstantDeclStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_tydef(&mut stmt.ty);
    vis.visit_ident(&mut stmt.identifier);
    vis.visit_value_expr(&mut stmt.init_expr);
}

fn walk_continue_stmt(vis: &mut impl MutVisitor, stmt: &mut ContinueStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_def_stmt(vis: &mut impl MutVisitor, stmt: &mut DefStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident(&mut stmt.name);
    stmt.params
        .iter_mut()
        .for_each(|p| vis.visit_def_parameter(p));
    stmt.return_type
        .iter_mut()
        .for_each(|ty| vis.visit_scalar_type(ty));
    vis.visit_block(&mut stmt.body);
}

fn walk_def_cal_stmt(vis: &mut impl MutVisitor, stmt: &mut DefCalStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_delay_stmt(vis: &mut impl MutVisitor, stmt: &mut DelayStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.duration);
    stmt.qubits
        .iter_mut()
        .for_each(|operand| vis.visit_gate_operand(operand));
}

fn walk_end_stmt(vis: &mut impl MutVisitor, stmt: &mut EndStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_expr_stmt(vis: &mut impl MutVisitor, stmt: &mut ExprStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.expr);
}

fn walk_extern_stmt(vis: &mut impl MutVisitor, stmt: &mut ExternDecl) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident(&mut stmt.ident);
    stmt.params
        .iter_mut()
        .for_each(|p| vis.visit_extern_parameter(p));
    stmt.return_type
        .iter_mut()
        .for_each(|ty| vis.visit_scalar_type(ty));
}

fn walk_for_stmt(vis: &mut impl MutVisitor, stmt: &mut ForStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_scalar_type(&mut stmt.ty);
    vis.visit_ident(&mut stmt.ident);
    vis.visit_enumerable_set(&mut stmt.set_declaration);
    vis.visit_stmt(&mut stmt.body);
}

fn walk_if_stmt(vis: &mut impl MutVisitor, stmt: &mut IfStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.condition);
    vis.visit_stmt(&mut stmt.if_body);
    stmt.else_body
        .iter_mut()
        .for_each(|else_body| vis.visit_stmt(else_body));
}

fn walk_gate_call_stmt(vis: &mut impl MutVisitor, stmt: &mut GateCall) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident(&mut stmt.name);
    stmt.modifiers
        .iter_mut()
        .for_each(|m| vis.visit_gate_modifier(m));
    stmt.args.iter_mut().for_each(|arg| vis.visit_expr(arg));
    stmt.duration.iter_mut().for_each(|d| vis.visit_expr(d));
    stmt.qubits
        .iter_mut()
        .for_each(|q| vis.visit_gate_operand(q));
}

fn walk_gphase_stmt(vis: &mut impl MutVisitor, stmt: &mut GPhase) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.gphase_token_span);
    stmt.modifiers
        .iter_mut()
        .for_each(|m| vis.visit_gate_modifier(m));
    stmt.args.iter_mut().for_each(|arg| vis.visit_expr(arg));
    stmt.duration.iter_mut().for_each(|d| vis.visit_expr(d));
    stmt.qubits
        .iter_mut()
        .for_each(|q| vis.visit_gate_operand(q));
}

fn walk_include_stmt(vis: &mut impl MutVisitor, stmt: &mut IncludeStmt) {
    vis.visit_span(&mut stmt.span);
}

fn walk_io_declaration_stmt(vis: &mut impl MutVisitor, stmt: &mut IODeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_tydef(&mut stmt.ty);
    vis.visit_ident(&mut stmt.ident);
}

fn walk_measure_stmt(vis: &mut impl MutVisitor, stmt: &mut MeasureArrowStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.target
        .iter_mut()
        .for_each(|t| vis.visit_ident_or_indexed_ident(t));
    vis.visit_measure_expr(&mut stmt.measurement);
}

fn walk_pragma_stmt(vis: &mut impl MutVisitor, stmt: &mut Pragma) {
    vis.visit_span(&mut stmt.span);
}

fn walk_quantum_gate_definition_stmt(vis: &mut impl MutVisitor, stmt: &mut QuantumGateDefinition) {
    vis.visit_span(&mut stmt.span);
    vis.visit_ident(&mut stmt.ident);
    stmt.params.iter_mut().for_each(|p| match &mut **p {
        super::prim::SeqItem::Item(i) => vis.visit_ident(i),
        super::prim::SeqItem::Missing(span) => vis.visit_span(span),
    });
    stmt.qubits.iter_mut().for_each(|p| match &mut **p {
        super::prim::SeqItem::Item(i) => vis.visit_ident(i),
        super::prim::SeqItem::Missing(span) => vis.visit_span(span),
    });
    vis.visit_block(&mut stmt.body);
}

fn walk_quantum_decl_stmt(vis: &mut impl MutVisitor, stmt: &mut QubitDeclaration) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.ty.span);
    vis.visit_ident(&mut stmt.qubit);
    stmt.ty.size.iter_mut().for_each(|s| vis.visit_expr(s));
}

fn walk_reset_stmt(vis: &mut impl MutVisitor, stmt: &mut ResetStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_span(&mut stmt.reset_token_span);
    vis.visit_gate_operand(&mut stmt.operand);
}

fn walk_return_stmt(vis: &mut impl MutVisitor, stmt: &mut ReturnStmt) {
    vis.visit_span(&mut stmt.span);
    stmt.expr.iter_mut().for_each(|e| vis.visit_value_expr(e));
}

fn walk_switch_stmt(vis: &mut impl MutVisitor, stmt: &mut SwitchStmt) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.target);
    stmt.cases.iter_mut().for_each(|c| vis.visit_switch_case(c));
    stmt.default.iter_mut().for_each(|d| vis.visit_block(d));
}

fn walk_while_loop_stmt(vis: &mut impl MutVisitor, stmt: &mut WhileLoop) {
    vis.visit_span(&mut stmt.span);
    vis.visit_expr(&mut stmt.while_condition);
    vis.visit_stmt(&mut stmt.body);
}

fn walk_switch_case(vis: &mut impl MutVisitor, case: &mut SwitchCase) {
    vis.visit_span(&mut case.span);
    case.labels.iter_mut().for_each(|l| vis.visit_expr(l));
    vis.visit_block(&mut case.block);
}

pub fn walk_expr(vis: &mut impl MutVisitor, expr: &mut Expr) {
    vis.visit_span(&mut expr.span);

    match &mut *expr.kind {
        super::ast::ExprKind::Err => {}
        super::ast::ExprKind::Ident(ident) => vis.visit_ident(ident),
        super::ast::ExprKind::UnaryOp(unary_op_expr) => vis.visit_unary_op_expr(unary_op_expr),
        super::ast::ExprKind::BinaryOp(binary_op_expr) => vis.visit_binary_op_expr(binary_op_expr),
        super::ast::ExprKind::Lit(lit) => vis.visit_lit_expr(lit),
        super::ast::ExprKind::FunctionCall(function_call) => {
            vis.visit_function_call_expr(function_call);
        }
        super::ast::ExprKind::Cast(cast) => vis.visit_cast_expr(cast),
        super::ast::ExprKind::IndexExpr(index_expr) => vis.visit_index_expr(index_expr),
        super::ast::ExprKind::Paren(expr) => vis.visit_expr(expr),
        super::ast::ExprKind::DurationOf(expr) => vis.visit_duration_of_expr(expr),
    }
}

pub fn walk_unary_op_expr(vis: &mut impl MutVisitor, expr: &mut UnaryOpExpr) {
    vis.visit_unary_op(&mut expr.op);
    vis.visit_expr(&mut expr.expr);
}

pub fn walk_binary_op_expr(vis: &mut impl MutVisitor, expr: &mut BinaryOpExpr) {
    vis.visit_expr(&mut expr.lhs);
    vis.visit_binop(&mut expr.op);
    vis.visit_expr(&mut expr.rhs);
}

pub fn walk_lit_expr(vis: &mut impl MutVisitor, lit: &mut Lit) {
    vis.visit_span(&mut lit.span);
    if let LiteralKind::Array(exprs) = &mut lit.kind {
        exprs.iter_mut().for_each(|e| vis.visit_expr(e));
    }
}

pub fn walk_function_call_expr(vis: &mut impl MutVisitor, expr: &mut FunctionCall) {
    vis.visit_span(&mut expr.span);
    vis.visit_ident(&mut expr.name);
    expr.args.iter_mut().for_each(|arg| vis.visit_expr(arg));
}

pub fn walk_cast_expr(vis: &mut impl MutVisitor, expr: &mut Cast) {
    vis.visit_span(&mut expr.span);
    vis.visit_tydef(&mut expr.ty);
    vis.visit_expr(&mut expr.arg);
}

pub fn walk_duration_of_expr(vis: &mut impl MutVisitor, expr: &mut DurationofCall) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.name_span);
    vis.visit_block(&mut expr.scope);
}

pub fn walk_index_expr(vis: &mut impl MutVisitor, expr: &mut IndexExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_expr(&mut expr.collection);
    vis.visit_index(&mut expr.index);
}

pub fn walk_value_expr(vis: &mut impl MutVisitor, expr: &mut ValueExpr) {
    match &mut *expr {
        ValueExpr::Concat(expr) => vis.visit_concat_expr(expr),
        ValueExpr::Expr(expr) => vis.visit_expr(expr),
        ValueExpr::Measurement(measure_expr) => vis.visit_measure_expr(measure_expr),
    }
}

pub fn walk_measure_expr(vis: &mut impl MutVisitor, expr: &mut MeasureExpr) {
    vis.visit_span(&mut expr.span);
    vis.visit_span(&mut expr.measure_token_span);
    vis.visit_gate_operand(&mut expr.operand);
}

pub fn walk_concat_expr(vis: &mut impl MutVisitor, expr: &mut ConcatExpr) {
    vis.visit_span(&mut expr.span);
    expr.operands
        .iter_mut()
        .for_each(|expr| vis.visit_expr(expr));
}

pub fn walk_ident_or_indexed_ident(vis: &mut impl MutVisitor, ident: &mut IdentOrIndexedIdent) {
    match ident {
        IdentOrIndexedIdent::Ident(ident) => vis.visit_ident(ident),
        IdentOrIndexedIdent::IndexedIdent(indexed_ident) => vis.visit_indexed_ident(indexed_ident),
    }
}

pub fn walk_indexed_ident(vis: &mut impl MutVisitor, ident: &mut IndexedIdent) {
    vis.visit_span(&mut ident.span);
    vis.visit_ident(&mut ident.ident);
    ident
        .indices
        .iter_mut()
        .for_each(|elem| vis.visit_index(elem));
}

pub fn walk_ident(vis: &mut impl MutVisitor, ident: &mut Ident) {
    vis.visit_span(&mut ident.span);
}

pub fn walk_index(vis: &mut impl MutVisitor, elem: &mut Index) {
    match elem {
        Index::IndexSet(discrete_set) => vis.visit_set(discrete_set),
        Index::IndexList(index_set) => vis.visit_index_list(index_set),
    }
}

pub fn walk_set(vis: &mut impl MutVisitor, set: &mut Set) {
    vis.visit_span(&mut set.span);
    set.values.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_index_list(vis: &mut impl MutVisitor, set: &mut IndexList) {
    vis.visit_span(&mut set.span);
    set.values
        .iter_mut()
        .for_each(|item| vis.visit_index_list_item(item));
}

pub fn walk_index_list_item(vis: &mut impl MutVisitor, item: &mut IndexListItem) {
    match item {
        IndexListItem::RangeDefinition(range_definition) => {
            vis.visit_range(range_definition);
        }
        IndexListItem::Expr(expr) => vis.visit_expr(expr),
        IndexListItem::Err => {}
    }
}

pub fn walk_gate_operand(vis: &mut impl MutVisitor, operand: &mut GateOperand) {
    vis.visit_span(&mut operand.span);
    match &mut operand.kind {
        GateOperandKind::IdentOrIndexedIdent(ident) => vis.visit_ident_or_indexed_ident(ident),
        GateOperandKind::HardwareQubit(hardware_qubit) => vis.visit_hardware_qubit(hardware_qubit),
        GateOperandKind::Err => {}
    }
}

pub fn walk_tydef(vis: &mut impl MutVisitor, ty: &mut TypeDef) {
    match ty {
        TypeDef::Array(array) => vis.visit_array_type(array),
        TypeDef::ArrayReference(array_ref) => vis.visit_array_ref_type(array_ref),
        TypeDef::Scalar(scalar) => vis.visit_scalar_type(scalar),
    }
}

pub fn walk_array_type(vis: &mut impl MutVisitor, ty: &mut ArrayType) {
    vis.visit_span(&mut ty.span);
    vis.visit_array_base_type(&mut ty.base_type);
    ty.dimensions.iter_mut().for_each(|d| vis.visit_expr(d));
}

pub fn walk_array_base_type(vis: &mut impl MutVisitor, ty: &mut ArrayBaseTypeKind) {
    match ty {
        ArrayBaseTypeKind::Int(ty) => vis.visit_span(&mut ty.span),
        ArrayBaseTypeKind::UInt(ty) => vis.visit_span(&mut ty.span),
        ArrayBaseTypeKind::Float(ty) => vis.visit_span(&mut ty.span),
        ArrayBaseTypeKind::Complex(ty) => vis.visit_span(&mut ty.span),
        ArrayBaseTypeKind::Angle(ty) => vis.visit_span(&mut ty.span),
        _ => {}
    }
}

pub fn walk_array_ref_type(vis: &mut impl MutVisitor, ty: &mut ArrayReferenceType) {
    match ty {
        ArrayReferenceType::Dyn(ty) => {
            vis.visit_span(&mut ty.span);
            vis.visit_access_control(&mut ty.mutability);
            vis.visit_array_base_type(&mut ty.base_type);
            vis.visit_expr(&mut ty.dimensions);
        }
        ArrayReferenceType::Static(ty) => {
            vis.visit_span(&mut ty.span);
            vis.visit_access_control(&mut ty.mutability);
            vis.visit_array_base_type(&mut ty.base_type);
            ty.dimensions.iter_mut().for_each(|d| vis.visit_expr(d));
        }
    }
}

pub fn walk_scalar_type(vis: &mut impl MutVisitor, ty: &mut ScalarType) {
    vis.visit_span(&mut ty.span);
    match &mut ty.kind {
        super::ast::ScalarTypeKind::Bit(ty) => vis.visit_span(&mut ty.span),
        super::ast::ScalarTypeKind::Int(ty) => vis.visit_span(&mut ty.span),
        super::ast::ScalarTypeKind::UInt(ty) => vis.visit_span(&mut ty.span),
        super::ast::ScalarTypeKind::Float(ty) => vis.visit_span(&mut ty.span),
        super::ast::ScalarTypeKind::Complex(ty) => vis.visit_span(&mut ty.span),
        super::ast::ScalarTypeKind::Angle(ty) => vis.visit_span(&mut ty.span),
        _ => {}
    }
}

pub fn walk_qubit_type(vis: &mut impl MutVisitor, ty: &mut QubitType) {
    vis.visit_span(&mut ty.span);
    ty.size.iter_mut().for_each(|s| vis.visit_expr(s));
}

pub fn walk_def_parameter(vis: &mut impl MutVisitor, param: &mut DefParameter) {
    vis.visit_span(&mut param.span);
    vis.visit_ident(&mut param.ident);
    match &mut *param.ty {
        DefParameterType::ArrayReference(ty) => {
            vis.visit_array_ref_type(ty);
        }
        DefParameterType::Qubit(ty) => {
            vis.visit_qubit_type(ty);
        }
        DefParameterType::Scalar(ty) => {
            vis.visit_scalar_type(ty);
        }
    }
}

pub fn walk_extern_parameter(vis: &mut impl MutVisitor, param: &mut ExternParameter) {
    match param {
        ExternParameter::ArrayReference(ty, span) => {
            vis.visit_span(span);
            vis.visit_array_ref_type(ty);
        }
        ExternParameter::Scalar(ty, span) => {
            vis.visit_span(span);
            vis.visit_scalar_type(ty);
        }
    }
}

pub fn walk_enumerable_set(vis: &mut impl MutVisitor, set: &mut EnumerableSet) {
    match set {
        EnumerableSet::Set(set) => vis.visit_set(set),
        EnumerableSet::Range(range_definition) => {
            vis.visit_range(range_definition);
        }
        EnumerableSet::Expr(expr) => vis.visit_expr(expr),
    }
}

pub fn walk_gate_modifier(vis: &mut impl MutVisitor, modifier: &mut QuantumGateModifier) {
    vis.visit_span(&mut modifier.span);
    vis.visit_span(&mut modifier.modifier_keyword_span);
    match &mut modifier.kind {
        GateModifierKind::Inv => {}
        GateModifierKind::Pow(expr) => vis.visit_expr(expr),
        GateModifierKind::Ctrl(expr) => expr.iter_mut().for_each(|e| vis.visit_expr(e)),
        GateModifierKind::NegCtrl(expr) => expr.iter_mut().for_each(|e| vis.visit_expr(e)),
    }
}

pub fn walk_hardware_qubit(vis: &mut impl MutVisitor, operand: &mut HardwareQubit) {
    vis.visit_span(&mut operand.span);
}

pub fn walk_range(vis: &mut impl MutVisitor, range: &mut Range) {
    vis.visit_span(&mut range.span);
    range.start.iter_mut().for_each(|s| vis.visit_expr(s));
    range.step.iter_mut().for_each(|s| vis.visit_expr(s));
    range.end.iter_mut().for_each(|s| vis.visit_expr(s));
}
