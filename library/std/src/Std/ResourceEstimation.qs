// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.



// Functionality needed to instruct the resource estimator to cache estimates of a code fragment
// and reuse these estimates without executing the code fragment repeatedly. This functionality
// is only available when using resource estimator execution target. `BeginCostCaching`
// and `EndCostCaching` are not defined for other execution targets.

/// # Summary
/// Used to specify that there's only one execution variant in `BeginEstimateCaching`
/// function
function SingleVariant() : Int {
    return 0;
}

/// # Summary
/// Informs the resource estimator of the start of the code fragment
/// for which estimates caching can be done. This function
/// is only available when using resource estimator execution target.
///
/// # Input
/// ## name
/// The name of the code fragment. Used to distinguish it from other code fragments.
/// Typically this is the name of the operation for which estimates can be cached.
/// ## variant
/// Specific variant of the execution. Cached estimates can only be reused if the
/// variant for which they were collected and the current variant is the same.
///
/// # Output
/// `true` indicated that the cached estimates are not yet available and the code fragment
/// needs to be executed in order to collect and cache estimates.
/// `false` indicates if cached estimates have been incorporated into the overall costs
/// and the code fragment should be skipped.
function BeginEstimateCaching(name : String, variant : Int) : Bool {
    body intrinsic;
}

/// # Summary
/// Instructs the resource estimator to stop estimates caching
/// because the code fragment in consideration is over. This function
/// is only available when using resource estimator execution target.
function EndEstimateCaching() : Unit {
    body intrinsic;
}

// Functionality needed to account for the resource estimates of an operation
// that is not implemented. Estimates that are obtained separately and passed to the
// `AccountForEstimates` become incorporated into the overall program estimates.
// This functionality is only available when using resource estimator execution target.
// `AccountForEstimates' is not defined for other execution targets.

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the number of auxiliary qubits is equal to the `amount`.
function AuxQubitCount(amount : Int) : (Int, Int) {
    return (0, amount);
}

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the number of the T gates is equal to the `amount`.
function TCount(amount : Int) : (Int, Int) {
    return (1, amount);
}

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the number of rotations is equal to the `amount`.
function RotationCount(amount : Int) : (Int, Int) {
    return (2, amount);
}

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the rotation depth is equal to the `amount`.
function RotationDepth(amount : Int) : (Int, Int) {
    return (3, amount);
}

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the number of the CCZ gates is equal to the `amount`.
function CczCount(amount : Int) : (Int, Int) {
    return (4, amount);
}

/// # Summary
/// Returns a tuple that can be passed to the `AccountForEstimates` operation
/// to specify that the number Measurements is equal to the `amount`.
function MeasurementCount(amount : Int) : (Int, Int) {
    return (5, amount);
}

/// # Summary
/// Pass the value returned by the function to the `AccountForEstimates` operation
/// to indicate Parallel Synthesis Sequential Pauli Computation (PSSPC) layout.
/// See https://arxiv.org/pdf/2211.07629.pdf for details.
function PSSPCLayout() : Int {
    return 1;
}

/// # Summary
/// Account for the resource estimates of an unimplemented operation,
/// which were obtained separately. This operation is only available
/// when using resource estimator execution target.
/// # Input
/// ## cost
/// Array of tuples containing resource estimates of the operation. For example,
/// if the operation uses three T gates, pass the tuple returned by TCount(3)
/// as one of the array elements.
/// ## layout
/// Provides the layout scheme that is used to convert logical resource estimates
/// to physical resource estimates. Only PSSPCLayout() is supported at this time.
/// ## arguments
/// Operation takes these qubits as its arguments.
operation AccountForEstimates(estimates : (Int, Int)[], layout : Int, arguments : Qubit[]) : Unit is Adj {
    body ... {
        AccountForEstimatesInternal(estimates, layout, arguments);
    }
    adjoint self;
}

internal operation AccountForEstimatesInternal(estimates : (Int, Int)[], layout : Int, arguments : Qubit[]) : Unit {
    body intrinsic;
}

/// # Summary
/// Instructs the resource estimator to assume that the resources from the
/// call of this operation until a call to `EndRepeatEstimates` are
/// accounted for `count` times, without the need to execute the code that many
/// times. Calls to `BeginRepeatEstimates` and `EndRepeatEstimates` can be nested.
/// A helper operation `RepeatEstimates` allows to call the two functions in a
/// `within` block.
///
/// # Input
/// ## count
/// Assumed number of repetitions, factor to multiply the cost with
operation BeginRepeatEstimates(count : Int) : Unit {
    body ... {
        BeginRepeatEstimatesInternal(count);
    }
    adjoint self;
}

internal operation BeginRepeatEstimatesInternal(count : Int) : Unit {
    body intrinsic;
}

/// # Summary
/// Companion operation to `BeginRepeatEstimates`.
operation EndRepeatEstimates() : Unit {
    body ... {
        EndRepeatEstimatesInternal();
    }
    adjoint self;
}

internal operation EndRepeatEstimatesInternal() : Unit {
    body intrinsic;
}

/// # Summary
/// Instructs the resource estimator to assume that the resources from the
/// call of this operation until a call to `Adjoint RepeatEstimates` are
/// accounted for `count` times, without the need to execute the code that many
/// times.
///
/// # Input
/// ## count
/// Assumed number of repetitions, factor to multiply the cost with
operation RepeatEstimates(count : Int) : Unit is Adj {
    body ... {
        BeginRepeatEstimates(count);
    }
    adjoint ... {
        EndRepeatEstimates();
    }
}

/// # Summary
/// Instructs the resource estimator to assume a memory/compute architecture,
/// in which computation can only happen on compute qubits.  The estimator will
/// automatically move qubits from and back into the memory such that never
/// more than `computeCapacity` qubits are used to compute at any time.
///
/// # Input
/// ## computeCapacity
/// The maximum number of compute qubits which can be used to perform
/// operations.
/// ## strategy
/// The strategy applied when evicting qubits from the compute qubits in case
/// of maximum capacity: 0 = LRU (least recently used), 1 = LFU (least
/// frequently used)
function EnableMemoryComputeArchitecture(computeCapacity : Int, strategy : Int) : Unit {
    body intrinsic;
}

/// # Summary
/// The LRU (least recently used) strategy that can be used as input to
/// `EnableMemoryComputeArchitecture`.
function LeastRecentlyUsed() : Int {
    return 0;
}

/// # Summary
/// The LFU (least frequently used) strategy that can be used as input to
/// `EnableMemoryComputeArchitecture`.
function LeastFrequentlyUsed() : Int {
    return 1;
}

export
    SingleVariant,
    BeginEstimateCaching,
    EndEstimateCaching,
    AuxQubitCount,
    TCount,
    RotationCount,
    RotationDepth,
    CczCount,
    MeasurementCount,
    PSSPCLayout,
    AccountForEstimates,
    BeginRepeatEstimates,
    EndRepeatEstimates,
    RepeatEstimates,
    EnableMemoryComputeArchitecture,
    LeastRecentlyUsed,
    LeastFrequentlyUsed;
