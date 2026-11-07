// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where a context goes: its entry, conditions, `select`, branches,
//! `PHI`s, switches, and function calls and returns.

use super::super::{
    binary, branch, call, call_return, compare, jump, measure, op, phi, program, read, record_int,
    ret, set,
};
use super::{both, computed, computed_by, records, run, trap};
use crate::OutputRecord;
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_EQ, OP_ADD, OP_ALLOCA,
    OP_BRANCH, OP_CALL_RETURN, OP_ICMP, OP_LOAD, OP_READ_RESULT, OP_SELECT, OP_STORE, OP_SUB,
    OP_SWITCH, OPID_MZ, VOID_RETURN,
};
use crate::bytecode::{AdaptiveProgram, Function, Instruction, PhiNodeEntry, SwitchCase};
use crate::qir::{QirReadout, QirTrap};

/// A register holding 0, 1, 2 or 3 (2 as `add i1 true, true` leaves it
/// in the pass's 64-bit registers) as the condition of a `SELECT` of 11
/// or 22, and of a branch to a block recording 1 or to one recording 0.
/// (The condition is register 3, as register 0, which holds 0, is what an
/// unset field names.)
#[test]
fn a_condition_other_than_0_or_1_is_a_trap() {
    let select = Instruction {
        dst: 1,
        src0: 3,
        aux0: 11,
        aux1: 22,
        ..op(OP_SELECT, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    };
    let selecting = |condition| program(&[&[set(3, condition), select, record_int(1), ret()]]);
    let branching = |condition| {
        program(&[
            &[set(3, condition), branch(3, 1, 2)],
            &[set(2, 1), record_int(2), ret()],
            &[set(2, 0), record_int(2), ret()],
        ])
    };
    for (condition, chosen, taken) in [(0, 22, 0), (1, 11, 1)] {
        assert_eq!(
            records(&selecting(condition), |_| QirReadout::One),
            [OutputRecord::Int(chosen)],
            "the select on {condition}"
        );
        assert_eq!(
            records(&branching(condition), |_| QirReadout::One),
            [OutputRecord::Int(taken)],
            "the branch on {condition}"
        );
    }
    for condition in [2, 3] {
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_SELECT`,
        // `OP_BRANCH`) reads any value but 0 as true; the machine traps
        // instead, because such a value can only come from the pass, which
        // does not wrap narrow integer results (`add i1 true, true` leaves
        // 2, where LLVM gives false), and reading it as true hides the bug.
        let trap_at_1 = QirTrap::NotABit {
            instruction: 1,
            value: condition,
        };
        assert_eq!(
            trap(&selecting(condition)),
            trap_at_1,
            "the select on {condition}"
        );
        assert_eq!(
            trap(&branching(condition)),
            trap_at_1,
            "the branch on {condition}"
        );
    }
}

/// A `SELECT` of 11 or 22 on register 0, which holds 0.
#[test]
fn select_takes_its_false_value_when_its_condition_is_0() {
    let select = Instruction {
        dst: 1,
        src0: 0,
        aux0: 11,
        aux1: 22,
        ..op(OP_SELECT, FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    };
    assert_eq!(computed(select), 22);
}

/// `SELECT`s of register 1, which holds 3, or the constant 2, and of the
/// constant 2 or register 1, each on the constant 1 (true) and the
/// constant 0 (false).
#[test]
fn select_reads_each_value_as_a_constant_or_a_register_by_its_own_flag() {
    let select = |condition, flags, yes, no| Instruction {
        dst: 1,
        src0: condition,
        aux0: yes,
        aux1: no,
        ..op(OP_SELECT, flags | FLAG_SRC0_IMM)
    };
    assert_eq!(computed(select(1, FLAG_AUX1_IMM, 1, 2)), 3);
    assert_eq!(computed(select(0, FLAG_AUX1_IMM, 1, 2)), 2);
    assert_eq!(computed(select(1, FLAG_AUX0_IMM, 2, 1)), 2);
    assert_eq!(computed(select(0, FLAG_AUX0_IMM, 2, 1)), 3);
}

/// `SELECT`s of the constant 11 or 22: on the constant 1 (true), while
/// register 1 holds 0, and on register 1 (false).
#[test]
fn select_reads_its_condition_as_a_constant_or_a_register_by_its_flag() {
    let select = |flags| Instruction {
        dst: 1,
        src0: 1,
        aux0: 11,
        aux1: 22,
        ..op(OP_SELECT, flags | FLAG_AUX0_IMM | FLAG_AUX1_IMM)
    };
    assert_eq!(computed_by(&[set(1, 0), select(FLAG_SRC0_IMM)]), 11);
    assert_eq!(computed_by(&[set(1, 0), select(0)]), 22);
}

/// A branch to block 1, recording 1, or to block 2, recording 2: on the
/// constant 1 (true), while register 1 holds 0, and on register 1 (false).
#[test]
fn branch_reads_its_condition_as_a_constant_or_a_register_by_its_flag() {
    let taken = |flags| {
        let bytecode = program(&[
            &[Instruction {
                opcode: u64::from(OP_BRANCH) | flags,
                ..branch(1, 1, 2)
            }],
            &[set(1, 1), record_int(1), ret()],
            &[set(1, 2), record_int(1), ret()],
        ]);
        let (machine, run_shot) = both(&bytecode);
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_BRANCH`: `resolve_u64` of its condition
        // by its flag), hence we keep it so that the machine is a drop-in
        // replacement.
        assert_eq!(machine, run_shot, "the block `run_shot` takes");
        machine
    };
    assert_eq!(taken(FLAG_SRC0_IMM), [OutputRecord::Int(1)]);
    assert_eq!(taken(0), [OutputRecord::Int(2)]);
}

/// A program of two blocks, each recording a different integer, entered
/// at the second.
#[test]
fn a_context_starts_at_the_program_s_entry_block() {
    let bytecode = AdaptiveProgram {
        entry_block: 1,
        ..program(&[
            &[set(0, 1), record_int(0), ret()],
            &[set(0, 2), record_int(0), ret()],
        ])
    };
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(2)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `entry_block`), hence we keep it so that the
    // machine is a drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// A diamond: the entry block branches on a measurement to blocks 1 and
/// 2, which set registers 1 and 2 and join in block 3, whose `PHI` picks
/// one, as its block's first instruction and after a `MOV` of an
/// immediate.
#[test]
fn a_phi_takes_the_value_of_the_edge_taken() {
    let diamond = |join: &[Instruction<u64>]| {
        program(&[
            &[
                measure(OPID_MZ, 0, 0),
                read(OP_READ_RESULT, 0, 0),
                branch(0, 1, 2),
            ],
            &[set(1, 11), jump(3)],
            &[set(2, 22), jump(3)],
            join,
        ])
    };
    let first = diamond(&[phi(3, 0, 2), record_int(3), ret()]);
    let after_a_move = diamond(&[set(0, 0), phi(3, 0, 2), record_int(3), ret()]);
    for (name, mut bytecode) in [("first", first), ("after a move", after_a_move)] {
        bytecode.phi_entries = vec![
            PhiNodeEntry::from_tuple((1, 1)),
            PhiNodeEntry::from_tuple((2, 2)),
        ];
        assert_eq!(
            records(&bytecode, |_| QirReadout::One),
            [OutputRecord::Int(11)],
            "{name}"
        );
        assert_eq!(
            records(&bytecode, |_| QirReadout::Zero),
            [OutputRecord::Int(22)],
            "{name}"
        );
    }
}

/// Two `PHI`s that swap registers 0 and 1, as LLVM reads them; a run
/// copies them one after the other, as `run_shot` does. And a chain, into
/// register 0 from 1 then into 1 from 2: each copied once.
#[test]
fn the_phis_of_a_block_copy_in_order() {
    let cases = [
        // (entry for the first `PHI`, then the second's: from block 0,
        // the register copied)
        ((0, 1), (0, 0), [2, 2]),
        ((0, 1), (0, 2), [2, 3]),
    ];
    for (first, second, copied) in cases {
        let mut bytecode = program(&[
            &[set(0, 1), set(1, 2), set(2, 3), jump(1)],
            &[
                phi(0, 0, 1),
                phi(1, 1, 1),
                record_int(0),
                record_int(1),
                ret(),
            ],
        ]);
        bytecode.phi_entries = vec![
            PhiNodeEntry::from_tuple(first),
            PhiNodeEntry::from_tuple(second),
        ];
        let (machine, run_shot) = both(&bytecode);
        // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_PHI` copies as it runs), but LLVM
        // reads every `PHI` of a block before any writes, so the swap
        // gives [2, 1] there, so we keep it here meanwhile but we expect
        // discussion/feedback as part of the review. We think this is OK
        // for now because no corpus program has a `PHI` reading another's
        // destination, so we recommend fixing both runtimes together once
        // a producer emits one (an optimised LLVM loop can).
        assert_eq!(machine, copied.map(OutputRecord::Int), "{second:?}");
        assert_eq!(machine, run_shot, "{second:?}");
    }
}

/// A `PHI` with two entries for block 0, holding 11 and 22 (bytecode
/// that Python's pass does not emit, but that building accepts), reached
/// from block 0 by a jump, as its block's first instruction and after a
/// `MOV` of an immediate: each takes the first, as `run_shot` does.
#[test]
fn a_phi_takes_its_first_entry_for_the_block_it_comes_from() {
    let first = program(&[
        &[set(1, 11), set(2, 22), jump(1)],
        &[phi(3, 0, 2), record_int(3), ret()],
    ]);
    let after_a_move = program(&[
        &[set(1, 11), set(2, 22), jump(1)],
        &[set(0, 0), phi(3, 0, 2), record_int(3), ret()],
    ]);
    for (name, mut bytecode) in [("first", first), ("after a move", after_a_move)] {
        bytecode.phi_entries = vec![
            PhiNodeEntry::from_tuple((0, 1)),
            PhiNodeEntry::from_tuple((0, 2)),
        ];
        let (machine, run_shot) = both(&bytecode);
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_PHI` takes the first match), hence we
        // keep it so that the machine is a drop-in replacement; LLVM
        // requires repeated entries to carry the same value, so a valid
        // program cannot tell.
        assert_eq!(machine, [OutputRecord::Int(11)], "{name}");
        assert_eq!(machine, run_shot, "{name}");
    }
}

/// A `PHI` of one entry from offset 1 in the `PHI` entries, which give
/// register 1, holding 5, then register 3, holding 9, both for block 0;
/// reached from block 0 by a jump, as its block's first instruction and
/// after a `MOV` of an immediate: each takes register 3.
#[test]
fn a_phi_reads_its_entries_from_its_offset() {
    let first = program(&[
        &[set(1, 5), set(3, 9), jump(1)],
        &[phi(2, 1, 1), record_int(2), ret()],
    ]);
    let after_a_move = program(&[
        &[set(1, 5), set(3, 9), jump(1)],
        &[set(0, 0), phi(2, 1, 1), record_int(2), ret()],
    ]);
    for (name, mut bytecode) in [("first", first), ("after a move", after_a_move)] {
        bytecode.phi_entries = vec![
            PhiNodeEntry::from_tuple((0, 1)),
            PhiNodeEntry::from_tuple((0, 3)),
        ];
        let (machine, run_shot) = both(&bytecode);
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_PHI`: the entries at
        // `phi_entries[aux0..]`), hence we keep it so that the machine is
        // a drop-in replacement.
        assert_eq!(machine, [OutputRecord::Int(9)], "{name}");
        assert_eq!(machine, run_shot, "{name}");
    }
}

/// The entry block sets register 1 to 11, calls a function whose blocks
/// 2 and 3 set register 2 to 22 and return, then jumps to block 1, whose
/// `PHI` has entries for blocks 2 and 3 (register 2) and for block 0
/// (register 1); the `PHI` starts its block, or follows a `MOV` of an
/// immediate. Each takes the entry for block 0: the edge after a return
/// comes from the caller's block, not the callee's last.
#[test]
fn an_edge_after_a_return_comes_from_the_caller_s_block() {
    let callee = [&[set(2, 22), jump(3)][..], &[call_return(0)]];
    let first = program(&[
        &[set(1, 11), call(0, 0, 0, VOID_RETURN), jump(1)],
        &[phi(3, 0, 3), record_int(3), ret()],
        callee[0],
        callee[1],
    ]);
    let after_a_move = program(&[
        &[set(1, 11), call(0, 0, 0, VOID_RETURN), jump(1)],
        &[set(0, 0), phi(3, 0, 3), record_int(3), ret()],
        callee[0],
        callee[1],
    ]);
    for (name, mut bytecode) in [("first", first), ("after a move", after_a_move)] {
        bytecode.phi_entries = vec![
            PhiNodeEntry::from_tuple((2, 2)),
            PhiNodeEntry::from_tuple((3, 2)),
            PhiNodeEntry::from_tuple((0, 1)),
        ];
        bytecode.function_table = vec![Function::from_tuple((2, 0, 0))];
        let (machine, run_shot) = both(&bytecode);
        assert_eq!(machine, [OutputRecord::Int(11)], "{name}");
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, the edge into a block is the jump's, and
        // `OP_CALL_RETURN` restores the caller's), hence we keep it so
        // that the machine is a drop-in replacement; the machine holds it
        // by construction (the edge is the jump's, fixed when built).
        assert_eq!(machine, run_shot, "{name}");
    }
}

/// A switch on register 0, holding 5, whose cases list 5 twice: to block
/// 1, then to block 2; each block records its number. It goes to block 1,
/// as `run_shot` does.
#[test]
fn a_switch_takes_its_first_case_for_the_value() {
    let switch = Instruction {
        src0: 0,
        aux0: 3,
        aux1: 0,
        aux2: 2,
        ..op(OP_SWITCH, 0)
    };
    let mut bytecode = program(&[
        &[set(0, 5), switch],
        &[set(1, 1), record_int(1), ret()],
        &[set(1, 2), record_int(1), ret()],
        &[set(1, 3), record_int(1), ret()],
    ]);
    bytecode.switch_cases = vec![
        SwitchCase::from_tuple((5, 1)),
        SwitchCase::from_tuple((5, 2)),
    ];
    let (machine, run_shot) = both(&bytecode);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_SWITCH` takes the first match), hence we
    // keep it so that the machine is a drop-in replacement; LLVM forbids
    // a repeated case value, so a valid program cannot tell.
    assert_eq!(machine, [OutputRecord::Int(1)]);
    assert_eq!(machine, run_shot);
}

/// A switch on register 0, holding 7, whose one case is 5, to block 1; its
/// default is block 3, its cases start at entry 2 of the table (entries 0
/// and 1, not its own, take 7 to block 2). Each block records its number.
/// It goes to block 3, as `run_shot` does.
#[test]
fn a_switch_takes_its_default_when_no_case_has_the_value() {
    let switch = Instruction {
        src0: 0,
        aux0: 3,
        aux1: 2,
        aux2: 1,
        ..op(OP_SWITCH, 0)
    };
    let mut bytecode = program(&[
        &[set(0, 7), switch],
        &[set(1, 1), record_int(1), ret()],
        &[set(1, 2), record_int(1), ret()],
        &[set(1, 3), record_int(1), ret()],
    ]);
    bytecode.switch_cases = vec![
        SwitchCase::from_tuple((7, 2)),
        SwitchCase::from_tuple((7, 2)),
        SwitchCase::from_tuple((5, 1)),
    ];
    let (machine, run_shot) = both(&bytecode);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_SWITCH` reads only its own cases), hence
    // we keep it so that the machine is a drop-in replacement.
    assert_eq!(machine, [OutputRecord::Int(3)]);
    assert_eq!(machine, run_shot);
}

