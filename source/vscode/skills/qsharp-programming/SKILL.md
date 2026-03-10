---
name: qsharp-programming
description: 'Write Q# quantum programs. Use when: user asks to "write Q# code", "implement a quantum algorithm", "create a Q# operation", "write quantum gates", debug Q# syntax errors, needs Q# standard library guidance, asks about Q# types/operators/control flow, or wants quantum algorithm patterns like Grover, QFT, teleportation, error correction. Covers Q# syntax, type system, quantum operations, standard library, algorithms, testing, and project structure.'
---

# Q# Programming

Write correct, idiomatic Q# code for quantum computing. This skill covers the full Q# language — syntax, type system, quantum operations, standard library, algorithm patterns, testing, and project structure.

## When to Use

- User asks to write, debug, or explain Q# code
- User wants to implement a quantum algorithm
- User needs Q# syntax help (operators, control flow, types)
- User asks about Q# standard library functions
- User wants to write Q# tests
- User needs help structuring a Q# project

## Critical: Library Lookups

**Always call the `qsharpGetLibraryDescriptions` tool** before generating Q# code that uses standard library functions. This returns the authoritative, up-to-date list of all Q# library items with signatures. Do not guess at function names or signatures.

## Q# Language Quick Reference

### File Structure

Every `.qs` file is a namespace (the filename minus extension). No `namespace` blocks needed.

```qsharp
// Main.qs — this file's namespace is "Main"
import Std.Diagnostics.*;
import Std.Math.*;
import MyModule.MyOperation;       // import from another project file

operation Main() : Unit {
    Message("Hello quantum world!");
}
```

### Entry Points

```qsharp
// Convention: operation named Main() is the entry point
operation Main() : Result[] {
    use qs = Qubit[3];
    // ...
    return MeasureEachZ(qs);
}

// Alternative: explicit attribute (any name)
@EntryPoint()
operation RunExperiment() : Unit { }
```

### Variables

```qsharp
let x = 42;                        // Immutable (default)
let x : Int = 42;                  // Explicit type annotation
let x = 43;                        // Shadowing allowed

mutable counter = 0;               // Mutable
counter += 1;                      // Mutation
```

### Primitive Types

| Type | Examples | Notes |
|------|----------|-------|
| `Int` | `42`, `0xFF`, `0b1010`, `0o52` | 64-bit signed |
| `BigInt` | `42L`, `0xFFL` | Arbitrary precision, `L` suffix |
| `Double` | `3.14`, `1.0e-3` | 64-bit floating point |
| `Bool` | `true`, `false` | |
| `String` | `"hello"`, `$"x={x}"` | String interpolation with `$` |
| `Result` | `Zero`, `One` | Measurement outcome |
| `Pauli` | `PauliI`, `PauliX`, `PauliY`, `PauliZ` | Pauli basis |
| `Range` | `1..10`, `0..2..8`, `10..-1..0` | start..step..end |
| `Unit` | `()` | No value |

### Aggregate Types

```qsharp
// Arrays
let arr = [1, 2, 3];               // Int[]
let zeros = [0, size = 5];         // [0, 0, 0, 0, 0]
let slice = arr[1..2];             // [2, 3]
let tail = arr[2...];              // Open-ended range

// Tuples
let pair = (42, "answer");         // (Int, String)
let (x, name) = pair;              // Destructuring
let (x, _) = pair;                 // Discard with _

// User-defined structs
struct Complex2D { Re : Double, Im : Double }
let c = new Complex2D { Re = 1.0, Im = 0.0 };
let c2 = new Complex2D { ...c, Im = 2.0 };  // Copy-update
let re = c.Re;                     // Field access
```

### Operators

```qsharp
// Arithmetic: + - * / % ^       Comparison: == != < <= > >=
// Logical: and or not           Bitwise: ~~~ &&& ||| ^^^ <<< >>>
// String interpolation: $"x={x}"  Array update: arr w/ idx <- val
// Ternary: cond ? trueVal | falseVal
```

### Control Flow

