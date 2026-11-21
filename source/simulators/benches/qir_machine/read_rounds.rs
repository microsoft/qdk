// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a round that stops the machine costs: [`UNITS`] rounds of a loop,
//! each a measurement, a read of its result (which ends the batch, so each
//! round is a stop of the machine and a resume), then the loop's count,
//! compare and branch; it records its counter at the end.

use criterion::Criterion;
use qdk_simulators::bytecode::Instruction;

use crate::encoding::{FLAG_SRC0_IMM, OP_READ_RESULT};
use crate::workload::{counting_loop, measure_z, op, program};
use crate::{UNITS, measure};

pub(crate) fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_rounds");
    let read = Instruction {
        dst: 2,
        src0: 0,
        ..op(OP_READ_RESULT, FLAG_SRC0_IMM)
    };
    let [entry, round, exit] = counting_loop(&[measure_z(0, 0), read], UNITS, None);
    measure(
        &mut group,
        "measure_read_branch",
        &program(&[&entry, &round, &exit]),
    );
    group.finish();
}