/// A switch on register 0, holding 2^32 + 5, whose one case is 5, to block
/// 1; its default is block 2. Each block records its number.
#[test]
fn a_switch_compares_every_bit_of_its_value() {
    let switch = Instruction {
        src0: 0,
        aux0: 2,
        aux1: 0,
        aux2: 1,
        ..op(OP_SWITCH, 0)
    };
    let mut bytecode = program(&[
        &[set(0, (1 << 32) | 5), switch],
        &[set(1, 1), record_int(1), ret()],
        &[set(1, 2), record_int(1), ret()],
    ]);
    bytecode.switch_cases = vec![SwitchCase::from_tuple((5, 1))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(2)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_SWITCH` compares the whole word), hence we
    // keep it so that the machine is a drop-in replacement; LLVM compares
    // at the switch's width, which is `i64` for Q#.
    assert_eq!(machine, run_shot);
}

/// A switch whose one case is 2, to block 1; its default is block 2. Each
/// block records its number. On the constant 2, while register 2 holds 0,
/// and on register 2.
#[test]
fn switch_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    let taken = |flags| {
        let switch = Instruction {
            src0: 2,
            aux0: 2,
            aux1: 0,
            aux2: 1,
            ..op(OP_SWITCH, flags)
        };
        let mut bytecode = program(&[
            &[switch],
            &[set(1, 1), record_int(1), ret()],
            &[set(1, 2), record_int(1), ret()],
        ]);
        bytecode.switch_cases = vec![SwitchCase::from_tuple((2, 1))];
        let (machine, run_shot) = both(&bytecode);
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_SWITCH`: `resolve_u64` of its value by
        // its flag), hence we keep it so that the machine is a drop-in
        // replacement.
        assert_eq!(machine, run_shot, "the block `run_shot` takes");
        machine
    };
    assert_eq!(taken(FLAG_SRC0_IMM), [OutputRecord::Int(1)]);
    assert_eq!(taken(0), [OutputRecord::Int(2)]);
}

