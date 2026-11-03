// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Emitting runtime IR for the graph path.
//!
//! The direct path has its own emitter and this is deliberately a second one.
//! One emitter behind both paths removes any chance that the two disagree, and
//! it removes the same code from what the differential test can see: two readers
//! of one gate table cannot disagree about the table. Independence costs the
//! duplication here and buys a comparison that can fail on it.
//!
//! What stays shared is [`crate::circuit::decompose`], which is gate algebra
//! rather than a decision about this target, and which the OpenQASM exporter
//! consumes as well.

use qsc_circuit_store::StandardGate;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_rir::builder;
use qsc_rir::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Config, Instruction, Literal, Operand,
    Prim, Program, Ty, Variable, VariableId,
};
use rustc_hash::FxHashMap;

use super::Error;
use crate::circuit::decompose;

/// What a gate becomes in the runtime IR.
enum Form {
    /// A call to this QIR intrinsic.
    Call(&'static str),
    /// Nothing, because the gate has no effect to emit.
    Empty,
    /// Gates that do have intrinsics, once it has been rewritten into them.
    Rewrite,
}

/// Decides what a gate becomes.
///
/// The intrinsic set the compiler emits is the authority, so a gate outside it
/// is rewritten into gates inside it rather than approximated by a near match.
const fn form(gate: StandardGate) -> Form {
    match gate {
        StandardGate::I => Form::Empty,
        StandardGate::X => Form::Call("__quantum__qis__x__body"),
        StandardGate::Y => Form::Call("__quantum__qis__y__body"),
        StandardGate::Z => Form::Call("__quantum__qis__z__body"),
        StandardGate::H => Form::Call("__quantum__qis__h__body"),
        StandardGate::S => Form::Call("__quantum__qis__s__body"),
        StandardGate::SDag => Form::Call("__quantum__qis__s__adj"),
        StandardGate::T => Form::Call("__quantum__qis__t__body"),
        StandardGate::TDag => Form::Call("__quantum__qis__t__adj"),
        StandardGate::SqrtX => Form::Call("__quantum__qis__sx__body"),
        StandardGate::Cx => Form::Call("__quantum__qis__cx__body"),
        StandardGate::Cy => Form::Call("__quantum__qis__cy__body"),
        StandardGate::Cz => Form::Call("__quantum__qis__cz__body"),
        StandardGate::Swap => Form::Call("__quantum__qis__swap__body"),
        StandardGate::Ccx => Form::Call("__quantum__qis__ccx__body"),
        StandardGate::Rx => Form::Call("__quantum__qis__rx__body"),
        StandardGate::Ry => Form::Call("__quantum__qis__ry__body"),
        StandardGate::Rz => Form::Call("__quantum__qis__rz__body"),
        StandardGate::Rxx => Form::Call("__quantum__qis__rxx__body"),
        StandardGate::Ryy => Form::Call("__quantum__qis__ryy__body"),
        StandardGate::Rzz => Form::Call("__quantum__qis__rzz__body"),
        StandardGate::MeasureZ => Form::Call("__quantum__qis__m__body"),
        // Preparing the Z basis state and resetting are the same operation.
        StandardGate::PrepareZ => Form::Call("__quantum__qis__reset__body"),
        StandardGate::SqrtXDag
        | StandardGate::Cswap
        | StandardGate::Crx
        | StandardGate::Cry
        | StandardGate::Crz
        | StandardGate::MeasureX
        | StandardGate::MeasureY
        | StandardGate::PrepareX
        | StandardGate::PrepareY => Form::Rewrite,
    }
}

/// Builds a runtime-IR program from operands the reader has already resolved.
///
/// This is a second emitter on purpose, and it stays one. Converging it with
/// the direct path's emitter would be cheaper and would put the translation
/// decisions the differential test exists to compare behind a single
/// implementation, where two readers over one emitter cannot disagree.
pub(super) struct Emitter {
    program: Program,
    /// Declared intrinsics, so each is declared once however often it is called.
    declared: FxHashMap<&'static str, CallableId>,
    next_callable: u32,
    next_block: u32,
    next_variable: u32,
    /// Classical bits a measurement wrote, in the order first written.
    measured: Vec<u32>,
}

impl Emitter {
    pub(super) fn new(
        capabilities: TargetCapabilityFlags,
        num_qubits: u32,
        num_results: u32,
    ) -> Self {
        let mut program = Program::new();
        program.config = Config { capabilities };
        program.num_qubits = num_qubits;
        program.num_results = num_results;
        Self {
            program,
            declared: FxHashMap::default(),
            next_callable: 0,
            next_block: 0,
            next_variable: 0,
            measured: Vec::new(),
        }
    }

