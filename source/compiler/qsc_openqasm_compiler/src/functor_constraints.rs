// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Functor Constraint Solver Pass
//!
//! This module implements a visitor-based pass that analyzes gate calls in an `OpenQASM`
//! program to determine which functors (Adjoint, Controlled) need to be supported by
//! each gate definition.
//!
//! When a gate is called with modifiers like `inv @` (inverse) or `ctrl @` (controlled),
//! the corresponding Q# operation needs to declare support for the `Adj` and `Ctl` functors
//! respectively. This pass walks through the entire program, collects all gate calls with
//! their modifiers, and builds a map from gate symbol IDs to the required functor constraints.

use rustc_hash::{FxHashMap, FxHashSet};

use qsc_openqasm_parser::semantic::{
    ast::{GateCall, GateModifierKind, Program, QuantumGateDefinition},
    symbols::SymbolId,
    visit::{Visitor, walk_gate_call_stmt, walk_quantum_gate_definition},
};

/// Represents the functor constraints that a gate must satisfy based on how it's called.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctorConstraints {
    /// Whether the gate requires adjoint (Adj) support (from `inv @` modifier).
    pub requires_adj: bool,
    /// Whether the gate requires controlled (Ctl) support (from `ctrl @` or `negctrl @` modifier).
    pub requires_ctl: bool,
}

impl FunctorConstraints {
    /// Returns true if any functor constraints are required.
    #[must_use]
    pub fn any(&self) -> bool {
        self.requires_adj || self.requires_ctl
    }

    /// Returns true if no functor constraints are required.
    #[must_use]
    pub fn none(&self) -> bool {
        !self.any()
    }

    /// Merge another set of constraints into this one.
    pub fn merge(&mut self, other: &FunctorConstraints) {
        self.requires_adj = self.requires_adj || other.requires_adj;
        self.requires_ctl = self.requires_ctl || other.requires_ctl;
    }
}

/// A visitor-based pass that collects functor constraints for gate definitions.
///
/// This pass traverses the program and for each gate call:
/// - Analyzes the modifiers applied to the call
/// - Records the required functors for the called gate's symbol ID
///
/// The pass also handles nested gate calls within gate definitions, propagating
/// functor requirements transitively.
pub struct FunctorConstraintSolver {
    /// Map from gate symbol ID to its functor constraints.
    constraints: FxHashMap<SymbolId, FunctorConstraints>,
    /// Set of gate symbol IDs that are defined in this program (user-defined gates).
    defined_gates: FxHashSet<SymbolId>,
    /// Stack of gate definitions we're currently inside (for tracking nested calls).
    current_gate_stack: Vec<SymbolId>,
}

impl FunctorConstraintSolver {
    /// Creates a new functor constraint solver.
    #[must_use]
    pub fn new() -> Self {
        Self {
            constraints: FxHashMap::default(),
            defined_gates: FxHashSet::default(),
            current_gate_stack: Vec::new(),
        }
    }

    /// Runs the constraint solver pass on a program and returns the functor constraints
    /// for each gate definition.
    ///
    /// The returned map contains entries only for gates that have been called with
    /// modifiers requiring functor support. Gates not in the map have no constraints.
    #[must_use]
    pub fn solve(program: &Program) -> FxHashMap<SymbolId, FunctorConstraints> {
        let mut solver = Self::new();

        // First pass: collect all gate definitions
        solver.collect_gate_definitions(program);

        // Second pass: analyze gate calls and collect constraints
        solver.visit_program(program);

        // Third pass: propagate constraints transitively through gate call chains
        solver.propagate_constraints(program);

        solver.constraints
    }

    /// First pass: collect all gate definitions in the program.
    fn collect_gate_definitions(&mut self, program: &Program) {
        struct GateDefCollector<'a> {
            defined_gates: &'a mut FxHashSet<SymbolId>,
        }

