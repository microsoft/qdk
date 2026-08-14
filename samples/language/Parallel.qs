// # Sample
// Parallel Expressions
//
// # Description
// The `parallel` keyword allows for designating an expression that should follow
// different qubit allocation rules and code generation patterns to optimize for
// parallel execution. While it does not guarantee that the expression will be executed
// in parallel, it provides the hint to the compiler that the resource utilization
// should optimize for "depth" or time rather than "width" or space.

operation Main() : Unit {
    use qs = Qubit[6];

    // This operation uses a normal for loop over calls to joint measurement,
    // which result in the ancilla qubit being reused across loop iterations.
    // This matches the default behavior in Q# of having qubit reuse optimize for
    // width/space by using fewer qubits but prevents the two iterations from being
    // parallelized during execution.
    JointMeasure(qs);

    // This operation uses `parallel for ...` to change the qubit allocation behavior
    // and instead optimize for depth/time, as well as force the loop to be unrolled when
    // compiling for hardware. As a result, the two iterations use distinct qubits and can
    // potentially be executed in parallel by the target quantum system.
    ParallelJointMeasure(qs);

    // Since `parallel` can apply to any expression, we can make `JointMeasure` have
    // the same behavior as `ParallelJointMeasure` without modifying the body by
    // using it at the call site instead. This allows for changing the behavior of
    // library code without needing to modify the library directly.
    parallel JointMeasure(qs);

    // If instead of full parallelism only limited parallelism is desired,
    // use the `parallel within <int> <expr>` expression instead. This allows for
    // specifying an upper limit on the number of new allocations that are performed
    // before falling back to qubit reuse. In this case, by specifying a limit of
    // 2, the call to `JointMeasure` will use two distinct ancilla qubits for the
    // first two iterations of the loop and then reuse the first ancilla during
    // the third iteration.
    parallel within 2 JointMeasure(qs);
}

operation JointMeasure(qs : Qubit[]) : Unit {
    // Jointly measures each pair of qubits in the given array.
    // Because `MeasureAllZ` uses allocates and releases an ancilla qubit,
    // each iteration of the loop will allocate the same ancilla, forcing
    // the execution of the loop to be sequential.
    for i in 0..2..Length(qs)-1 {
        let _ = MeasureAllZ(qs[i..i + 1]);
    }
}

operation ParallelJointMeasure(qs : Qubit[]) : Unit {
    // Jointly measures each pair of qubits in the given array.
    // Because this loop is part of a `parallel` expression,
    // released qubits will not be reused and each call to `MeasureAllZ`
    // will get a distinct qubit, allowing the resulting unrolled loop
    // to execute in parallel.
    // Once the `parallel` expression ends, all released qubits from that
    // scope will be available for reuse by later allocations.
    parallel for i in 0..2..Length(qs)-1 {
        let _ = MeasureAllZ(qs[i..i + 1]);
    }
}
