// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering a QHDG graph to the runtime IR and on to QIR.
//!
//! This is the path a program takes after passes have run on it. It exists so
//! that a graph is a compilation target rather than a detour: routing it through
//! the circuit surface first would limit what a pass may produce to what a
//! circuit can already express.
//!
//! It shares one thing with the direct path deliberately. It reads the graph
//! through the same reader the raise uses, so the two cannot disagree about
//! program order or which qubit index a capability stands for. Everything after
//! that is its own, including the gate table, the rewrite dispatch, the
//! declarations, the output recording, and the refusals. A single emitter behind
//! both paths would put all of that beyond the reach of the differential test
//! that exists to catch the two of them diverging.
//!
//! Only the vocabulary the circuit lowering emits compiles here. The compiler's
//! own graph importer produces a different one, and the reader refuses it by
//! name rather than inventing the operands it does not carry.

#[cfg(test)]
mod tests;

mod emit;

use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_qhdg::Graph;
use qsc_rir::rir::{BlockId, Program};

use crate::circuit_qhdg::{
    BARRIER, RESET, ReadConditional, ReadError, ReadInstruction, ReadItem, THETA, gate_from_name,
    read_program,
};
use emit::Emitter;

/// Why a graph could not be lowered to the runtime IR.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The graph could not be read as a circuit-vocabulary program.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The graph names an operation the runtime IR has no form for.
    ///
    /// Distinct from the reader's vocabulary refusal: this name is one the
    /// circuit conversion could have produced, and there is still nothing to
    /// emit for it.
    #[error("operation '{operation}' has no runtime-IR form")]
    Unsupported { operation: String },
    /// A gate with no intrinsic and no rewrite into gates that have one.
    ///
    /// The wording matches what the direct path reports, and the differential
    /// test compares the two messages. That is the point: two paths that refuse
    /// the same program for reasons they worded differently have disagreed.
    #[error("gate '{gate}' has no QIR intrinsic and must be decomposed before lowering")]
    UndecomposedGate { gate: &'static str },
    /// An operation that cannot be emitted as the graph presents it.
    #[error("operation '{operation}' cannot be lowered directly")]
    UnsupportedOperation { operation: &'static str },
    /// A construct the target profile has no form for.
    ///
    /// Worded as the direct path words it, because the differential test
    /// compares the two refusals and two paths that refuse the same program
    /// differently have disagreed about it.
    #[error("'{construct}' requires the {capability} capability, which the target profile lacks")]
    MissingCapability {
        construct: &'static str,
        capability: &'static str,
    },
    /// An angle that still names a parameter.
    ///
    /// The graph carries symbolic angles so that passes can run on a
    /// variational program. A target cannot, so binding has to happen first.
    #[error("parameter '{parameter}' is unbound; bind the circuit's parameters before lowering")]
    UnboundParameter { parameter: String },
}

/// Lowers a graph to the runtime IR for a target profile.
///
/// # Errors
///
/// Returns an error naming the operation, the construct, or the capability the
/// profile is missing, matching what the direct path reports for the same
/// program.
pub fn graph_to_rir(graph: &Graph, capabilities: TargetCapabilityFlags) -> Result<Program, Error> {
    let program = read_program(graph)?;
    let mut emitter = Emitter::new(capabilities, program.qubit_count, program.clbit_count);
    let entry = emitter.start();
    let tail = emit_items(&mut emitter, &program.items, entry)?;
    Ok(emitter.finish(tail))
}

/// Emits a run of steps, returning the block execution ends in.
///
/// A conditional splits the run across several blocks, so the block that
/// follows is not always the one it started in.
fn emit_items(emitter: &mut Emitter, items: &[ReadItem], start: BlockId) -> Result<BlockId, Error> {
    let mut current = start;
    for item in items {
        match item {
            ReadItem::Instruction(instruction) => {
                emit_instruction(emitter, instruction, current)?;
            }
            ReadItem::Conditional(conditional) => {
                current = emit_conditional(emitter, conditional, current)?;
            }
            // This path emits the circuit's own straight-line-plus-branch
            // shape. A loop reaches the runtime IR through the producer that
            // made it, which keeps its own instructions and does not rebuild
            // them from here.
            ReadItem::Loop(_) => {
                return Err(Error::UnsupportedOperation { operation: THETA });
            }
        }
    }
    Ok(current)
}

fn emit_instruction(
    emitter: &mut Emitter,
    instruction: &ReadInstruction,
    current: BlockId,
) -> Result<(), Error> {
    match instruction.operation.as_str() {
        RESET => emitter.emit_reset(&instruction.qubits, current),
        BARRIER => emitter.emit_barrier(current),
        name => {
            let gate = gate_from_name(name).ok_or_else(|| Error::Unsupported {
                operation: name.to_string(),
            })?;
            let params = instruction
                .params
                .iter()
                .map(|angle| {
                    angle.as_literal().ok_or_else(|| Error::UnboundParameter {
                        parameter: angle
                            .parameters()
                            .first()
                            .map_or_else(|| "?".to_string(), ToString::to_string),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            emitter.emit_gate(
                gate,
                &instruction.qubits,
                &instruction.clbits,
                &params,
                current,
            )?;
        }
    }
    Ok(())
}

/// Emits a conditional as a branch over blocks, and returns where they rejoin.
///
/// A conditional with no alternative branches straight to the join, which is
/// what the direct path does, so the two produce the same block structure for
/// the same program rather than the same instructions in a different shape.
fn emit_conditional(
    emitter: &mut Emitter,
    conditional: &ReadConditional,
    current: BlockId,
) -> Result<BlockId, Error> {
    let condition = emitter.read_result(conditional.clbit, current);
    emitter.require(TargetCapabilityFlags::Adaptive, "if", "Adaptive")?;

    let then_start = emitter.new_block();
    let join = emitter.new_block();
    let else_start = if conditional.else_items.is_some() {
        emitter.new_block()
    } else {
        join
    };
    emitter.branch(current, condition, then_start, else_start);

    let then_tail = emit_items(emitter, &conditional.then_items, then_start)?;
    emitter.jump(then_tail, join);
    if let Some(items) = &conditional.else_items {
        let else_tail = emit_items(emitter, items, else_start)?;
        emitter.jump(else_tail, join);
    }
    Ok(join)
}

/// Lowers a graph and emits QIR for a target profile.
///
/// # Errors
///
/// Returns the same errors as [`graph_to_rir`].
pub fn graph_to_qir(graph: &Graph, capabilities: TargetCapabilityFlags) -> Result<String, Error> {
    let mut program = graph_to_rir(graph, capabilities)?;
    qsc_rir::passes::check_and_transform(&mut program);
    Ok(
        if capabilities <= qsc_data_structures::target::Profile::AdaptiveRIF.into() {
            crate::qir::v1::ToQir::<String>::to_qir(&program, &program)
        } else {
            crate::qir::v2::ToQir::<String>::to_qir(&program, &program)
        },
    )
}
