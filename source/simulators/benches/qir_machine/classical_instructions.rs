// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a classical instruction costs: a loop of arithmetic, comparison and
//! branch, three instructions a turn, about [`UNITS`] in all; it records
//! its counter at the end.

use criterion::Criterion;

use crate::workload::{counting_loop, program};
use crate::{UNITS, measure};

pub(crate) fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("classical_instructions");
    let [entry, turn, exit] = counting_loop(&[], UNITS / 3, None);
    measure(
        &mut group,
        "add_compare_branch",
        &program(&[&entry, &turn, &exit]),
    );
    group.finish();
}