/// A function whose parameters are registers 0 and 1, called with
/// registers 1 and 0, holding 1 and 2: its parameters swapped, which it
/// records. Each parameter takes its argument's value at the function
/// call, so they
/// hold 2 and 1, though the first is written before the second is read.
#[test]
fn a_function_call_passes_each_argument_s_value_at_the_call() {
    let mut bytecode = program(&[
        &[set(0, 1), set(1, 2), call(0, 2, 0, VOID_RETURN), ret()],
        &[record_int(0), record_int(1), call_return(0)],
    ]);
    bytecode.call_args = vec![1, 0];
    bytecode.function_table = vec![Function::from_tuple((1, 2, 0))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(2), OutputRecord::Int(1)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL` reads each argument from the
    // caller's saved registers), hence we keep it so that the machine is a
    // drop-in replacement; LLVM passes every argument's value at the call.
    assert_eq!(machine, run_shot);
}

/// A function whose first parameter register is 2^32, past the program's
/// registers and past `u32` too, called with no arguments: the function
/// call names
/// no parameter register, so the program builds and runs as `run_shot`
/// runs it, which copies nothing.
#[test]
fn a_function_call_with_no_arguments_runs_whatever_its_parameter_base() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, VOID_RETURN), set(0, 7), record_int(0), ret()],
        &[call_return(0)],
    ]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 1 << 32))];
    let (machine, run_shot) = both(&bytecode);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL` copies no argument), hence we keep
    // it so that the machine is a drop-in replacement.
    assert_eq!(machine, [OutputRecord::Int(7)]);
    assert_eq!(machine, run_shot);
}