        impl Visitor for GateDefCollector<'_> {
            fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
                self.defined_gates.insert(stmt.symbol_id);
                // Don't walk into the body here - we'll do that in the main pass
            }
        }

        let mut collector = GateDefCollector {
            defined_gates: &mut self.defined_gates,
        };
        collector.visit_program(program);
    }

    /// Third pass: propagate constraints transitively.
    ///
    /// If gate A calls gate B with `inv @`, and gate B is a user-defined gate,
    /// then gate B needs Adj support. But if gate A is itself called with `ctrl @`,
    /// then gate B also needs Ctl support (because the `ctrl @` will propagate through).
    fn propagate_constraints(&mut self, program: &Program) {
        /// Safety limit to prevent infinite loops in constraint propagation.
        const MAX_ITERATIONS: usize = 100;

        // We need to iterate until we reach a fixed point, as constraints can cascade
        // through multiple levels of gate calls.
        let mut changed = true;
        let mut iterations = 0;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            // For each gate definition, check if it calls other gates that have constraints
            let mut visitor = PropagationVisitor {
                constraints: &self.constraints,
                defined_gates: &self.defined_gates,
                current_gate: None,
                updates: Vec::new(),
            };
            visitor.visit_program(program);

            // Apply updates
            for (symbol_id, new_constraints) in visitor.updates {
                let entry = self.constraints.entry(symbol_id).or_default();
                let old_adj = entry.requires_adj;
                let old_ctl = entry.requires_ctl;
                entry.merge(&new_constraints);
                if entry.requires_adj != old_adj || entry.requires_ctl != old_ctl {
                    changed = true;
                }
            }
        }
    }

    /// Gets the functor constraints for a specific gate, if any.
    #[must_use]
    pub fn get_constraints(&self, symbol_id: SymbolId) -> Option<&FunctorConstraints> {
        self.constraints.get(&symbol_id)
    }
}

/// Helper visitor for propagating constraints through gate call chains.
struct PropagationVisitor<'a> {
    constraints: &'a FxHashMap<SymbolId, FunctorConstraints>,
    defined_gates: &'a FxHashSet<SymbolId>,
    current_gate: Option<SymbolId>,
    updates: Vec<(SymbolId, FunctorConstraints)>,
}

impl Visitor for PropagationVisitor<'_> {
    fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
        self.current_gate = Some(stmt.symbol_id);
        walk_quantum_gate_definition(self, stmt);
        self.current_gate = None;
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        // If we're inside a gate definition and the current gate has constraints,
        // we need to propagate those constraints to any gates we call.
        if let Some(current_gate_id) = self.current_gate
            && let Some(current_constraints) = self.constraints.get(&current_gate_id)
            && self.defined_gates.contains(&stmt.symbol_id)
            && current_constraints.any()
        {
            let mut new_constraints = current_constraints.clone();
            // Also add any constraints from modifiers on this specific call
            for modifier in &stmt.modifiers {
                match &modifier.kind {
                    GateModifierKind::Inv => {
                        new_constraints.requires_adj = true;
                    }
                    GateModifierKind::Ctrl(_) | GateModifierKind::NegCtrl(_) => {
                        new_constraints.requires_ctl = true;
                    }
                    GateModifierKind::Pow(_) => {
                        // pow modifier doesn't directly require functors
                    }
                }
            }
            self.updates.push((stmt.symbol_id, new_constraints));
        }
        walk_gate_call_stmt(self, stmt);
    }
}

impl Default for FunctorConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Visitor for FunctorConstraintSolver {
    fn visit_quantum_gate_definition(&mut self, stmt: &QuantumGateDefinition) {
        // Push this gate onto the stack when we enter its definition
        self.current_gate_stack.push(stmt.symbol_id);
        walk_quantum_gate_definition(self, stmt);
        self.current_gate_stack.pop();
    }

    fn visit_gate_call_stmt(&mut self, stmt: &GateCall) {
        // Analyze modifiers on this gate call
        let mut call_constraints = FunctorConstraints::default();

        for modifier in &stmt.modifiers {
            match &modifier.kind {
                GateModifierKind::Inv => {
                    call_constraints.requires_adj = true;
                }
                GateModifierKind::Ctrl(_) | GateModifierKind::NegCtrl(_) => {
                    call_constraints.requires_ctl = true;
                }
                GateModifierKind::Pow(_) => {
                    // The pow modifier uses ApplyOperationPowerA which requires Adj functor
                    // because it needs to invert the operation for negative powers.
                    call_constraints.requires_adj = true;
                }
            }
        }

        // Only add constraints if this is a user-defined gate and there are constraints
        if call_constraints.any() && self.defined_gates.contains(&stmt.symbol_id) {
            let entry = self.constraints.entry(stmt.symbol_id).or_default();
            entry.merge(&call_constraints);
        }

        // Continue walking to handle nested expressions in the gate call
        walk_gate_call_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_functor_constraints_default() {
        let constraints = FunctorConstraints::default();
        assert!(!constraints.requires_adj);
        assert!(!constraints.requires_ctl);
        assert!(constraints.none());
        assert!(!constraints.any());
    }

    #[test]
    fn test_functor_constraints_merge() {
        let mut c1 = FunctorConstraints {
            requires_adj: true,
            requires_ctl: false,
        };
        let c2 = FunctorConstraints {
            requires_adj: false,
            requires_ctl: true,
        };
        c1.merge(&c2);
        assert!(c1.requires_adj);
        assert!(c1.requires_ctl);
    }
}
