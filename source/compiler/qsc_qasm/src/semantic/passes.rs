// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;

use crate::{
    semantic::{
        ast::{
            AliasDeclStmt, Block, BoxStmt, ClassicalDeclarationStmt, DefParameter, DefStmt,
            DelayStmt, Expr, ExprKind, ExternDecl, ForStmt, FunctionCall, GateCall,
            InputDeclaration, OutputDeclaration, Program, QuantumGateDefinition,
            QubitArrayDeclaration, QubitDeclaration,
        },
        symbols::{self, SymbolId},
        visit::{
            Visitor, walk_alias_decl_stmt, walk_classical_decl_stmt, walk_def_param, walk_def_stmt,
            walk_expr, walk_extern_decl, walk_for_stmt, walk_function_call_expr,
            walk_gate_call_stmt, walk_input_declaration, walk_output_declaration,
            walk_quantum_gate_definition, walk_qubit_array_decl, walk_qubit_decl,
        },
    },
    stdlib::duration::Duration,
};

pub struct ReferenceFinder<'a> {
    references: Vec<Span>,
    id: SymbolId,
    symbol_table: &'a symbols::SymbolTable,
}

impl<'a> ReferenceFinder<'a> {
    fn new(id: SymbolId, symbol_table: &'a symbols::SymbolTable) -> Self {
        Self {
            references: Vec::new(),
            id,
            symbol_table,
        }
    }

    /// Visits the block and accumulates the references of all relevant
    /// statements that have a reference.
    /// Returns the total references of all statements in the block.
    #[must_use]
    pub fn get_references(
        scope: &Program,
        id: SymbolId,
        symbol_table: &symbols::SymbolTable,
    ) -> Vec<Span> {
        let mut accumulator = ReferenceFinder::new(id, symbol_table);
        accumulator.visit_program(scope);
        accumulator.references
    }
}

impl Visitor for ReferenceFinder<'_> {
    fn visit_alias_decl_stmt(&mut self, stmt: &AliasDeclStmt) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration
            return;
        }
        walk_alias_decl_stmt(self, stmt);
    }

    fn visit_classical_decl_stmt(&mut self, stmt: &ClassicalDeclarationStmt) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration
            return;
        }
        walk_classical_decl_stmt(self, stmt);
    }

    fn visit_def_stmt(&mut self, stmt: &DefStmt) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
        }
        // function calls can be recursive, so we need to visit the body looking for references
        // to the target id.
        walk_def_stmt(self, stmt);
    }

    fn visit_def_param(&mut self, param: &DefParameter) {
        if self.id == param.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
        }
        walk_def_param(self, param);
    }

    fn visit_extern_decl(&mut self, stmt: &ExternDecl) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration
            return;
        }
        walk_extern_decl(self, stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        if self.id == stmt.loop_variable {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
        }
        walk_for_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        if self.id == stmt.symbol_id {
            self.references.push(stmt.gate_name_span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration
            // as gate calls cannot take gate calls as parameters,
            return;
        }
        walk_gate_call_stmt(self, stmt);
    }

    fn visit_input_declaration(&mut self, stmt: &InputDeclaration) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration.
            return;
        }
        walk_input_declaration(self, stmt);
    }

    fn visit_output_declaration(&mut self, stmt: &OutputDeclaration) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // we just saw that the decl id is this target id,
            // so we can skip visiting the rest of the declaration.
            return;
        }
        walk_output_declaration(self, stmt);
    }

    fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
            // gates can't be recursive, and we just saw that the id is this gate's id,
            // so we can skip visiting the body.
            // This is an optimization to avoid unnecessary recursion.
            return;
        }
        // for params and qubits, we are looking at the original definition,
        // so we use the symbol table to find the symbol and its span.
        stmt.params.iter().for_each(|id| {
            if self.id == *id {
                let symbol = &self.symbol_table[self.id];
                self.references.push(symbol.span);
            }
        });
        stmt.qubits.iter().for_each(|id| {
            if self.id == *id {
                let symbol = &self.symbol_table[self.id];
                self.references.push(symbol.span);
            }
        });
        walk_quantum_gate_definition(self, stmt);
    }

    fn visit_qubit_decl(&mut self, stmt: &QubitDeclaration) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
        }
        walk_qubit_decl(self, stmt);
    }

    fn visit_qubit_array_decl(&mut self, stmt: &QubitArrayDeclaration) {
        if self.id == stmt.symbol_id {
            let symbol = &self.symbol_table[self.id];
            self.references.push(symbol.span);
        }
        walk_qubit_array_decl(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        // we process here rather than visit_symbol_id
        // since we need to push the expr's span.
        match expr.kind.as_ref() {
            ExprKind::CapturedIdent(id) | ExprKind::Ident(id) => {
                if self.id == *id {
                    self.references.push(expr.span);
                }
            }
            _ => {}
        }
        walk_expr(self, expr);
    }

    fn visit_function_call_expr(&mut self, expr: &FunctionCall) {
        if self.id == expr.symbol_id {
            self.references.push(expr.fn_name_span);
        }
        walk_function_call_expr(self, expr);
    }
}