```qsharp
// If-elif-else (expression-based — returns a value)
let abs = if x > 0 { x } else { -x };

// For loops
for i in 0..n-1 { }               // Range
for i in 0..2..10 { }             // Step by 2
for elem in array { }             // Array iteration

// While loops
while condition { }

// Repeat-until (quantum retry pattern)
repeat {
    H(q);
} until M(q) == Zero
fixup {
    Reset(q);
}

// Within-apply (automatic uncomputation)
within {
    H(q);                          // Preparation
} apply {
    Z(q);                          // Main operation
}                                  // H auto-applied again (adjoint)
```

### Operations vs Functions

```qsharp
// Operations: can use qubits and quantum gates
operation ApplyHadamard(q : Qubit) : Unit is Adj + Ctl {
    H(q);
}

// Functions: pure computation, NO quantum side effects
function Square(x : Int) : Int { x * x }

// Lambdas
let addOne = x -> x + 1;           // Function lambda (->)
let flip = q => X(q);              // Operation lambda (=>)

// Partial application
let cnot0 = CNOT(control, _);      // _ is placeholder
```

### Functor Support

```qsharp
// Declare with `is` keyword
operation MyGate(q : Qubit) : Unit is Adj + Ctl { H(q); }

// Use functors
Adjoint MyGate(q);                  // Inverse
Controlled MyGate([ctrl], q);       // Controlled version
Controlled Adjoint MyGate([ctrl], q);

// Auto-generated: compiler derives Adjoint/Controlled
// Manual specializations:
operation Custom(q : Qubit) : Unit is Adj {
    body (...) { H(q); }
    adjoint (...) { H(q); }        // H is self-adjoint
}
```

### Generics

```qsharp
function Identity<'T>(x : 'T) : 'T { x }

// With constraints
function AllEqual<'T : Eq>(items : 'T[]) : Bool {
    All(x -> x == items[0], items)
}
// Available constraints: Eq, Add, Sub, Mul, Div, Mod, Signed, Ord, Show, Integral, Exp['T]
```

## Quantum Operations

### Qubit Management

```qsharp
use q = Qubit();                    // Single qubit (starts in |0⟩)
use qs = Qubit[n];                  // Array of n qubits
use (a, b) = (Qubit(), Qubit());    // Multiple named qubits

Reset(q);                           // Reset to |0⟩
ResetAll(qs);                       // Reset array
```

### Common Gates

```qsharp
// Single-qubit
H(q); X(q); Y(q); Z(q); S(q); T(q);          // Pauli, Hadamard, Phase
Rx(theta, q); Ry(theta, q); Rz(theta, q);     // Rotations
// Two-qubit
CNOT(control, target); SWAP(q1, q2); Rzz(theta, q1, q2);
CCNOT(c1, c2, target);                         // Toffoli
ApplyToEach(H, qubits);                        // H on each qubit
```

### Measurement

```qsharp
let r = M(q);                      // Z-basis, qubit stays measured
let r = MResetZ(q);                // Measure + reset to |0⟩
let rs = MeasureEachZ(qs);         // Measure each, qubits stay
let rs = MResetEachZ(qs);          // Measure each + reset

// Joint measurement (parity)
let parity = Measure([PauliZ, PauliZ], [q1, q2]);

// Conditional on result
if M(q) == One { X(target); }
```

### Controlled Operations

```qsharp
Controlled X([ctrl], target);                  // CNOT equivalent
Controlled H([c1, c2], target);                // Multi-controlled
ApplyControlledOnBitString([true, false], X, [c1, c2], target);
ApplyControlledOnInt(3, X, [c1, c2], target);
```

## Standard Library (Std)

**Always call `qsharpGetLibraryDescriptions`** for authoritative signatures. Key namespaces:

| Namespace | Purpose | Key Items |
|-----------|---------|-----------|
| `Std.Arrays` | Array ops | `Length`, `Mapped`, `Fold`, `Zipped`, `Enumerated`, `Reversed`, `Sorted`, `Head`, `Tail`, `Rest`, `Most`, `Chunks`, `All`, `Any` |
| `Std.Math` | Math | `PI()`, `E()`, `Sin`, `Cos`, `Sqrt`, `Log`, `Exp`, `AbsD`, `AbsI`, `Min`, `Max`, `BitSizeI`, `GreatestCommonDivisorI` |
| `Std.Convert` | Type conversion | `IntAsDouble`, `IntAsBigInt`, `BoolArrayAsInt`, `IntAsBoolArray`, `ResultAsBool` |
| `Std.Diagnostics` | Debug/test | `Message`, `DumpMachine`, `DumpRegister`, `Fact`, `CheckZero`, `CheckOperationsAreEqual` |
| `Std.Measurement` | Measurement | `MResetZ`, `MResetEachZ`, `MeasureAllZ`, `MeasureEachZ` |
| `Std.Canon` | Patterns | `ApplyToEach`, `ApplyToEachA`, `ApplyToEachCA` |
| `Std.Random` | RNG | `DrawRandomInt`, `DrawRandomDouble`, `DrawRandomBool` |
| `Std.StatePreparation` | State prep | `PreparePureStateD`, `PrepareUniformSuperposition` |
| `Std.Arithmetic` | Quantum arith | `IncByI`, `IncByLE`, `AddLE`, `ReflectAboutInteger` |
| `Std.ResourceEstimation` | RE hints | `AccountForEstimates`, `BeginEstimateCaching`, `AuxQubitCount` |

## Algorithm Patterns

For detailed algorithm implementations, see [algorithms reference](./references/algorithms.md).

### Superposition + Measurement

```qsharp
use q = Qubit();
H(q);                              // |0⟩ → (|0⟩+|1⟩)/√2
let result = MResetZ(q);           // 50/50 Zero or One
```

### Bell Pair (Entanglement)

```qsharp
use (q1, q2) = (Qubit(), Qubit());
H(q1);
CNOT(q1, q2);                     // (|00⟩+|11⟩)/√2
```

### Quantum Teleportation

```qsharp
// Teleport state of `msg` to `target` using entangled `aux`
H(aux); CNOT(aux, target);         // Create Bell pair
CNOT(msg, aux); H(msg);            // Bell measurement
if M(aux) == One { X(target); }
if M(msg) == One { Z(target); }
```

### Grover's Search

```qsharp
operation GroverSearch(nQubits : Int, oracle : Qubit[] => Unit) : Result[] {
    use qs = Qubit[nQubits];
    let iterations = Round(PI() / 4.0 * Sqrt(IntAsDouble(2^nQubits)));
    ApplyToEach(H, qs);            // Uniform superposition
    for _ in 1..iterations {
        oracle(qs);                // Mark target states
        ReflectAboutUniform(qs);   // Amplitude amplification
    }
    return MResetEachZ(qs);
}
```

### Within-Apply for Reversible Computation

```qsharp
// Automatically uncomputes the `within` block after `apply`
within {
    ApplyToEach(H, register);
    ApplyToEach(X, register);
} apply {
    Controlled Z(Most(register), Tail(register));
}
```

## Testing

```qsharp
@Test()
operation TestBellPair() : Unit {
    use (q1, q2) = (Qubit(), Qubit());
    H(q1);
    CNOT(q1, q2);

    // Prefer conditional fail over Fact for better error location
    let (r1, r2) = (MResetZ(q1), MResetZ(q2));
    if r1 != r2 {
        fail $"Bell pair broken: got ({r1}, {r2})";
    }
}

@Test()
function TestMath() : Unit {
    Fact(AbsI(-5) == 5, "AbsI failed");
}

@Test()
operation TestOperationEquivalence() : Unit {
    // Verify two operations produce the same result on all inputs
    let actual = qs => { CNOT(qs[0], qs[1]); CNOT(qs[1], qs[0]); CNOT(qs[0], qs[1]); };
    let expected = qs => SWAP(qs[0], qs[1]);
    Fact(CheckOperationsAreEqual(2, actual, expected), "SWAP equivalence failed");
}
```