/// A function call of one argument from offset 1 in the `call_args`
/// table, which lists
/// registers 3 and 2, holding 6 and 5: the function, whose parameter is
/// register 0, records it.
#[test]
fn a_function_call_reads_its_arguments_from_its_offset() {
    let mut bytecode = program(&[
        &[set(2, 5), set(3, 6), call(0, 1, 1, VOID_RETURN), ret()],
        &[record_int(0), call_return(0)],
    ]);
    bytecode.call_args = vec![3, 2];
    bytecode.function_table = vec![Function::from_tuple((1, 1, 0))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(5)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL`: the arguments at `call_args[aux2..]`),
    // hence we keep it so that the machine is a drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// A function whose parameters are registers 2 and 3, called with
/// registers 1 and 0, holding 5 and 6; the function records registers 2
/// and 3.
#[test]
fn a_function_call_copies_its_arguments_into_its_function_s_parameter_registers() {
    let mut bytecode = program(&[
        &[set(1, 5), set(0, 6), call(0, 2, 0, VOID_RETURN), ret()],
        &[record_int(2), record_int(3), call_return(0)],
    ]);
    bytecode.call_args = vec![1, 0];
    bytecode.function_table = vec![Function::from_tuple((1, 2, 2))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(5), OutputRecord::Int(6)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL`: argument `i` into register
    // `param_base_reg + i`), hence we keep it so that the machine is a
    // drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// A function setting register 3 to 7 and returning it, called from the
/// entry block, which keeps the value in register 2 and records it: the
/// value is read from the function's registers, and written once the
/// caller's are back.
#[test]
fn a_function_call_keeps_its_return_value() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, 2), record_int(2), ret()],
        &[set(3, 7), call_return(3)],
    ]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(7)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL_RETURN` writes its value to the
    // function call's register), hence we keep it so that the machine is a
    // drop-in
    // replacement.
    assert_eq!(machine, run_shot);
}

/// A function returning 2 or register 2, which holds 0, called from the
/// entry block, which keeps the value in register 1 and records it.
#[test]
fn call_return_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    let returned = |flags| {
        let mut bytecode = program(&[
            &[call(0, 0, 0, 1), record_int(1), ret()],
            &[Instruction {
                src0: 2,
                ..op(OP_CALL_RETURN, flags)
            }],
        ]);
        bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
        let (machine, run_shot) = both(&bytecode);
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_CALL_RETURN`: `resolve_u64` of its
        // value by its flag), hence we keep it so that the machine is a
        // drop-in replacement.
        assert_eq!(machine, run_shot, "the value `run_shot` returns");
        machine
    };
    assert_eq!(returned(FLAG_SRC0_IMM), [OutputRecord::Int(2)]);
    assert_eq!(returned(0), [OutputRecord::Int(0)]);
}

