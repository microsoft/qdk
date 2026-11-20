// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Validate loop-carried releases of identities live on entry, using the emitted CFG.
//! This analysis never evaluates FIR or emits instructions. Loop-local allocation
//! generations remain the responsibility of the allocator and callable analysis.

use crate::{Error, EvalError};
use qsc_eval::{PackageSpan, val::Qubit};
use qsc_rir::rir::{BlockId, Instruction, Literal, Operand, Program, VariableId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) struct Event {
    pub block: BlockId,
    pub offset: usize,
    pub qubit: Rc<Qubit>,
    pub release: bool,
    pub span: PackageSpan,
}

pub(crate) struct EntryState {
    pub released: bool,
    pub booleans: BTreeMap<VariableId, bool>,
}

pub(crate) struct Input {
    pub qubit: Rc<Qubit>,
    pub entry_states: Vec<EntryState>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Path {
    block: BlockId,
    released: bool,
    backedge: bool,
    // Keep alternatives separate so a return flag and its cleanup stay correlated.
    booleans: BTreeMap<VariableId, bool>,
}

fn boolean(operand: &Operand, values: &BTreeMap<VariableId, bool>) -> Option<bool> {
    match operand {
        Operand::Literal(Literal::Bool(value)) => Some(*value),
        Operand::Variable(variable) => values.get(&variable.variable_id).copied(),
        Operand::Literal(_) => None,
    }
}

fn transfer(instruction: &Instruction, values: &mut BTreeMap<VariableId, bool>) {
    let (output, value) = match instruction {
        Instruction::Store(input, output) | Instruction::Convert(input, output) => {
            (*output, boolean(input, values))
        }
        Instruction::Load(input, output) => (*output, values.get(&input.variable_id).copied()),
        Instruction::LogicalNot(input, output) => (*output, boolean(input, values).map(|v| !v)),
        Instruction::LogicalAnd(left, right, output) => (
            *output,
            match (boolean(left, values), boolean(right, values)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
        ),
        Instruction::LogicalOr(left, right, output) => (
            *output,
            match (boolean(left, values), boolean(right, values)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
        ),
        Instruction::Call(_, _, Some(output), _)
        | Instruction::StoreArray(_, output)
        | Instruction::Add(_, _, output)
        | Instruction::Sub(_, _, output)
        | Instruction::Mul(_, _, output)
        | Instruction::Sdiv(_, _, output)
        | Instruction::Srem(_, _, output)
        | Instruction::Shl(_, _, output)
        | Instruction::Ashr(_, _, output)
        | Instruction::Fadd(_, _, output)
        | Instruction::Fsub(_, _, output)
        | Instruction::Fmul(_, _, output)
        | Instruction::Fdiv(_, _, output)
        | Instruction::Frem(_, _, output)
        | Instruction::Fcmp(_, _, _, output)
        | Instruction::Icmp(_, _, _, output)
        | Instruction::BitwiseNot(_, output)
        | Instruction::BitwiseAnd(_, _, output)
        | Instruction::BitwiseOr(_, _, output)
        | Instruction::BitwiseXor(_, _, output)
        | Instruction::Phi(_, output)
        | Instruction::Alloca(output)
        | Instruction::Index(_, _, output) => (*output, None),
        Instruction::Call(_, _, None, _)
        | Instruction::Jump(_)
        | Instruction::Branch(..)
        | Instruction::Return(_) => return,
    };
    if let Some(value) = value {
        values.insert(output.variable_id, value);
    } else {
        // In particular, a new measurement must invalidate the previous iteration's fact.
        values.remove(&output.variable_id);
    }
}

pub(crate) fn validate(
    program: &Program,
    header: BlockId,
    continuation: BlockId,
    events: &[Event],
    inputs: &[Input],
    entry_literals: &FxHashMap<VariableId, Literal>,
) -> Result<(), Error> {
    for input in inputs {
        let relevant: Vec<_> = events
            .iter()
            .filter(|event| Rc::ptr_eq(&event.qubit, &input.qubit))
            .collect();
        if !relevant.iter().any(|event| event.release)
            && !input.entry_states.iter().any(|entry| entry.released)
        {
            continue;
        }
        let mut pending = input
            .entry_states
            .iter()
            .map(|entry| Path {
                block: header,
                released: entry.released,
                backedge: false,
                booleans: entry_literals
                    .iter()
                    .filter_map(|(id, literal)| {
                        if let Literal::Bool(value) = literal {
                            Some((*id, *value))
                        } else {
                            None
                        }
                    })
                    .chain(entry.booleans.iter().map(|(id, value)| (*id, *value)))
                    .collect(),
            })
            .collect::<Vec<_>>();
        let mut visited = FxHashSet::default();
        while let Some(mut path) = pending.pop() {
            if path.block == continuation || !visited.insert(path.clone()) {
                continue;
            }
            let block = program.get_block(path.block);
            for offset in 0..=block.0.len() {
                for event in relevant
                    .iter()
                    .filter(|event| event.block == path.block && event.offset == offset)
                {
                    if path.released && path.backedge {
                        return Err(if event.release {
                            EvalError::QubitDoubleRelease(event.span)
                        } else {
                            EvalError::QubitUsedAfterRelease(event.span)
                        }
                        .into());
                    }
                    path.released |= event.release;
                }
                let Some(instruction) = block.0.get(offset) else {
                    break;
                };
                match instruction {
                    Instruction::Jump(target) => {
                        let mut next = path.clone();
                        next.block = *target;
                        next.backedge |= *target == header;
                        pending.push(next);
                    }
                    Instruction::Branch(condition, yes, no, _) => {
                        for (value, target) in [(true, *yes), (false, *no)] {
                            if path
                                .booleans
                                .get(&condition.variable_id)
                                .is_some_and(|known| *known != value)
                            {
                                continue;
                            }
                            let mut next = path.clone();
                            next.booleans.insert(condition.variable_id, value);
                            next.block = target;
                            next.backedge |= target == header;
                            pending.push(next);
                        }
                    }
                    _ => transfer(instruction, &mut path.booleans),
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tests::get_rir_program_with_capabilities;
    use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
    use qsc_rir::rir::Instruction;

    #[test]
    fn release_on_exit_branch_preserves_other_backedge_and_emits_effect_once() {
        let program = get_rir_program_with_capabilities(
            r#"
            namespace Test {
                operation Op(q : Qubit) : Unit { body intrinsic; }
                @EntryPoint()
                operation Main() : Unit {
                    use control = Qubit();
                    let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                    mutable keep = MResetZ(control) == One;
                    while keep {
                        Op(q);
                        if MResetZ(control) == One {
                            QIR.Runtime.__quantum__rt__qubit_release(q);
                            set keep = false;
                        }
                    }
                }
            }
            "#,
            TargetCapabilityFlags::from(Profile::AdaptiveRIF)
                | TargetCapabilityFlags::BackwardsBranching,
        );
        let op = program
            .callables
            .iter()
            .find(|(_, callable)| callable.name == "Op")
            .expect("Op should be emitted")
            .0;
        let calls = program
            .blocks
            .iter()
            .flat_map(|(_, block)| &block.0)
            .filter(|instruction| matches!(instruction, Instruction::Call(id, ..) if *id == op))
            .count();
        assert_eq!(
            calls, 1,
            "analysis must not emit another copy of the effect"
        );
    }
}
