open QIR.Intrinsic;

/// # Summary
/// Dumps the current target machine's status.
///
/// # Description
/// This method allows you to dump information about the current quantum state.
/// The actual information generated and the semantics are specific to each target machine.
///
/// For the local sparse-state simulator distributed as part of the
/// Quantum Development Kit, this method will write the wave function as a
/// one-dimensional array of pairs of state indices and complex numbers, in which each element represents
/// the amplitudes of the probability of measuring the corresponding state.
///
/// # Example
/// When run on the sparse-state simulator, the following snippet dumps
/// the Bell state (|00⟩ + |11⟩ ) / √2 to the console:
/// ```qsharp
/// use left = Qubit();
/// use right = Qubit();
/// within {
///     H(left);
///     CNOT(left, right);
/// } apply {
///     DumpMachine();
/// }
/// ```
function DumpMachine() : Unit {
    body intrinsic;
}

/// # Summary
/// Dumps the current target machine's status associated with the given qubits.
///
/// # Input
/// ## qubits
/// The list of qubits to report.
///
/// # Remarks
/// This method allows you to dump the information associated with the state of the
/// given qubits.
///
/// For the local sparse-state simulator distributed as part of the
/// Quantum Development Kit, this method will write the
/// state of the given qubits (i.e. the wave function of the corresponding subsystem) as a
/// one-dimensional array of pairs of state indices and complex numbers, in which each element represents
/// the amplitudes of the probability of measuring the corresponding state.
/// If the given qubits are entangled with some other qubit and their
/// state can't be separated, it fails with a runtime error indicating that the qubits are entangled.
///
/// # Example
/// When run on the sparse-state simulator, the following snippet dumps
/// the Bell state (|00⟩ + |11⟩ ) / √2 to the console:
/// ```qsharp
/// use left = Qubit();
/// use right = Qubit();
/// within {
///     H(left);
///     CNOT(left, right);
/// } apply {
///     DumpRegister([left, right]);
/// }
/// ```
function DumpRegister(register : Qubit[]) : Unit {
    body intrinsic;
}

/// # Summary
/// Given an operation, dumps the matrix representation of the operation action on the given
/// number of qubits.
///
/// # Input
/// ## nQubits
/// The number of qubits on which the given operation acts.
/// ## op
/// The operation that is to be diagnosed.
///
/// # Remarks
/// When run on the sparse-state simulator, the following snippet
/// will output the matrix
/// $\left(\begin{matrix} 0.707 & 0.707 \\\\ 0.707 & -0.707\end{matrix}\right)$:
///
/// ```qsharp
/// operation DumpH() : Unit {
///     DumpOperation(1, qs => H(qs[0]));
/// }
/// ```
/// Calling this operation has no observable effect from within Q#.
/// Note that if `DumpOperation` is called when there are other qubits allocated,
/// the matrix displayed may reflect any global phase that has accumulated from operations
/// on those other qubits.
@SimulatableIntrinsic()
operation DumpOperation(nQubits : Int, op : Qubit[] => Unit) : Unit {
    use (targets, extra) = (Qubit[nQubits], Qubit[nQubits]);
    for i in 0..nQubits - 1 {
        H(targets[i]);
        CNOT(targets[i], extra[i]);
    }
    op(targets);
    DumpMatrix(targets + extra);
    ResetAll(targets + extra);
}

function DumpMatrix(qs : Qubit[]) : Unit {
    body intrinsic;
}

/// # Summary
/// Checks whether a qubit is in the |0⟩ state, returning true if it is.
///
/// # Description
/// This operation checks whether a qubit is in the |0⟩ state. It will return true only
/// if the qubit is deterministically in the |0⟩ state, and will return false otherwise. This operation
/// does not change the state of the qubit.
///
/// # Input
/// ## qubit
/// The qubit to check.
/// # Output
/// True if the qubit is in the |0⟩ state, false otherwise.
///
/// # Remarks
/// This operation is useful for checking whether a qubit is in the |0⟩ state during simulation. It is not possible to check
/// this on hardware without measuring the qubit, which could change the state.
operation CheckZero(qubit : Qubit) : Bool {
    body intrinsic;
}

