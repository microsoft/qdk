// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Qodec's runtime driven directly, as Qodec's own tests drive it: the
//! oracle every chain is checked against.

use executor_protocol::ShotConditions;
use qdk_simulators::action_simulators::{FullStateActionSimulator, StabilizerActionSimulator};
use std::fmt;
use std::sync::Arc;

use qodec_runtime_recipes::{
    ActionOp, ActionTranslator, Backend, BasicReadout, Call, Isa, OwnedParam, ParamKind, Stack,
};
use smallvec::SmallVec;

use crate::programs::{Program, ShotReadouts, Step};

/// An action simulator, under Qodec's translator.
pub type Engine = Box<dyn Backend<Vec<ActionOp>, Result<Vec<bool>, String>>>;

/// A stabilizer simulator drawing from `stream`.
pub fn stabilizer(stream: u64) -> Engine {
    Box::new(StabilizerActionSimulator::new(stream))
}

/// A state-vector simulator of `qubits` qubits drawing from the low half of
/// `stream`, as the crate's executor seeds it.
pub fn full_state(stream: u64, qubits: usize) -> Engine {
    #[expect(clippy::cast_possible_truncation, reason = "the stream's low half")]
    let seed = stream as u32;
    Box::new(FullStateActionSimulator::new(qubits, seed))
}

/// `isa` on `engine`, under Qodec's translator: an executor's job, done
/// directly.
pub fn simulator(isa: &Arc<Isa>, engine: Engine) -> Stack<ActionTranslator, Engine> {
    Stack::new(ActionTranslator::new(isa), engine)
}

/// What one shot of `program`, of `top`, reads on `backend` (Qodec's
/// runtime, or a simulator alone), each of its `execute`s run in turn.
pub fn run_directly<E: fmt::Display>(
    top: &Isa,
    program: &Program,
    backend: &mut impl Backend<Vec<Call>, Result<Vec<BasicReadout>, E>>,
) -> ShotReadouts {
    let mut readouts = ShotReadouts {
        values: Vec::new(),
        conditions: ShotConditions::NONE,
    };
    for execute in &program.executes {
        let calls = execute.iter().map(|step| call(top, step)).collect();
        let results = backend
            .execute(calls)
            .unwrap_or_else(|error| panic!("the program runs directly: {error}"));
        for result in results {
            add(&mut readouts, &result);
        }
    }
    readouts
}

/// `step`, a call of `isa`.
fn call(isa: &Isa, step: &Step) -> Call {
    let instruction_id = isa
        .instructions
        .iter()
        .position(|signature| signature.mnemonic == step.mnemonic)
        .unwrap_or_else(|| panic!("`{}` is in `{}`", step.mnemonic, isa.name));
    let mut parameters: SmallVec<[OwnedParam; 2]> = step
        .blocks
        .iter()
        .map(|&block| OwnedParam::Block(u64::from(block)))
        .collect();
    for parameter in &isa.instructions[instruction_id].parameters {
        let (_, value) = step
            .numbers
            .iter()
            .find(|(name, _)| *name == parameter.name)
            .unwrap_or_else(|| panic!("`{}` has an argument `{}`", step.mnemonic, parameter.name));
        parameters.push(match parameter.kind {
            ParamKind::Bit => OwnedParam::Bool(*value != 0.0),
            _ => OwnedParam::Float(*value),
        });
    }
    Call {
        instruction_id,
        parameters,
    }
}

/// Adds `result`'s outcomes, flags and status to `readouts`.
fn add(readouts: &mut ShotReadouts, result: &BasicReadout) {
    readouts
        .values
        .extend(result.outcomes.iter().chain(&result.flags).copied());
    for (raised, condition) in [
        (result.status.rejected, ShotConditions::REJECTED),
        (result.status.inconsistent, ShotConditions::INCONSISTENT),
        (result.status.unresolved, ShotConditions::UNRESOLVED),
    ] {
        if raised {
            readouts.conditions |= condition;
        }
    }
}
