// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A Qodec ISA as the protocol's instruction set ([`QodecInstructionSet`]),
//! and the translation of each runtime's instructions and readouts into the
//! other's.
//!
//! Both parts of this crate need it: the adapter turns the instructions it
//! gets into the Qodec calls its layers take, and the calls they lower into
//! instructions for the part below; the executor turns the instructions it
//! gets into the calls its simulator takes. The set keeps the ISA's order,
//! so an instruction's opcode is at the place of its Qodec instruction id.
//! Both also keep each shot's [`LiveBlocks`], as a block's address in one
//! runtime is not always one in the other.

mod live_blocks;

use std::fmt;
use std::sync::Arc;

use executor_protocol::{
    Argument, Arguments, Bit, BlockOperand, BlockType, Blocks, DeclarationFault, Instruction,
    InstructionDef, InstructionSet, Opcode, OpcodeMap, ParameterKind, ReadoutFailure, ReadoutValue,
    ShotConditions,
};
use qodec_runtime_recipes::pauli::Pauli;
use qodec_runtime_recipes::{BasicReadout, Call, Isa, Operand, OwnedParam, ParamKind, Status};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

pub(crate) use live_blocks::LiveBlocks;

/// A Qodec ISA, and the same instructions as the protocol's
/// [`InstructionSet`]: its name, block types and instructions, each with
/// its operands, parameters, outcomes and flags. Its meanings are
/// undeclared, so two sets fit by mnemonic, between sets of one name.
#[derive(Clone, Debug)]
pub struct QodecInstructionSet {
    isa: Arc<Isa>,
    instruction_set: InstructionSet,
    /// The opcode of each Qodec instruction id.
    opcodes: Vec<Opcode>,
    /// The Qodec instruction id of each opcode.
    ids: FxHashMap<Opcode, usize>,
}

impl QodecInstructionSet {
    /// The set of `isa`.
    ///
    /// # Errors
    ///
    /// The faults the protocol finds in its declaration
    /// ([`InstructionSet::new`]), such as a variadic operand where the
    /// protocol allows none.
    pub fn new(isa: &Arc<Isa>) -> Result<Self, Vec<DeclarationFault>> {
        let block_type = |operand: &Operand| {
            let block_type = BlockType::new(isa.types.names[operand.block].clone());
            if operand.variadic {
                BlockOperand::variadic(block_type)
            } else {
                BlockOperand::from(block_type)
            }
        };
        let defs = isa.instructions.iter().map(|signature| {
            let mut def = InstructionDef::new(signature.mnemonic.clone())
                .takes(signature.inputs.iter().map(block_type))
                .gives(signature.outputs.iter().map(block_type))
                .outcomes(u32::try_from(signature.outcomes).expect("fewer than 2^32 outcomes"))
                .flags((0..signature.flags).map(|k| format!("flag_{k}")));
            for parameter in &signature.parameters {
                def = def.parameter(parameter.name.clone(), parameter_kind(parameter.kind));
            }
            def
        });
        let block_types = isa
            .types
            .names
            .iter()
            .zip(&isa.types.caps)
            .map(|(name, &cap)| {
                (
                    BlockType::new(name.clone()),
                    u32::try_from(cap).expect("a block holds fewer than 2^32 qubits"),
                )
            });
        let instruction_set = InstructionSet::new(isa.name.clone(), block_types, defs)?;
        let opcodes: Vec<Opcode> = isa
            .instructions
            .iter()
            .map(|signature| {
                instruction_set
                    .opcode(&signature.mnemonic)
                    .expect("the set declares every instruction of its ISA")
            })
            .collect();
        let ids = opcodes
            .iter()
            .enumerate()
            .map(|(id, &op)| (op, id))
            .collect();
        Ok(Self {
            isa: Arc::clone(isa),
            instruction_set,
            opcodes,
            ids,
        })
    }

    /// The protocol's set.
    #[must_use]
    pub fn instruction_set(&self) -> &InstructionSet {
        &self.instruction_set
    }

    /// The Qodec ISA, which the executor's translator runs.
    pub(crate) fn isa(&self) -> &Arc<Isa> {
        &self.isa
    }

