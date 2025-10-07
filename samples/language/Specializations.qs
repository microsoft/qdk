// # Sample
// Specializations
//
// # Description
// Q# allows specialized implementations. Operations in Q# can implicitly
// or explicitly define adjoint and/or controlled versions.
// Q# employs symbolic computation that can automatically generate the
// corresponding adjoint and controlled implementations for a particular
// body implementation.

/// The adjoint, controlled and adjoint-controlled specializations are implicitly
/// generated for the `DoNothing` operation that declares supports for these
/// specializations using the `is` keyword followed by the union of the supported
/// specializations (`Adj + Ctl`).
operation DoNothing() : Unit is Adj + Ctl {}

/// Here, the specializations have been explicitly defined.
/// In the following example, the declaration for an operation SWAP,
/// which exchanges the state of two qubits q1 and q2, declares an
/// explicit specialization for its adjoint version and its controlled
/// version. While the implementations for Adjoint SWAP and Controlled
/// SWAP are thus user-defined, the compiler still needs to generate
/// the implementation for the combination of both functors (Controlled
/// Adjoint SWAP, which is the same as Adjoint Controlled SWAP).
operation SWAP(q1 : Qubit, q2 : Qubit) : Unit is Adj + Ctl {

    body (...) {
        CNOT(q1, q2);
        CNOT(q2, q1);
        CNOT(q1, q2);
    }

    adjoint (...) {
        SWAP(q1, q2);
    }

    controlled (cs, ...) {
        CNOT(q1, q2);
        Controlled CNOT(cs, (q2, q1));
        CNOT(q1, q2);
    }
}

/// The main function cannot be Adj or Ctl.
operation Main() : Unit {

    // We invoke specializations using functors at the call site.
    // In order to call these operations and their specializations, we need
    // to allocate some qubits.
    use (q1, q2) = (Qubit(), Qubit());

    // We can call `SWAP` in the normal way, without any functors or
    // specializations.
    SWAP(q1, q2);

    // We use the functor `Adjoint` on the operation `SWAP` to invoke the
    // adjoint specialization of `SWAP`.
    Adjoint SWAP(q1, q2);

    // Now,we will use the `Controlled` functor. To use this functor, we
    // need some control qubits.

    // The `Controlled` functor invokes the control specialization of the
    // operation. Note that the control specialization has a different
    // signature than the regular operation: first, the control qubits;
    // second, the parameters for the regular operation.
    // For a singly controlled invocation, only pass one control qubit
    // in the first array.
    use ctl = Qubit();
    Controlled SWAP([ctl], (q1, q2));

    // For a multi controlled invocation, pass an array of multiple qubits
    // in as the first argument.
    use ctls = Qubit[3];
    Controlled SWAP(ctls, (q1, q2));
}