    /// Opens the entry callable and returns the block instructions go into.
    pub(super) fn start(&mut self) -> BlockId {
        let entry_block = self.new_block();
        let entry = self.declare(
            "main",
            Callable {
                name: "main".to_string(),
                input_type: Vec::new(),
                input_vars: Vec::new(),
                output_type: Some(Ty::Prim(Prim::Integer)),
                body: Some(entry_block),
                call_type: CallableType::Regular,
            },
        );
        self.program.entry = entry;

        let initialize = self.declare("__quantum__rt__initialize", builder::initialize_decl());
        self.call(
            initialize,
            vec![Operand::Literal(Literal::NullPointer)],
            entry_block,
        );
        entry_block
    }

    /// Records the measured results and returns from the entry callable.
    pub(super) fn finish(mut self, tail: BlockId) -> Program {
        self.record_output(tail);
        self.push(
            tail,
            Instruction::Return(Some(Operand::Literal(Literal::Integer(0)))),
        );
        self.program
    }

    /// Reads a measurement result into a condition to branch on.
    pub(super) fn read_result(&mut self, clbit: u32, current: BlockId) -> Variable {
        let read = self.declare("__quantum__rt__read_result", builder::read_result_decl());
        let variable = self.new_variable(Ty::Prim(Prim::Boolean));
        self.push(
            current,
            Instruction::Call(
                read,
                vec![Operand::Literal(Literal::Result(clbit))],
                Some(variable),
                None,
            ),
        );
        variable
    }

    pub(super) fn branch(
        &mut self,
        current: BlockId,
        condition: Variable,
        taken: BlockId,
        untaken: BlockId,
    ) {
        self.push(
            current,
            Instruction::Branch(condition, taken, untaken, None),
        );
    }

    pub(super) fn jump(&mut self, current: BlockId, target: BlockId) {
        self.push(current, Instruction::Jump(target));
    }

    /// Fails when the target profile lacks a capability the program needs.
    pub(super) fn require(
        &self,
        capability: TargetCapabilityFlags,
        construct: &'static str,
        name: &'static str,
    ) -> Result<(), Error> {
        if self.program.config.capabilities.contains(capability) {
            Ok(())
        } else {
            Err(Error::MissingCapability {
                construct,
                capability: name,
            })
        }
    }

    pub(super) fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.program.blocks.insert(id, Block::default());
        id
    }

    fn new_variable(&mut self, ty: Ty) -> Variable {
        let variable = Variable {
            variable_id: VariableId(self.next_variable),
            ty,
        };
        self.next_variable += 1;
        variable
    }