    /// The Qodec call of `instruction`, an instruction of this set. Its
    /// `select` is not sent: Qodec's `Call` has no field for it, and the
    /// core makes it advisory.
    ///
    /// # Panics
    ///
    /// When this set has no instruction of `instruction`'s opcode, which
    /// only an unchecked batch can carry; the core lets a receiver panic on
    /// such a batch.
    pub(crate) fn call(&self, instruction: &Instruction) -> Result<Call, TranslationFault> {
        let instruction_id = *self
            .ids
            .get(&instruction.opcode)
            .expect("senders send only instructions of the set they fit");
        let mut parameters: SmallVec<[OwnedParam; 2]> = instruction
            .blocks
            .iter()
            .map(|&block| OwnedParam::Block(u64::from(block)))
            .collect();
        for argument in &instruction.arguments {
            parameters.push(owned_param(argument).map_err(|detail| TranslationFault {
                mnemonic: self.isa.instructions[instruction_id].mnemonic.clone(),
                detail,
            })?);
        }
        Ok(Call {
            instruction_id,
            parameters,
        })
    }

    /// The instruction of `call`, a call of this ISA, in the opcodes of the
    /// set `below` maps this one into. A call whose values do not match its
    /// instruction's parameters, in number or in kind, is a fault, never
    /// sent short.
    pub(crate) fn instruction(
        &self,
        call: &Call,
        below: &OpcodeMap,
    ) -> Result<Instruction, TranslationFault> {
        let signature = &self.isa.instructions[call.instruction_id];
        let fault = |detail: String| TranslationFault {
            mnemonic: signature.mnemonic.clone(),
            detail,
        };
        let mut blocks = Blocks::new();
        let mut rest = call.parameters.as_slice();
        while let [OwnedParam::Block(label), tail @ ..] = rest {
            blocks.push(
                u32::try_from(*label)
                    .map_err(|_| fault(format!("block {label} is beyond a `u32`")))?,
            );
            rest = tail;
        }
        if rest.len() != signature.parameters.len() {
            return Err(fault(format!(
                "{} values for {} parameters",
                rest.len(),
                signature.parameters.len()
            )));
        }
        let mut arguments = Arguments::new();
        for (parameter, value) in signature.parameters.iter().zip(rest) {
            arguments.push(argument(parameter.kind, value).map_err(fault)?);
        }
        Ok(Instruction {
            opcode: below.map(self.opcodes[call.instruction_id]),
            blocks,
            arguments,
            select: None,
        })
    }

    /// The readout slots of `calls`: each call's outcomes, then its flags.
    pub(crate) fn readout_slots(&self, calls: &[Call]) -> u32 {
        calls
            .iter()
            .filter_map(|call| self.isa.shape(call.instruction_id))
            .map(|(outcomes, flags)| u32::try_from(outcomes + flags).expect("fewer than 2^32"))
            .sum()
    }

    /// The results of `calls`, one per call that has a result, read from
    /// `row`, a shot's readouts of them, each with `conditions`, those of
    /// the batch below that read them: the shot's, or one part's when the
    /// adapter splits a layer's batch. The protocol raises conditions per
    /// shot and batch, not per result.
    pub(crate) fn results(
        &self,
        calls: &[Call],
        row: &[ReadoutValue],
        conditions: ShotConditions,
    ) -> Vec<BasicReadout> {
        let status = Status {
            rejected: conditions.contains(ShotConditions::REJECTED),
            inconsistent: conditions.contains(ShotConditions::INCONSISTENT),
            unresolved: conditions.contains(ShotConditions::UNRESOLVED),
        };
        let bit = |value: &ReadoutValue| match value {
            ReadoutValue::Zero => Some(false),
            ReadoutValue::One => Some(true),
            // Missing, or a later kind of value: no bit either way.
            _ => None,
        };
        let mut values = row.iter();
        calls
            .iter()
            .filter_map(|call| self.isa.shape(call.instruction_id))
            .map(|(outcomes, flags)| BasicReadout {
                outcomes: values.by_ref().take(outcomes).map(bit).collect(),
                flags: values.by_ref().take(flags).map(bit).collect(),
                status,
            })
            .collect()
    }
}

/// Writes `results` into `row`, a shot's readouts: each result's outcomes,
/// then its flags, in order. Returns the shot's conditions: those any
/// result raises.
///
/// # Panics
///
/// When `row` has fewer slots than `results` have values, which only an
/// unchecked batch that misstates its readout slots can cause; the core
/// lets a receiver panic on such a batch.
pub(crate) fn write_results(results: &[BasicReadout], row: &mut [ReadoutValue]) -> ShotConditions {
    let value = |bit: &Option<bool>| match bit {
        Some(bit) => ReadoutValue::from(*bit),
        None => ReadoutValue::Missing(ReadoutFailure::Other),
    };
    let mut slots = row.iter_mut();
    let mut conditions = ShotConditions::NONE;
    for result in results {
        for bit in result.outcomes.iter().chain(&result.flags) {
            *slots.next().expect("a slot per outcome and flag") = value(bit);
        }
        let status = result.status;
        for (raised, condition) in [
            (status.rejected, ShotConditions::REJECTED),
            (status.inconsistent, ShotConditions::INCONSISTENT),
            (status.unresolved, ShotConditions::UNRESOLVED),
        ] {
            if raised {
                conditions |= condition;
            }
        }
    }
    conditions
}