/// # Summary
/// Checks whether all qubits in the provided array are in the |0⟩ state. Returns true if they are.
///
/// # Description
/// This operation checks whether all qubits in the provided array are in the |0⟩ state. It will return true only
/// if all qubits are deterministically in the |0⟩ state, and will return false otherwise. This operation
/// does not change the state of the qubits.
///
/// # Input
/// ## qubits
/// The qubits to check.
/// # Output
/// True if all qubits are in the |0⟩ state, false otherwise.
///
/// # Remarks
/// This operation is useful for checking whether a qubit is in the |0⟩ state during simulation. It is not possible to check
/// this on hardware without measuring the qubit, which could change the state.
operation CheckAllZero(qubits : Qubit[]) : Bool {
    for q in qubits {
        if not CheckZero(q) {
            return false;
        }
    }

    return true;
}

/// # Summary
/// Checks whether a given condition is true, failing with a message if it is not.
///
/// # Description
/// This function checks whether a given condition is true. If the condition is false, the operation fails with the given message,
/// terminating the program.
///
/// # Input
/// ## actual
/// The condition to check.
/// ## message
/// The message to use in the failure if the condition is false.
function Fact(actual : Bool, message : String) : Unit {
    if (not actual) {
        fail message;
    }
}

/// # Summary
/// Given two operations, checks that they act identically for all input states.
///
/// # Description
/// This check is implemented by using the Choi–Jamiołkowski isomorphism to reduce
/// this check to a check on two entangled registers.
/// Thus, this operation needs only a single call to each operation being tested,
/// but requires twice as many qubits to be allocated.
/// This check can be used to ensure, for instance, that an optimized version of an
/// operation acts identically to its naïve implementation, or that an operation
/// which acts on a range of non-quantum inputs agrees with known cases.
///
/// # Remarks
/// This operation requires that the operation modeling the expected behavior is
/// adjointable, so that the inverse can be performed on the target register alone.
/// Formally, one can specify a transpose operation, which relaxes this requirement,
/// but the transpose operation is not in general physically realizable for arbitrary
/// quantum operations and thus is not included here as an option.
///
/// # Input
/// ## nQubits
/// Number of qubits to pass to each operation.
/// ## actual
/// Operation to be tested.
/// ## expected
/// Operation defining the expected behavior for the operation under test.
/// # Output
/// True if operations are equal, false otherwise.
operation CheckOperationsAreEqual(
    nQubits : Int,
    actual : (Qubit[] => Unit),
    expected : (Qubit[] => Unit is Adj)
) : Bool {

    // Prepare a reference register entangled with the target register.
    use reference = Qubit[nQubits];
    use target = Qubit[nQubits];

    // Apply operations.
    within {
        for i in 0..nQubits - 1 {
            H(reference[i]);
            CNOT(reference[i], target[i]);
        }
    } apply {
        actual(target);
        Adjoint expected(target);
    }

    // Check and return result.
    let areEqual = CheckAllZero(reference) and CheckAllZero(target);
    ResetAll(target);
    ResetAll(reference);
    areEqual
}

/// # Summary
/// Starts counting the number of times the given operation is called. Fails if the operation is already being counted.
///
/// # Description
/// This operation allows you to count the number of times a given operation is called. If the given operation is already
/// being counted, calling `StartCountingOperation` again will trigger a runtime failure. Counting is based on the specific
/// specialization of the operation invoked, so `X` and `Adjoint X` are counted separately.
/// Likewise `Controlled X`, `CNOT`, and `CX` are independent operations that are counted separately, as are `Controlled X`
/// and `Controlled Adjoint X`.
///
/// # Input
/// ## callable
/// The operation to be counted.
///
/// # Remarks
/// Counting operation calls requires specific care in what operation is passed as input. For example, `StartCountingOperation(H)` will
/// count only the number of times `H` is called, while `StartCountingOperation(Adjoint H)` will count only the number of times `Adjoint H` is called, even
/// though `H` is self-adjoint. This is due to how the execution treats the invocation of these operations as distinct by their specialization.
/// In the same way, `StartCountingOperation(Controlled X)` will count only the number of times `Controlled X` is called, while
/// `StartCountingOperation(CNOT)` will count only the number of times `CNOT` is called.
///
/// When counting lambdas, the symbol the lambda is bound to is used to identify the operation and it is counted as a separate operation. For example,
/// ```qsharp
/// let myOp = q => H(q);
/// StartCountingOperation(myOp);
/// ```
/// Will count specifically calls to `myOp` and not `H`. By contrast, the following code will count calls to `H` itself:
/// ```qsharp
/// let myOp = H;
/// StartCountingOperation(myOp);
/// ```
/// This is because this code does not define a lambda and instead just creates a binding to `H` directly.
@Config(Unrestricted)
operation StartCountingOperation<'In, 'Out>(callable : 'In => 'Out) : Unit {
    body intrinsic;
}