    /// Emits a standard gate, rewriting it first when it has no intrinsic.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UndecomposedGate`] when the gate has no intrinsic and no
    /// rewrite into gates that do.
    pub(super) fn emit_gate(
        &mut self,
        gate: StandardGate,
        qubits: &[u32],
        clbits: &[u32],
        angles: &[f64],
        current: BlockId,
    ) -> Result<(), Error> {
        match form(gate) {
            Form::Call(name) => self.emit_call(gate, name, qubits, clbits, angles, current),
            Form::Empty => Ok(()),
            Form::Rewrite => {
                let steps = decompose::decompose(gate, qubits, angles)
                    .ok_or(Error::UndecomposedGate { gate: gate.name() })?;
                for step in steps {
                    // A rewrite only produces gates the intrinsic set holds, so
                    // this does not recur.
                    let Form::Call(name) = form(step.gate) else {
                        return Err(Error::UndecomposedGate {
                            gate: step.gate.name(),
                        });
                    };
                    self.emit_call(step.gate, name, &step.qubits, clbits, &step.angles, current)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn emit_reset(&mut self, qubits: &[u32], current: BlockId) {
        let reset = self.declare("__quantum__qis__reset__body", builder::reset_decl());
        let args = qubits
            .iter()
            .map(|qubit| Operand::Literal(Literal::Qubit(*qubit)))
            .collect();
        self.call(reset, args, current);
    }

    /// Emits a barrier.
    ///
    /// The QIR barrier takes no operands, so the qubits a barrier named do not
    /// survive into the emitted program.
    pub(super) fn emit_barrier(&mut self, current: BlockId) {
        let barrier = self.declare("__quantum__qis__barrier__body", barrier_decl());
        self.call(barrier, Vec::new(), current);
    }

    fn emit_call(
        &mut self,
        gate: StandardGate,
        name: &'static str,
        qubits: &[u32],
        clbits: &[u32],
        angles: &[f64],
        current: BlockId,
    ) -> Result<(), Error> {
        // Angles precede qubits, matching the declaration order below.
        let mut args: Vec<Operand> = angles
            .iter()
            .map(|angle| Operand::Literal(Literal::Double(*angle)))
            .collect();
        args.extend(
            qubits
                .iter()
                .map(|qubit| Operand::Literal(Literal::Qubit(*qubit))),
        );

        let callable = match gate {
            StandardGate::MeasureZ => {
                let clbit = *clbits.first().ok_or(Error::UnsupportedOperation {
                    operation: "measurement without a classical bit",
                })?;
                args.push(Operand::Literal(Literal::Result(clbit)));
                if !self.measured.contains(&clbit) {
                    self.measured.push(clbit);
                }
                self.declare(name, builder::m_decl())
            }
            StandardGate::PrepareZ => self.declare(name, builder::reset_decl()),
            _ => {
                let decl = gate_decl(name, qubits.len(), usize::from(gate.param_arity()));
                self.declare(name, decl)
            }
        };
        self.call(callable, args, current);
        Ok(())
    }

    /// Records measured results so they appear in the program's output.
    ///
    /// The tags follow the compiler's scheme for an array of results, so output
    /// parsed from this program reads the same as output from an equivalent Q#
    /// program.
    fn record_output(&mut self, tail: BlockId) {
        if self.measured.is_empty() {
            return;
        }
        self.measured.sort_unstable();

        let array_record = self.declare(
            "__quantum__rt__array_record_output",
            builder::array_record_decl(),
        );
        let count =
            i64::from(u32::try_from(self.measured.len()).expect("result count fits in u32"));
        let tag = self.tag("0_a");
        self.call(
            array_record,
            vec![Operand::Literal(Literal::Integer(count)), tag],
            tail,
        );

        let result_record = self.declare(
            "__quantum__rt__result_record_output",
            builder::result_record_decl(),
        );
        for position in 0..self.measured.len() {
            let clbit = self.measured[position];
            let tag = self.tag(&format!("{}_a{position}r", position + 1));
            self.call(
                result_record,
                vec![Operand::Literal(Literal::Result(clbit)), tag],
                tail,
            );
        }
    }

    fn tag(&mut self, tag: &str) -> Operand {
        let index = self.program.tags.len();
        self.program.tags.push(tag.to_string());
        Operand::Literal(Literal::Tag(index, tag.len()))
    }

    fn declare(&mut self, name: &'static str, callable: Callable) -> CallableId {
        if let Some(existing) = self.declared.get(name) {
            return *existing;
        }
        let id = CallableId(self.next_callable);
        self.next_callable += 1;
        self.program.callables.insert(id, callable);
        self.declared.insert(name, id);
        id
    }

    fn call(&mut self, callable: CallableId, args: Vec<Operand>, current: BlockId) {
        self.push(current, Instruction::Call(callable, args, None, None));
    }

    fn push(&mut self, block: BlockId, instruction: Instruction) {
        self.program.get_block_mut(block).0.push(instruction);
    }
}

/// Declares the barrier intrinsic, which takes no operands.
fn barrier_decl() -> Callable {
    Callable {
        name: "__quantum__qis__barrier__body".to_string(),
        input_type: Vec::new(),
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

/// Declares a gate intrinsic, with angles ahead of qubits.
fn gate_decl(name: &str, qubit_arity: usize, param_arity: usize) -> Callable {
    let mut input_type = Vec::with_capacity(param_arity + qubit_arity);
    input_type.extend(std::iter::repeat_n(Ty::Prim(Prim::Double), param_arity));
    input_type.extend(std::iter::repeat_n(Ty::Prim(Prim::Qubit), qubit_arity));
    Callable {
        name: name.to_string(),
        input_type,
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}
