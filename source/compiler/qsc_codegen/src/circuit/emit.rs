// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Emitting runtime IR, shared by every path that produces it.
//!
//! Two paths reach QIR from a program this crate holds: directly from a circuit,
//! and from a graph a pass may have transformed. They differ in what they read
//! and must not differ in what they emit. A second copy of the gate table, the
//! output-recording scheme, or the capability checks would be free to drift, and
//! the drift would show up as two programs that run and disagree.
//!
//! So the reading stays with each path and the emitting lives here.

use qsc_circuit_store::StandardGate;
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_rir::builder;
use qsc_rir::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Config, Instruction, Literal as RirLiteral,
    Operand, Prim, Program, Ty, Variable, VariableId,
};
use rustc_hash::FxHashMap;

use super::{Error, decompose};

/// What lowering a gate produces.
pub(crate) enum GateLowering {
    /// Calls this QIR intrinsic.
    Intrinsic(&'static str),
    /// Emits nothing, because the gate has no effect to emit.
    Nothing,
    /// Has no counterpart in the intrinsic set the compiler emits, so it has to
    /// be decomposed into gates that do before it can be lowered.
    NeedsDecomposition,
}

/// Decides how a gate lowers.
///
/// The intrinsic set the compiler emits is the authority here, which is why the
/// gates without one are named as needing decomposition rather than being
/// approximated by something close.
pub(crate) const fn gate_lowering(gate: StandardGate) -> GateLowering {
    match gate {
        StandardGate::I => GateLowering::Nothing,
        StandardGate::X => GateLowering::Intrinsic("__quantum__qis__x__body"),
        StandardGate::Y => GateLowering::Intrinsic("__quantum__qis__y__body"),
        StandardGate::Z => GateLowering::Intrinsic("__quantum__qis__z__body"),
        StandardGate::H => GateLowering::Intrinsic("__quantum__qis__h__body"),
        StandardGate::S => GateLowering::Intrinsic("__quantum__qis__s__body"),
        StandardGate::SDag => GateLowering::Intrinsic("__quantum__qis__s__adj"),
        StandardGate::T => GateLowering::Intrinsic("__quantum__qis__t__body"),
        StandardGate::TDag => GateLowering::Intrinsic("__quantum__qis__t__adj"),
        StandardGate::SqrtX => GateLowering::Intrinsic("__quantum__qis__sx__body"),
        StandardGate::Cx => GateLowering::Intrinsic("__quantum__qis__cx__body"),
        StandardGate::Cy => GateLowering::Intrinsic("__quantum__qis__cy__body"),
        StandardGate::Cz => GateLowering::Intrinsic("__quantum__qis__cz__body"),
        StandardGate::Swap => GateLowering::Intrinsic("__quantum__qis__swap__body"),
        StandardGate::Ccx => GateLowering::Intrinsic("__quantum__qis__ccx__body"),
        StandardGate::Rx => GateLowering::Intrinsic("__quantum__qis__rx__body"),
        StandardGate::Ry => GateLowering::Intrinsic("__quantum__qis__ry__body"),
        StandardGate::Rz => GateLowering::Intrinsic("__quantum__qis__rz__body"),
        StandardGate::Rxx => GateLowering::Intrinsic("__quantum__qis__rxx__body"),
        StandardGate::Ryy => GateLowering::Intrinsic("__quantum__qis__ryy__body"),
        StandardGate::Rzz => GateLowering::Intrinsic("__quantum__qis__rzz__body"),
        StandardGate::MeasureZ => GateLowering::Intrinsic("__quantum__qis__m__body"),
        // Preparing the Z basis state is the same operation as a reset.
        StandardGate::PrepareZ => GateLowering::Intrinsic("__quantum__qis__reset__body"),
        StandardGate::SqrtXDag
        | StandardGate::Cswap
        | StandardGate::Crx
        | StandardGate::Cry
        | StandardGate::Crz
        | StandardGate::MeasureX
        | StandardGate::MeasureY
        | StandardGate::PrepareX
        | StandardGate::PrepareY => GateLowering::NeedsDecomposition,
    }
}

/// Builds a runtime-IR program from resolved operands.
///
/// Everything here takes indices and literal angles, which is what both paths
/// have once they have finished reading their own input.
pub(crate) struct Emitter {
    pub(crate) program: Program,
    /// Declared intrinsics, so each is declared once however often it is used.
    declared: FxHashMap<&'static str, CallableId>,
    next_callable: u32,
    next_block: u32,
    next_variable: u32,
    /// Classical bits written by a measurement, in the order first written.
    measured: Vec<u32>,
}

impl Emitter {
    pub(crate) fn new(
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
    pub(crate) fn start(&mut self) -> BlockId {
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
        self.push(
            entry_block,
            Instruction::Call(
                initialize,
                vec![Operand::Literal(RirLiteral::NullPointer)],
                None,
                None,
            ),
        );
        entry_block
    }

    /// Records the measured results and returns from the entry callable.
    pub(crate) fn finish(mut self, tail: BlockId) -> Program {
        self.record_output(tail);
        self.push(
            tail,
            Instruction::Return(Some(Operand::Literal(RirLiteral::Integer(0)))),
        );
        self.program
    }

    /// Emits a standard gate, decomposing it first when it has no intrinsic.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UndecomposedGate`] when the gate has no intrinsic and no
    /// decomposition, which is the case a caller has to resolve rather than one
    /// this can approximate.
    pub(crate) fn emit_standard(
        &mut self,
        gate: StandardGate,
        qubits: &[u32],
        clbits: &[u32],
        angles: &[f64],
        current: BlockId,
    ) -> Result<(), Error> {
        let name = match gate_lowering(gate) {
            GateLowering::Intrinsic(name) => name,
            GateLowering::Nothing => return Ok(()),
            GateLowering::NeedsDecomposition => {
                let steps = decompose::decompose(gate, qubits, angles)
                    .ok_or(Error::UndecomposedGate { gate: gate.name() })?;
                for step in steps {
                    // A rewrite only ever produces gates the intrinsic set
                    // holds, so this does not recur.
                    self.emit_step(&step, clbits, current)?;
                }
                return Ok(());
            }
        };
        self.emit_gate(gate, name, qubits, clbits, angles, current)
    }

    /// Emits one gate of a rewrite.
    fn emit_step(
        &mut self,
        step: &decompose::Step,
        clbits: &[u32],
        current: BlockId,
    ) -> Result<(), Error> {
        let GateLowering::Intrinsic(name) = gate_lowering(step.gate) else {
            return Err(Error::UndecomposedGate {
                gate: step.gate.name(),
            });
        };
        self.emit_gate(step.gate, name, &step.qubits, clbits, &step.angles, current)
    }

    fn emit_gate(
        &mut self,
        gate: StandardGate,
        name: &'static str,
        qubits: &[u32],
        clbits: &[u32],
        angles: &[f64],
        current: BlockId,
    ) -> Result<(), Error> {
        let mut args: Vec<Operand> = angles
            .iter()
            .map(|angle| Operand::Literal(RirLiteral::Double(*angle)))
            .collect();
        args.extend(
            qubits
                .iter()
                .map(|qubit| Operand::Literal(RirLiteral::Qubit(*qubit))),
        );

        let callable = if gate == StandardGate::MeasureZ {
            let clbit = *clbits.first().ok_or(Error::UnsupportedOperation {
                operation: "measurement without a classical bit",
            })?;
            args.push(Operand::Literal(RirLiteral::Result(clbit)));
            if !self.measured.contains(&clbit) {
                self.measured.push(clbit);
            }
            self.declare(name, builder::m_decl())
        } else if gate == StandardGate::PrepareZ {
            self.declare(name, builder::reset_decl())
        } else {
            let decl = gate_decl(name, qubits.len(), usize::from(gate.param_arity()));
            self.declare(name, decl)
        };
        self.push(current, Instruction::Call(callable, args, None, None));
        Ok(())
    }

    pub(crate) fn emit_reset(&mut self, qubits: &[u32], current: BlockId) {
        let reset = self.declare("__quantum__qis__reset__body", builder::reset_decl());
        let args = qubits
            .iter()
            .map(|qubit| Operand::Literal(RirLiteral::Qubit(*qubit)))
            .collect();
        self.push(current, Instruction::Call(reset, args, None, None));
    }

    /// Emits a barrier.
    ///
    /// A QIR barrier takes no operands, so the qubits a circuit barrier names do
    /// not survive. This matches how the OpenQASM compiler emits one.
    pub(crate) fn emit_barrier(&mut self, current: BlockId) {
        let barrier = self.declare("__quantum__qis__barrier__body", barrier_decl());
        self.push(current, Instruction::Call(barrier, Vec::new(), None, None));
    }

    /// Records measured results so they appear in the program's output.
    fn record_output(&mut self, block: BlockId) {
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
        let tag = self.push_tag("0_a");
        self.push(
            block,
            Instruction::Call(
                array_record,
                vec![Operand::Literal(RirLiteral::Integer(count)), tag],
                None,
                None,
            ),
        );

        let result_record = self.declare(
            "__quantum__rt__result_record_output",
            builder::result_record_decl(),
        );
        for index in 0..self.measured.len() {
            let clbit = self.measured[index];
            // Tags follow the compiler's own scheme for an array of results, so
            // output parsed from this program looks the same as output from an
            // equivalent Q# program.
            let tag = self.push_tag(&format!("{}_a{index}r", index + 1));
            self.push(
                block,
                Instruction::Call(
                    result_record,
                    vec![Operand::Literal(RirLiteral::Result(clbit)), tag],
                    None,
                    None,
                ),
            );
        }
    }

    /// Fails when the target profile lacks a capability the program needs.
    pub(crate) fn require(
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

    fn push_tag(&mut self, tag: &str) -> Operand {
        let index = self.program.tags.len();
        self.program.tags.push(tag.to_string());
        Operand::Literal(RirLiteral::Tag(index, tag.len()))
    }

    pub(crate) fn declare(&mut self, name: &'static str, callable: Callable) -> CallableId {
        if let Some(existing) = self.declared.get(name) {
            return *existing;
        }
        let id = CallableId(self.next_callable);
        self.next_callable += 1;
        self.program.callables.insert(id, callable);
        self.declared.insert(name, id);
        id
    }

    pub(crate) fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.program.blocks.insert(id, Block::default());
        id
    }

    pub(crate) fn new_variable(&mut self, ty: Ty) -> Variable {
        let variable = Variable {
            variable_id: VariableId(self.next_variable),
            ty,
        };
        self.next_variable += 1;
        variable
    }

    pub(crate) fn push(&mut self, block: BlockId, instruction: Instruction) {
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

/// Declares a gate intrinsic, with angles ahead of qubits as the compiler emits them.
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
