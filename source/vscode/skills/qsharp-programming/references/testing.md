# Q# Testing Patterns Reference

## Basic Test Structure

```qsharp
import Std.Diagnostics.*;

// Function test (pure, no qubits)
@Test()
function TestArithmetic() : Unit {
    Fact(2 + 2 == 4, "Basic arithmetic failed");
    Fact(AbsI(-5) == 5, "AbsI failed");
}

// Operation test (quantum)
@Test()
operation TestMeasurement() : Unit {
    use q = Qubit();
    // |0⟩ always measures Zero
    let result = MResetZ(q);
    Fact(result == Zero, $"Expected Zero, got {result}");
}
```

## Preferred Error Reporting

Use conditional `fail` instead of `Fact` for better error location in VS Code:

```qsharp
@Test()
operation TestBellState() : Unit {
    use (q1, q2) = (Qubit(), Qubit());
    H(q1);
    CNOT(q1, q2);
    let (r1, r2) = (MResetZ(q1), MResetZ(q2));
    if r1 != r2 {
        fail $"Bell pair correlation broken: ({r1}, {r2})";
    }
}
```

## State Verification with DumpMachine

```qsharp
@Test()
operation TestSuperposition() : Unit {
    use q = Qubit();
    H(q);
    // DumpMachine() prints current state to console for inspection
    // Useful during development — not a pass/fail assertion
    DumpMachine();
    Reset(q);
}

@Test()
operation TestSpecificQubits() : Unit {
    use qs = Qubit[3];
    H(qs[0]);
    CNOT(qs[0], qs[1]);
    // Dump only specific qubits
    DumpRegister([qs[0], qs[1]]);
    ResetAll(qs);
}
```

## Operation Equivalence Testing

Verify two operations produce identical quantum states for all inputs:

```qsharp
@Test()
operation TestSwapEquivalence() : Unit {
    let actual = qs => {
        CNOT(qs[0], qs[1]);
        CNOT(qs[1], qs[0]);
        CNOT(qs[0], qs[1]);
    };
    let expected = qs => SWAP(qs[0], qs[1]);

    // CheckOperationsAreEqual tests all 2^n input basis states
    let areEqual = CheckOperationsAreEqual(2, actual, expected);
    if not areEqual {
        fail "Three CNOTs should equal SWAP";
    }
}
```

## Statistical Testing (Probabilistic Outcomes)

For operations with probabilistic results, run multiple shots:

```qsharp
@Test()
operation TestFairCoin() : Unit {
    let shots = 1000;
    mutable oneCount = 0;
    for _ in 1..shots {
        use q = Qubit();
        H(q);
        if MResetZ(q) == One {
            set oneCount += 1;
        }
    }
    // Expect roughly 50% — allow wide tolerance
    let ratio = IntAsDouble(oneCount) / IntAsDouble(shots);
    if ratio < 0.35 or ratio > 0.65 {
        fail $"H gate seems biased: {oneCount}/{shots} = {ratio}";
    }
}
```

## Testing Deterministic Quantum Operations

```qsharp
@Test()
operation TestXGateFlip() : Unit {
    use q = Qubit();
    // X on |0⟩ should always give |1⟩
    X(q);
    let result = MResetZ(q);
    Fact(result == One, "X gate should flip |0⟩ to |1⟩");
}

@Test()
operation TestHZHEqualsX() : Unit {
    // HZH = X (up to global phase)
    use q = Qubit();
    H(q); Z(q); H(q);
    let result = MResetZ(q);
    Fact(result == One, "HZH should act as X on |0⟩");
}
```

## Testing with CheckZero

```qsharp
@Test()
operation TestResetWorks() : Unit {
    use q = Qubit();
    X(q);
    Reset(q);
    // CheckZero verifies qubit is in |0⟩ state
    Fact(CheckZero(q), "Qubit should be |0⟩ after Reset");
}
```

## Testing Adjoint Correctness

```qsharp
@Test()
operation TestAdjointUndoes() : Unit {
    use q = Qubit();
    // Apply then adjoint should return to |0⟩
    MyOperation(q);
    Adjoint MyOperation(q);
    Fact(CheckZero(q), "Adjoint should undo the operation");
}
```

## Testing Controlled Operations

```qsharp
@Test()
operation TestControlledDoesNothing() : Unit {
    // Controlled op with control in |0⟩ should not affect target
    use (ctrl, target) = (Qubit(), Qubit());
    // ctrl is |0⟩, so Controlled X should not fire
    Controlled X([ctrl], target);
    Fact(CheckZero(target), "Controlled X with |0⟩ control should not flip target");
    Reset(ctrl);
}

@Test()
operation TestControlledActivates() : Unit {
    use (ctrl, target) = (Qubit(), Qubit());
    X(ctrl);  // Set control to |1⟩
    Controlled X([ctrl], target);
    let result = MResetZ(target);
    Fact(result == One, "Controlled X with |1⟩ control should flip target");
    Reset(ctrl);
}
```

## Organizing Tests in Multi-File Projects

```
project_root/
├── qsharp.json
└── src/
    ├── Main.qs         # Entry point
    ├── Algorithms.qs   # Implementation
    └── Tests.qs        # Test operations
```

```qsharp
// Tests.qs
import Algorithms.MyAlgorithm;

@Test()
operation TestMyAlgorithm() : Unit {
    let result = MyAlgorithm(5);
    if result != 42 {
        fail $"Expected 42, got {result}";
    }
}
```

## Running Tests

- **VS Code**: Tests with `@Test()` appear in the Test Explorer. Click to run.
- **Command line**: `qsharp test` runs all `@Test()` operations/functions.
- **Python**: `qsharp.run("Tests.TestMyAlgorithm()", shots=1)` to run from Python.
