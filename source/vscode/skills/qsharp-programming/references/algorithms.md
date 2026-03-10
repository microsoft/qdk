# Q# Algorithm Patterns Reference

## Deutsch-Jozsa Algorithm

Determines if a Boolean function is constant or balanced in a single query.

```qsharp
operation DeutschJozsa(Uf : (Qubit[], Qubit) => Unit, n : Int) : Bool {
    use queryRegister = Qubit[n];
    use target = Qubit();
    X(target);
    H(target);
    within {
        ApplyToEach(H, queryRegister);
    } apply {
        Uf(queryRegister, target);
    }
    // If all Zero → constant; any One → balanced
    let results = MResetEachZ(queryRegister);
    Reset(target);
    return All(r -> r == Zero, results);  // true = constant
}
```

## Bernstein-Vazirani Algorithm

Finds a hidden bit string `s` from an oracle that computes `f(x) = s·x mod 2`.

```qsharp
operation BernsteinVazirani(Uf : (Qubit[], Qubit) => Unit, n : Int) : Result[] {
    use queryRegister = Qubit[n];
    use target = Qubit();
    X(target);
    within {
        ApplyToEach(H, queryRegister);
        H(target);
    } apply {
        Uf(queryRegister, target);
    }
    let results = MResetEachZ(queryRegister);
    Reset(target);
    return results;
}
```

## Quantum Fourier Transform (QFT)

```qsharp
operation QFT(qs : Qubit[]) : Unit is Adj + Ctl {
    let n = Length(qs);
    for i in 0..n-1 {
        H(qs[i]);
        for j in i+1..n-1 {
            Controlled R1Frac([qs[j]], (1, j - i, qs[i]));
        }
    }
    // Reverse qubit order
    for i in 0..n/2-1 {
        SWAP(qs[i], qs[n - 1 - i]);
    }
}
```

## Quantum Phase Estimation

```qsharp
operation PhaseEstimation(
    oracle : Qubit => Unit is Adj + Ctl,
    eigenstate : Qubit,
    precision : Int
) : Double {
    use phase = Qubit[precision];
    ApplyToEach(H, phase);

    // Apply controlled U^(2^k)
    for k in 0..precision-1 {
        for _ in 1..2^k {
            Controlled oracle([phase[k]], eigenstate);
        }
    }

    Adjoint QFT(phase);
    let result = MResetEachZ(phase);
    // Convert binary fraction to Double
    mutable estimate = 0.0;
    for i in 0..precision-1 {
        if result[i] == One {
            estimate += 1.0 / IntAsDouble(2^(i + 1));
        }
    }
    return estimate;
}
```

## Grover's Search (Full Implementation)

```qsharp
operation GroverSearch(
    nQubits : Int,
    oracle : Qubit[] => Unit is Adj
) : Result[] {
    let iterations = Round(PI() / 4.0 * Sqrt(IntAsDouble(2^nQubits)));
    use qs = Qubit[nQubits];

    // Initialize uniform superposition
    ApplyToEach(H, qs);

    for _ in 1..iterations {
        // Phase oracle: mark target states with -1 phase
        oracle(qs);

        // Diffusion operator (reflect about uniform)
        within {
            ApplyToEachA(H, qs);
            ApplyToEachA(X, qs);
        } apply {
            Controlled Z(Most(qs), Tail(qs));
        }
    }

    return MResetEachZ(qs);
}
```

### Reflect About Uniform (Diffusion Operator)

```qsharp
operation ReflectAboutUniform(qs : Qubit[]) : Unit is Adj + Ctl {
    within {
        ApplyToEachA(H, qs);
        ApplyToEachA(X, qs);
    } apply {
        Controlled Z(Most(qs), Tail(qs));
    }
}
```

## Shor's Algorithm (Period Finding)

```qsharp
operation FactorSemiprimeInteger(number : Int) : (Int, Int) {
    if number % 2 == 0 {
        return (2, number / 2);
    }

    mutable foundFactors = false;
    mutable factors = (1, 1);

    repeat {
        let generator = DrawRandomInt(1, number - 1);
        let gcd = GreatestCommonDivisorI(generator, number);

        if gcd != 1 {
            // Lucky: found a factor directly
            set foundFactors = true;
            set factors = (gcd, number / gcd);
        } else {
            // Find the period of generator^x mod number
            let period = EstimatePeriod(generator, number);
            set (foundFactors, factors) =
                MaybeFactorsFromPeriod(number, generator, period);
        }
    } until foundFactors;

    return factors;
}
```