pub struct SymbolFinder<'a> {
    symbol_id: Option<SymbolId>,
    offset: u32,
    symbol_table: &'a symbols::SymbolTable,
}

impl<'a> SymbolFinder<'a> {
    fn new(offset: u32, symbol_table: &'a symbols::SymbolTable) -> Self {
        Self {
            symbol_id: None,
            offset,
            symbol_table,
        }
    }

    #[must_use]
    pub fn get_symbol_at_offset(
        scope: &Program,
        offset: u32,
        symbol_table: &symbols::SymbolTable,
    ) -> Option<SymbolId> {
        let mut accumulator = SymbolFinder::new(offset, symbol_table);
        accumulator.visit_program(scope);
        accumulator.symbol_id
    }
}

impl Visitor for SymbolFinder<'_> {
    fn visit_alias_decl_stmt(&mut self, stmt: &AliasDeclStmt) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_alias_decl_stmt(self, stmt);
    }

    fn visit_classical_decl_stmt(&mut self, stmt: &ClassicalDeclarationStmt) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_classical_decl_stmt(self, stmt);
    }

    fn visit_def_stmt(&mut self, stmt: &DefStmt) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        // function calls can be recursive, so we need to visit the body looking for references
        // to the target id.
        walk_def_stmt(self, stmt);
    }

    fn visit_def_param(&mut self, param: &DefParameter) {
        let symbol = &self.symbol_table[param.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(param.symbol_id);
            return;
        }
        walk_def_param(self, param);
    }

    fn visit_extern_decl(&mut self, stmt: &ExternDecl) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_extern_decl(self, stmt);
    }

    fn visit_for_stmt(&mut self, stmt: &ForStmt) {
        let symbol = &self.symbol_table[stmt.loop_variable];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.loop_variable);
            return;
        }
        walk_for_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        if stmt.gate_name_span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_gate_call_stmt(self, stmt);
    }

    fn visit_input_declaration(&mut self, stmt: &InputDeclaration) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_input_declaration(self, stmt);
    }

    fn visit_output_declaration(&mut self, stmt: &OutputDeclaration) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        walk_output_declaration(self, stmt);
    }

    fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }
        // for params and qubits, we are looking at the original definition,
        // so we use the symbol table to find the symbol and its span.
        stmt.params.iter().for_each(|id| {
            let symbol = &self.symbol_table[*id];
            if symbol.span.touches(self.offset) {
                self.symbol_id = Some(*id);
            }
        });
        stmt.qubits.iter().for_each(|id| {
            let symbol = &self.symbol_table[*id];
            if symbol.span.touches(self.offset) {
                self.symbol_id = Some(*id);
            }
        });
        walk_quantum_gate_definition(self, stmt);
    }

    fn visit_qubit_decl(&mut self, stmt: &QubitDeclaration) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }

        walk_qubit_decl(self, stmt);
    }

    fn visit_qubit_array_decl(&mut self, stmt: &QubitArrayDeclaration) {
        let symbol = &self.symbol_table[stmt.symbol_id];
        if symbol.span.touches(self.offset) {
            self.symbol_id = Some(stmt.symbol_id);
            return;
        }

        walk_qubit_array_decl(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr.kind.as_ref() {
            ExprKind::CapturedIdent(id) | ExprKind::Ident(id) => {
                if expr.span.touches(self.offset) {
                    self.symbol_id = Some(*id);
                    return;
                }
            }
            _ => {}
        }
        walk_expr(self, expr);
    }

    fn visit_function_call_expr(&mut self, expr: &FunctionCall) {
        if expr.fn_name_span.touches(self.offset) {
            self.symbol_id = Some(expr.symbol_id);
            return;
        }
        walk_function_call_expr(self, expr);
    }
}

pub(crate) struct DurationAccumulator {
    durations: Vec<Duration>,
}

impl DurationAccumulator {
    fn new() -> Self {
        Self {
            durations: Vec::new(),
        }
    }

    /// Visits the block and accumulates the durations of all relevant
    /// statements that have a duration.
    /// Returns the total duration of all statements in the block.
    #[must_use]
    pub fn get_duration(scope: &Block) -> Duration {
        let mut accumulator = DurationAccumulator::new();
        accumulator.visit_block(scope);
        accumulator
            .durations
            .into_iter()
            .reduce(|acc, d| acc + d)
            .unwrap_or_default()
    }
}

impl Visitor for DurationAccumulator {
    fn visit_box_stmt(&mut self, stmt: &BoxStmt) {
        if let Some(duration) = &stmt.duration
            && let Some(duration) = duration.get_const_duration()
        {
            self.durations.push(duration);
        }
        super::visit::walk_box_stmt(self, stmt);
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        if let Some(duration) = &stmt.duration
            && let Some(duration) = duration.get_const_duration()
        {
            self.durations.push(duration);
        }
        super::visit::walk_gate_call_stmt(self, stmt);
    }

    fn visit_delay_stmt(&mut self, stmt: &DelayStmt) {
        if let Some(duration) = stmt.duration.get_const_duration() {
            self.durations.push(duration);
        }
        super::visit::walk_delay_stmt(self, stmt);
    }
}