/// Why an instruction and a Qodec call cannot be translated into each
/// other. Into a call: a readout parity argument, which Qodec's calls
/// cannot take; or a call that prepares a block at the index of a live
/// block of another type, which Qodec's single label per block cannot tell
/// apart. Into an instruction: a call the layers send below whose values
/// do not match its instruction's parameters, in number or in kind (such
/// as a text that Qodec does not read as a Pauli, or a Pauli whose qubit
/// index is beyond a `u32`), or whose block label is beyond a `u32`; that
/// is a fault of the qodec's layers, which none of Qodec's examples makes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TranslationFault {
    /// The instruction's mnemonic.
    pub mnemonic: String,
    /// What has no form on the other side.
    pub detail: String,
}

impl fmt::Display for TranslationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", self.mnemonic, self.detail)
    }
}

impl std::error::Error for TranslationFault {}

/// The protocol's kind for a Qodec parameter kind: they name the same
/// kinds.
fn parameter_kind(kind: ParamKind) -> ParameterKind {
    match kind {
        ParamKind::Bit => ParameterKind::Bit,
        ParamKind::Number => ParameterKind::Number,
        ParamKind::Integer => ParameterKind::Integer,
        ParamKind::Boolean => ParameterKind::Boolean,
        ParamKind::String => ParameterKind::Text,
        ParamKind::Pauli => ParameterKind::Pauli,
    }
}

/// The Qodec value of `argument`. A readout parity has none: Qodec's
/// runtime resolves a parity inside the layer that wrote it, and takes only
/// values from above.
fn owned_param(argument: &Argument) -> Result<OwnedParam, String> {
    Ok(match argument {
        Argument::Bit(Bit::Value(bit)) | Argument::Boolean(bit) => OwnedParam::Bool(*bit),
        Argument::Bit(Bit::Parity(_)) => {
            return Err("a readout parity has no Qodec value".to_owned());
        }
        Argument::Number(number) => OwnedParam::Float(*number),
        Argument::Integer(integer) => OwnedParam::Int(*integer),
        Argument::Text(text) => OwnedParam::Text(text.to_string()),
        Argument::Pauli(pauli) => OwnedParam::Text(pauli.to_string()),
        // A kind the protocol adds later.
        _ => return Err("the argument's kind has no Qodec value".to_owned()),
    })
}

/// The protocol's argument for `value`, a Qodec value of a parameter of
/// `kind`, as Qodec's own check accepts it ([`ParamKind::matches`]).
fn argument(kind: ParamKind, value: &OwnedParam) -> Result<Argument, String> {
    let mismatch = || format!("{value:?} is not a value of a `{kind}` parameter");
    Ok(match (kind, value) {
        (ParamKind::Bit, OwnedParam::Bool(bit)) => Argument::Bit(Bit::Value(*bit)),
        (ParamKind::Bit, OwnedParam::Int(bit @ (0 | 1))) => Argument::Bit(Bit::Value(*bit == 1)),
        (ParamKind::Number, OwnedParam::Float(number)) => Argument::Number(*number),
        #[expect(
            clippy::cast_precision_loss,
            reason = "Qodec supplies a number parameter with an integer, as an `f64`"
        )]
        (ParamKind::Number, OwnedParam::Int(integer)) => Argument::Number(*integer as f64),
        (ParamKind::Integer, OwnedParam::Int(integer)) => Argument::Integer(*integer),
        (ParamKind::Boolean, OwnedParam::Bool(boolean)) => Argument::Boolean(*boolean),
        (ParamKind::String, OwnedParam::Text(text)) => Argument::Text(text.as_str().into()),
        // Qodec's grammar is wider than the protocol's: a term without its
        // `_index` acts on qubit 0, and an empty text is the identity. So
        // the text is read as Qodec reads it, and passed on in the form
        // both grammars share (`-X_0 Z_2`, `I`).
        (ParamKind::Pauli, OwnedParam::Text(text)) => {
            let pauli = Pauli::parse(text)?;
            Argument::pauli(&pauli.to_string()).map_err(|fault| fault.to_string())?
        }
        _ => return Err(mismatch()),
    })
}