/// The entry block sets registers 1 and 2 to 5 and 4, then calls a
/// function whose parameter is register 2, with register 1, and which sets
/// register 1 to 9; after the function call, the caller records registers
/// 1 and 2: 5 and 4, as before it.
#[test]
fn a_function_s_register_writes_are_undone_at_its_return() {
    let mut bytecode = program(&[
        &[
            set(1, 5),
            set(2, 4),
            call(0, 1, 0, VOID_RETURN),
            record_int(1),
            record_int(2),
            ret(),
        ],
        &[set(1, 9), call_return(0)],
    ]);
    bytecode.call_args = vec![1];
    bytecode.function_table = vec![Function::from_tuple((1, 1, 2))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(5), OutputRecord::Int(4)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL_RETURN` restores the registers
    // `OP_CALL` saved), hence we keep it so that the machine is a drop-in
    // replacement; in LLVM, each function call has its own registers.
    assert_eq!(machine, run_shot);
}

/// `sum(n)`, which returns 0 for 0 and else `n + sum(n - 1)`, called with
/// 3, and its value recorded: 6. Its `n` is register 0, which the
/// function call inside it overwrites until it returns.
#[test]
fn a_function_may_call_itself() {
    let is_zero = Instruction {
        opcode: compare(OP_ICMP, ICMP_EQ).opcode | FLAG_SRC1_IMM,
        dst: 1,
        src0: 0,
        src1: 0,
        ..Instruction::default()
    };
    let zero = Instruction {
        src0: 0,
        ..op(OP_CALL_RETURN, FLAG_SRC0_IMM)
    };
    let mut bytecode = program(&[
        &[set(3, 3), call(0, 1, 0, 2), record_int(2), ret()],
        &[is_zero, branch(1, 2, 3)],
        &[zero],
        &[
            binary(OP_SUB, FLAG_SRC1_IMM, 1, 0, 1),
            call(0, 1, 1, 2),
            binary(OP_ADD, 0, 2, 0, 2),
            call_return(2),
        ],
    ]);
    bytecode.call_args = vec![3, 1];
    bytecode.function_table = vec![Function::from_tuple((1, 1, 0))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(6)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL` saves the caller's registers), hence
    // we keep it so that the machine is a drop-in replacement; the Q#
    // compiler emits such recursion (the Python package's
    // `test_recursive_call_frames.py`).
    assert_eq!(machine, run_shot);
}

/// One word of constant data; a function that allocates a word, stores 7
/// there and returns its address; the caller then allocates a word too,
/// and records both addresses and its word: the same address, 1, and 0,
/// since the function's word was freed at its return.
#[test]
fn an_alloca_is_freed_at_its_function_s_return() {
    let one_word = |dst| binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, dst, 1, 0);
    let mut bytecode = program(&[
        &[
            call(0, 0, 0, 1),
            one_word(2),
            binary(OP_LOAD, 0, 3, 2, 0),
            record_int(1),
            record_int(2),
            record_int(3),
            ret(),
        ],
        &[
            one_word(0),
            binary(OP_STORE, FLAG_SRC0_IMM, 0, 7, 0),
            call_return(0),
        ],
    ]);
    bytecode.constant_data = vec![5];
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(
        machine,
        [
            OutputRecord::Int(1),
            OutputRecord::Int(1),
            OutputRecord::Int(0)
        ]
    );
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL_RETURN` truncates the memory to its
    // length at the function call), hence we keep it so that the machine
    // is a
    // drop-in replacement; in LLVM, an `alloca`'s words live until its
    // function returns.
    assert_eq!(machine, run_shot);
}

/// The caller allocates a word and passes its address to a function,
/// which stores 7 there, then allocates a word of its own; after the
/// return, the caller loads its word and records it: 7, since only the
/// function's own words are freed.
#[test]
fn a_function_s_store_to_its_caller_s_words_outlives_its_return() {
    let mut bytecode = program(&[
        &[
            binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 1, 0),
            call(0, 1, 0, VOID_RETURN),
            binary(OP_LOAD, 0, 1, 0, 0),
            record_int(1),
            ret(),
        ],
        &[
            binary(OP_STORE, FLAG_SRC0_IMM, 0, 7, 1),
            binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 1, 0),
            call_return(0),
        ],
    ]);
    bytecode.call_args = vec![0];
    bytecode.function_table = vec![Function::from_tuple((1, 1, 1))];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(7)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_CALL_RETURN` keeps the memory up to its
    // length at the function call), hence we keep it so that the machine
    // is a
    // drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// A function measuring qubit 0 into result 0, reading it into register 0
/// and returning it, called from the entry block, which keeps it in
/// register 1 and records it: the context stops in the function, before
/// the read, and once resumed reads the outcome, returns, and records it.
#[test]
fn a_context_stopped_in_a_function_resumes_there_and_returns_after_the_function_call() {
    let mut bytecode = program(&[
        &[call(0, 0, 0, 1), record_int(1), ret()],
        &[
            measure(OPID_MZ, 0, 0),
            read(OP_READ_RESULT, 0, 0),
            call_return(0),
        ],
    ]);
    bytecode.function_table = vec![Function::from_tuple((1, 0, 0))];
    let (batches, records) = run(&bytecode, |_| QirReadout::One);
    assert_eq!(batches, ["mz(qubit 0, result 0)"]);
    assert_eq!(records, [OutputRecord::Int(1)]);
}
