// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a function call and its return cost: [`UNITS`] turns of a loop,
//! each a call of a function of one parameter, the counter, that returns
//! it, then the loop's count, compare and branch. It records the counter
//! and the value the last call returned, so that a wrong count of turns
//! or a wrong return of the last call changes the records; a lost earlier
//! call, whose return nothing reads, does not.

use criterion::Criterion;
use qdk_simulators::bytecode::{Function, Instruction};

use crate::encoding::{OP_CALL, OP_CALL_RETURN};
use crate::workload::{counting_loop, op, program};
use crate::{UNITS, measure};

pub(crate) fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("function_calls");
    // The call of function 0 with one argument, the register of the first
    // entry of the call arguments (register 0, the counter); register 2
    // keeps what it returns.
    let call = Instruction {
        dst: 2,
        aux0: 0,
        aux1: 1,
        aux2: 0,
        ..op(OP_CALL, 0)
    };
    // The function's one block returns its parameter, register 3.
    let returns = Instruction {
        src0: 3,
        ..op(OP_CALL_RETURN, 0)
    };
    let [entry, turn, exit] = counting_loop(&[call], UNITS, Some(2));
    let mut bytecode = program(&[&entry, &turn, &exit, &[returns]]);
    // Function 0: its entry block 3, one parameter, in register 3.
    bytecode.function_table = vec![Function::from_tuple((3, 1, 3))];
    measure(&mut group, "call_return", &bytecode);
    group.finish();
}
