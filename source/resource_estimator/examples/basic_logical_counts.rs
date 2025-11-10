// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::cast_precision_loss)]

// This example illustrates how to use the Resource Estimator crate to perform a
// standard resource estimation on top of the qubit and QEC models provided by
// the systems architecture that we ship. We are using logical resource counts
// to compute the post-layout logical overhead based on the PSSPC layout method
// as described in [https://arxiv.org/abs/2211.07629].

use std::rc::Rc;

use resource_estimator::{
    estimates::{ErrorBudget, PhysicalResourceEstimation},
    system::{LogicalResourceCounts, PhysicalQubit, TFactoryBuilder, floquet_code},
};

fn main() {
    // There are 5 ingredients that we need to perform resource estimation.

    // 1) A quantum error correction code; in this example we are using a
    //    Floquet code.
    let code = floquet_code();

    // 2) A qubit model; in this example we are using a Majorana type qubit
    //    using a physical error rate of 1e-6.
    let qubit = Rc::new(PhysicalQubit::qubit_maj_ns_e6());

    // 3) A factory builder to provide magic states; in this example we are
    //    using a T factory builder that can create T factories using multiple
    //    distillation rounds.
    let builder = TFactoryBuilder::default();

    // 4) The logical resource overhead; in this example we are using logical
    //    resource counts that compute the logical post-layout overhead based on
    //    the PSSPC algorithm.
    let logical_counts = Rc::new(LogicalResourceCounts {
        num_qubits: 100,
        t_count: 10,
        rotation_count: 10,
        rotation_depth: 5,
        ccz_count: 100,
        ccix_count: 0,
        measurement_count: 10,
        num_compute_qubits: None,
        read_from_memory_count: None,
        write_to_memory_count: None,
    });

    // 5) An error budget; in this example we are using a uniform error budget
    //    of 0.1% distributed uniformly among logical errors, rotation synthesis
    //    errors, and T state production errors.
    let budget = ErrorBudget::from_uniform(0.001);

    // After we have set up all required inputs for the resource estimation
    // task, we can set up an estimation instance.
    let estimation = PhysicalResourceEstimation::new(code, qubit, builder, logical_counts);

    // In this example, we perform a standard estimation without any further
    // constraints.
    let result = estimation
        .estimate(&budget)
        .expect("estimation does not fail");

    // There is a lot of data contained in the resource estimation result
    // object, but in this sample we are only printing the total number of
    // physical qubits and the runtime in seconds (the value is returned in nano
    // seconds).
    println!("Number of physical qubits: {}", result.physical_qubits());
    println!(
        "Runtime:                   {:.2e} secs",
        result.runtime() as f64 / 1e9
    );
}
