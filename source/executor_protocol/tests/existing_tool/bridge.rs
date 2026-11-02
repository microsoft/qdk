// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bridge: an executor that runs an existing backend on the protocol,
//! taking its instructions as calls ([`InstructionCall`]), written with
//! [`executor_fn`], which keeps the protocol's rules.

use std::sync::Arc;

use executor_protocol::{
    CallInput, Capabilities, Close, Describe, ErrorKind, Executor, ExecutorReport,
    InstructionBatch, InstructionCall, InstructionSet, Operand, Outcome, Value, check_unique_tags,
    executor_fn,
};

use crate::backend::{ExistingInterface, NamedInstruction, Reading};

// Known gap, open: the bridge drops its backend when it closes, and gives no
// way to read it after a run.

/// The bridge: the executor around `backend`, which names its instructions
/// in its own way, running the calls of `set`, whose instructions it names
/// as `naming` does: pairs of a mnemonic of `set` and the backend's name.
///
/// The backend's state is one shot's: the bridge runs the shot of its first
/// call alone, and continues it from one call to the next, since the
/// backend keeps its state between batches; it raises no shot condition, so
/// it has none to keep. It promises that, its set and its one state
/// (`Capabilities::default()` with resumable shots), and no other limit:
/// an existing backend does not say how many qubits it has, so no qubit
/// count is checked over it, and a program that needs more than the backend
/// has fails at run time, by the backend's `Err`. Each instruction of `set`
/// is named once, here, so that a call is translated by its opcode. A
/// call's tags are not sent: the backend returns its results in the order
/// of the batch, and the bridge gives the `k`-th result the `k`-th tag. The
/// backend's `Err` does not say whether the batch started, so it ends the
/// reply `Failed`, as does a count of results that is not the count of
/// tags; either fails the bridge, whose backend's state is then not the
/// one its caller expects.
///
/// # Errors
///
/// Each mnemonic of `set` not in `naming`.
pub fn backend_bridge<B: ExistingInterface>(
    backend: B,
    set: Arc<InstructionSet>,
    naming: &[(&str, &'static str)],
) -> Result<
    impl Executor<InstructionBatch<InstructionCall>> + Describe + Close<Report = ExecutorReport<()>>,
    Vec<String>,
> {
    let names = set.table(|mnemonic| {
        let pair = naming.iter().find(|(ours, _)| *ours == mnemonic);
        pair.map(|&(_, theirs)| theirs)
    })?;
    let promise = Capabilities::default()
        .with_instruction_set(Some(set.clone()))
        .with_resumable_shots(true);
    let mut backend = Some(backend);
    let bridge = executor_fn(
        promise,
        move |_| {
            Ok(backend
                .take()
                .expect("a one-state executor makes its state once"))
        },
        // The calls' type is named here, since the program reads their
        // fields before anything else says what they are.
        move |backend, input: &CallInput<'_, InstructionCall>, outcomes| {
            let calls = input.instructions();
            let instructions: Vec<_> = calls
                .iter()
                .map(|call| NamedInstruction {
                    name: names[call.opcode],
                    qubits: (call.operands.iter())
                        .filter_map(|operand| match *operand {
                            Operand::Block(q) => Some(q),
                            _ => None,
                        })
                        .collect(),
                })
                .collect();
            let tags: Vec<u32> = call_tags(calls).map(|(_, tag)| tag).collect();
            let results = backend.execute_batch(&instructions)?;
            if results.len() != tags.len() {
                return Err(format!("{} results for {} tags", results.len(), tags.len()));
            }
            for (tag, reading) in tags.into_iter().zip(results) {
                outcomes.push(Outcome::new(tag, reading.into()));
            }
            Ok(())
        },
    );
    Ok(bridge.check(move |input| {
        set.check_batch(input.instructions())?;
        check_unique_tags(call_tags(input.instructions()))
    }))
}

/// A result, as a [`Value`]: a lost qubit is an error that says so.
impl From<Reading> for Value {
    fn from(reading: Reading) -> Self {
        match reading {
            Reading::Zero => Value::Zero,
            Reading::One => Value::One,
            Reading::Lost => Value::Error(ErrorKind::Loss),
        }
    }
}

/// The tags of `batch`, each with the index of its call.
fn call_tags(batch: &[InstructionCall]) -> impl Iterator<Item = (usize, u32)> + '_ {
    batch.iter().enumerate().flat_map(|(at, call)| {
        call.operands
            .iter()
            .filter_map(move |operand| match *operand {
                Operand::Tag(tag) => Some((at, tag)),
                _ => None,
            })
    })
}