For advanced testing patterns including state assertions, see [testing reference](./references/testing.md).

## Project Structure

### Single File

A standalone `.qs` file works without any project file. Best for simple programs.

### Multi-File Project

```
project_root/
├── qsharp.json
└── src/
    ├── Main.qs
    ├── Helpers.qs
    └── Tests.qs
```

Minimal `qsharp.json`:
```json
{}
```

Cross-file imports:
```qsharp
// In Tests.qs — import from Helpers.qs
import Helpers.MyOperation;
import Helpers.*;                   // Import all
```

### Dependencies (GitHub Libraries)

```json
{
  "dependencies": {
    "Chemistry": {
      "github": {
        "ref": "v1.15.0",
        "owner": "microsoft",
        "repo": "qdk",
        "path": "library/chemistry"
      }
    }
  }
}
```

Available libraries: `chemistry`, `fixed_point`, `rotations`, `signed`, `table_lookup`, `qtest`.

### Target Profiles

Set in `qsharp.json` to constrain operations for specific hardware:

```json
{ "profile": "adaptive_ri" }
```

| Profile | Description |
|---------|------------|
| `unrestricted` | Full simulation (default) |
| `adaptive_ri` | Mid-circuit measurement + classical feed-forward |
| `adaptive_rif` | Adds floating-point computation |
| `base` | No mid-circuit measurement, no classical branching |

## Jupyter Notebook Integration

```python
import qsharp

# Define Q# code in a cell
%%qsharp
operation Bell() : (Result, Result) {
    use (q1, q2) = (Qubit(), Qubit());
    H(q1);
    CNOT(q1, q2);
    return (MResetZ(q1), MResetZ(q2));
}

# Call from Python
result = qsharp.eval("Bell()")
```

## Common Patterns

### Oracle Construction

```qsharp
// Marking oracle (flips target qubit for marked states)
operation MarkingOracle(qs : Qubit[], target : Qubit) : Unit is Adj + Ctl {
    Controlled X(qs, target);       // Marks |111...1⟩
}

// Phase oracle (adds -1 phase to marked states)
operation PhaseOracle(qs : Qubit[]) : Unit {
    use aux = Qubit();
    within { X(aux); H(aux); }      // Prepare |-⟩
    apply { MarkingOracle(qs, aux); } // Phase kickback
}
```

### State Preparation

```qsharp
// Prepare specific computational basis state
operation PrepareBitString(bits : Bool[], qs : Qubit[]) : Unit is Adj + Ctl {
    ApplyPauliFromBitString(PauliX, true, bits, qs);
}

// Arbitrary superposition from amplitudes
operation PrepareState(qs : Qubit[]) : Unit {
    PreparePureStateD([0.5, 0.5, 0.5, 0.5], qs);  // Uniform
}
```

### Quantum Arithmetic

```qsharp
// Increment a quantum register
use register = Qubit[4];
IncByI(1, register);               // |0000⟩ → |0001⟩

// Reflect about an integer
ReflectAboutInteger(target, register);
```

### Resource Estimation Annotations

```qsharp
// Cache repeated computations for resource estimator
if BeginEstimateCaching("DistillMagicStates", 0) {
    // ... expensive subroutine ...
    EndEstimateCaching();
}

// Declare external resource costs
AccountForEstimates(
    [TCount(100), RotationCount(50), RotationDepth(10)],
    layout,
    qs
);
```

## Code Style Guidelines

1. **Immutability first** — use `let` by default, `mutable` only when needed
2. **Expression-based** — `if`/`else` returns values; use it instead of mutable + reassignment
3. **`within`/`apply`** — prefer over manual uncomputation
4. **`fail` over `Fact`** — in tests, `fail` gives better error locations
5. **No `namespace` blocks** — filename is the namespace
6. **Import specific items** — `import Std.Math.PI;` over `import Std.Math.*;` in large files
7. **Type inference** — omit type annotations when types are obvious
8. **`ApplyToEach`** — prefer over manual `for` loops for gate application