## Quantum Error Correction — Bit-Flip Code

```qsharp
// Encode: |ψ⟩ → |ψψψ⟩
operation Encode(register : Qubit[]) : Unit is Adj {
    CNOT(register[0], register[1]);
    CNOT(register[0], register[2]);
}

// Detect and correct single bit-flip errors
operation DetectAndCorrect(register : Qubit[]) : Unit {
    // Syndrome measurement
    use (s1, s2) = (Qubit(), Qubit());
    CNOT(register[0], s1); CNOT(register[1], s1);
    CNOT(register[1], s2); CNOT(register[2], s2);
    let (parity01, parity12) = (M(s1), M(s2));
    Reset(s1); Reset(s2);

    // Correction
    if parity01 == One and parity12 == Zero { X(register[0]); }
    if parity01 == One and parity12 == One  { X(register[1]); }
    if parity01 == Zero and parity12 == One { X(register[2]); }
}
```

## Hamiltonian Simulation (Trotter-Suzuki)

```qsharp
operation TrotterStep(
    qs : Qubit[],
    dt : Double,
    couplings : Double[]
) : Unit is Adj + Ctl {
    let n = Length(qs);

    // Single-qubit terms (transverse field)
    for i in 0..n-1 {
        Rx(2.0 * couplings[0] * dt, qs[i]);
    }

    // Two-qubit ZZ interactions
    for i in 0..n-2 {
        Rzz(2.0 * couplings[1] * dt, qs[i], qs[i+1]);
    }
}

operation Simulate(
    qs : Qubit[],
    totalTime : Double,
    steps : Int,
    couplings : Double[]
) : Unit is Adj + Ctl {
    let dt = totalTime / IntAsDouble(steps);
    for _ in 1..steps {
        TrotterStep(qs, dt, couplings);
    }
}
```

## Variational Quantum Eigensolver (VQE)

```qsharp
// Parameterized ansatz
operation Ansatz(thetas : Double[], qs : Qubit[]) : Unit is Adj {
    let n = Length(qs);
    // Single-qubit rotations
    for i in 0..n-1 {
        Ry(thetas[i], qs[i]);
    }
    // Entangling layer
    for i in 0..n-2 {
        CNOT(qs[i], qs[i+1]);
    }
}

// Measure expectation value in a given Pauli basis
operation MeasureExpectation(
    thetas : Double[],
    pauliBasis : Pauli[],
    nQubits : Int,
    shots : Int
) : Double {
    mutable total = 0.0;
    for _ in 1..shots {
        use qs = Qubit[nQubits];
        Ansatz(thetas, qs);
        let result = Measure(pauliBasis, qs);
        set total += result == Zero ? 1.0 | -1.0;
        ResetAll(qs);
    }
    return total / IntAsDouble(shots);
}
```

## Superdense Coding

```qsharp
operation SuperdenseCoding(bit1 : Bool, bit2 : Bool) : (Bool, Bool) {
    use (alice, bob) = (Qubit(), Qubit());

    // Create entangled pair
    H(alice);
    CNOT(alice, bob);

    // Alice encodes 2 classical bits
    if bit1 { Z(alice); }
    if bit2 { X(alice); }

    // Bob decodes
    CNOT(alice, bob);
    H(alice);
    let (r1, r2) = (MResetZ(alice), MResetZ(bob));
    return (r1 == One, r2 == One);
}
```

## CHSH Game (Quantum Strategy)

```qsharp
operation CHSHQuantumStrategy(refereeAliceBit : Bool) : Bool {
    use (alice, bob) = (Qubit(), Qubit());

    // Create entangled pair
    H(alice);
    CNOT(alice, bob);

    // Alice's measurement basis depends on her input
    if refereeAliceBit {
        Ry(PI() / 4.0, alice);  // Rotate by π/8
    }

    return MResetZ(alice) == One;
}
```