/// # Summary
/// Stops counting the number of times the given operation is called and returns the count. Fails
/// if the operation was not being counted.
///
/// # Description
/// This operation allows you to stop counting the number of times a given operation is called and returns the count.
/// If the operation was not being counted, it triggers a runtime failure.
///
/// # Input
/// ## callable
/// The operation whose count will be returned.
/// # Output
/// The number of times the operation was called since the last call to `StartCountingOperation`.
@Config(Unrestricted)
operation StopCountingOperation<'In, 'Out>(callable : 'In => 'Out) : Int {
    body intrinsic;
}

/// # Summary
/// Starts counting the number of times the given function is called. Fails if the function is already being counted.
///
/// # Description
/// This operation allows you to count the number of times a given function is called. If the given function is already
/// being counted, calling `StartCountingFunction` again will trigger a runtime failure.
///
/// # Input
/// ## callable
/// The function to be counted.
///
/// # Remarks
/// When counting lambdas, the symbol the lambda is bound to is used to identify the function and it is counted as a separate function. For example,
/// ```qsharp
/// let myFunc = i -> AbsI(i);
/// StartCountingFunction(myFunc);
/// ```
/// Will count specifically calls to `myFunc` and not `AbsI`. By contrast, the following code will count calls to `AbsI` itself:
/// ```qsharp
/// let myFunc = AbsI;
/// StartCountingFunction(myFunc);
/// ```
/// This is because this code does not define a lambda and instead just creates a binding to `AbsI` directly.
@Config(Unrestricted)
operation StartCountingFunction<'In, 'Out>(callable : 'In -> 'Out) : Unit {
    body intrinsic;
}

/// # Summary
/// Stops counting the number of times the given function is called and returns the count. Fails
/// if the function was not being counted.
///
/// # Description
/// This operation allows you to stop counting the number of times a given function is called and returns the count.
/// If the function was not being counted, it triggers a runtime failure.
///
/// # Input
/// ## callable
/// The function whose count will be returned.
/// # Output
/// The number of times the function was called since the last call to `StartCountingFunction`.
@Config(Unrestricted)
operation StopCountingFunction<'In, 'Out>(callable : 'In -> 'Out) : Int {
    body intrinsic;
}

/// # Summary
/// Starts counting the number of qubits allocated. Fails if qubits are already being counted.
///
/// # Description
/// This operation allows you to count the number of qubits allocated until `StopCountingQubits` is called.
/// The counter is incremented only when a new unique qubit is allocated, so reusing the same qubit multiple times
/// across separate allocations does not increment the counter.
///
/// # Remarks
/// This operation is useful for tracking the number of unique qubits allocated in a given scope. Along with
/// `StopCountingQubits`, it can be used to verify that a given operation does not allocate more qubits than
/// expected. For example,
/// ```qsharp
/// StartCountingQubits();
/// testOperation();
/// let qubitsAllocated = StopCountingQubits();
/// Fact(qubitsAllocated <= 4, "Operation should not allocate more than 4 qubits.");
/// ```
@Config(Unrestricted)
operation StartCountingQubits() : Unit {
    body intrinsic;
}

/// # Summary
/// Stops counting the number of qubits allocated and returns the count. Fails if the qubits were not being counted.
///
/// # Description
/// This operation allows you to stop counting the number of qubits allocated and returns the count since the
/// last call to `StartCountingQubits`. If the qubits were not being counted, it triggers a runtime failure.
///
/// # Output
/// The number of unique qubits allocated since the last call to `StartCountingQubits`.
@Config(Unrestricted)
operation StopCountingQubits() : Int {
    body intrinsic;
}

