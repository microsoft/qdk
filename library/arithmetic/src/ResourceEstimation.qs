// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.ResourceEstimation.BeginEstimateCaching;
import Std.ResourceEstimation.EndEstimateCaching;
import Std.ResourceEstimation.IsResourceEstimating;
import Std.ResourceEstimation.RepeatEstimates;

/// # Summary
/// Repeats an operation `num_iterations` times with resource-estimation-friendly behavior.
///
/// When running under resource estimation, this operation uses `RepeatEstimates`
/// and executes `iteration(0)` once to model the loop body cost without
/// classically iterating through all indices. During simulation/execution, it
/// executes `iteration(i)` for each `i` in `0 .. num_iterations - 1`.
///
/// # Input
/// ## num_iterations
/// Number of loop iterations.
/// ## iteration
/// Operation that implements one loop iteration and receives the iteration index.
operation Loop(num_iterations : Int, iteration : ((Int) => Unit)) : Unit {
    if (num_iterations == 0) {
        // Do nothing.
    } elif (IsResourceEstimating()) {
        within {
            RepeatEstimates(num_iterations);
        } apply {
            iteration(0);
        }
    } else {
        for i in 0..num_iterations-1 {
            iteration(i);
        }
    }
}

/// # Summary
/// Adjointable variant of `Loop` for adjointable iteration operations.
operation LoopA(num_iterations : Int, iteration : ((Int) => Unit is Adj)) : Unit is Adj {
    if (num_iterations == 0) {
        // Do nothing.
    } elif (IsResourceEstimating()) {
        within {
            RepeatEstimates(num_iterations);
        } apply {
            iteration(0);
        }
    } else {
        for i in 0..num_iterations-1 {
            iteration(i);
        }
    }
}

export Loop, LoopA;