/// # Summary
/// Configures Pauli noise for simulation.
///
/// # Description
/// This function configures Pauli noise for simulation. Parameters represent
/// probabilities of applying X, Y, and Z gates and must add up to at most 1.0.
/// Noise is applied after each gate and before each measurement in the simulator
/// backend. Decompositions may affect the number of times noise is applied.
/// Use 0.0 for all parameters to simulate without noise.
///
/// # Input
/// ## px
/// Probability of applying X gate.
/// ## py
/// Probability of applying Y gate.
/// ## pz
/// Probability of applying Z gate.
function ConfigurePauliNoise(px : Double, py : Double, pz : Double) : Unit {
    body intrinsic;
}

/// # Summary
/// Configures qubit loss during simulation.
///
/// # Description
/// This function configures qubit loss for simulation. The parameter `p` represents
/// the probability of a qubit loss during simulation. If `p` is greater than 0.0, the simulator will mark qubits
/// as lost with the given probability during each operation that acts on them.
/// Qubits that are lost are reset to the |0⟩ state but are not released. Loss is reported when the qubit is measured,
/// and then the qubit is considered "reloaded" and can be used again.
///
/// # Input
/// ## p
/// The probability of a qubit being lost during simulation. Must be between 0.0 and 1.0.
///
/// # Remarks
/// This operation is useful for simulating qubit loss for those modalities where qubit loss is a factor.
/// Note that the value returned from a measurement of a lost qubit is neither `Zero` nor `One`, but rather a special
/// value indicating that the qubit was lost. This value cannot be used in comparisons and will cause a runtime
/// failure if compared to another value.
/// To perform a measurement that includes a check for qubit loss, use the `MResetZChecked` operation.
function ConfigureQubitLoss(p : Double) : Unit {
    body intrinsic;
}

/// # Summary
/// Applies configured noise or loss to a qubit.
///
/// # Description
/// This operation applies configured noise and/or loss to a qubit during simulation. For example,
/// if configured noise is a bit-flip noise with 5% probability, the X gate will be applied
/// with 5% probability. If no noise is configured, no noise is applied.
/// This is useful to simulate noise during idle periods. It could also be used to
/// apply noise immediately after qubit allocation.
///
/// # Input
/// ## qubit
/// The qubit to which noise is applied.
///
/// # See Also
/// - [Std.Diagnostics.ConfigurePauliNoise](xref:Qdk.Std.Diagnostics.ConfigurePauliNoise)
/// - [Std.Diagnostics.ConfigureQubitLoss](xref:Qdk.Std.Diagnostics.ConfigureQubitLoss)
operation ApplyIdleNoise(qubit : Qubit) : Unit {
    body intrinsic;
}

/// # Summary
///  The bit flip noise with probability `p`.
function BitFlipNoise(p : Double) : (Double, Double, Double) {
    (p, 0.0, 0.0)
}

/// # Summary
///  The phase flip noise with probability `p`.
function PhaseFlipNoise(p : Double) : (Double, Double, Double) {
    (0.0, 0.0, p)
}

/// # Summary
///  The depolarizing noise with probability `p`.
function DepolarizingNoise(p : Double) : (Double, Double, Double) {
    (p / 3.0, p / 3.0, p / 3.0)
}

/// # Summary
///  No noise for noiseless operation.
function NoNoise() : (Double, Double, Double) {
    (0.0, 0.0, 0.0)
}

export
    DumpMachine,
    DumpRegister,
    DumpOperation,
    CheckZero,
    CheckAllZero,
    Fact,
    CheckOperationsAreEqual,
    StartCountingOperation,
    StopCountingOperation,
    StartCountingFunction,
    StopCountingFunction,
    StartCountingQubits,
    StopCountingQubits,
    ConfigurePauliNoise,
    ConfigureQubitLoss,
    ApplyIdleNoise,
    BitFlipNoise,
    PhaseFlipNoise,
    DepolarizingNoise,
    NoNoise;
