# Q# Programming

Write correct, idiomatic Q# code for quantum computing.

## Standard Library Documentation

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

// With target profile
@EntryPoint(Adaptive_RI)
operation Main() : Result { ... }
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

| Type     | Examples                               | Notes                           |
| -------- | -------------------------------------- | ------------------------------- |
| `Int`    | `42`, `0xFF`, `0b1010`, `0o52`         | 64-bit signed                   |
| `BigInt` | `42L`, `0xFFL`                         | Arbitrary precision, `L` suffix |
| `Double` | `3.14`, `1.0e-3`                       | 64-bit floating point           |
| `Bool`   | `true`, `false`                        |                                 |
| `String` | `"hello"`, `$"x={x}"`                  | String interpolation with `$`   |
| `Result` | `Zero`, `One`                          | Measurement outcome             |
| `Pauli`  | `PauliI`, `PauliX`, `PauliY`, `PauliZ` | Pauli basis                     |
| `Range`  | `1..10`, `0..2..8`, `10..-1..0`        | start..step..end                |
| `Unit`   | `()`                                   | No value                        |

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
// String interpolation: $"x={x}"  Array update: arr[i] = val
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
} until MResetZ(q) == Zero;

// Repeat-until with fixup block
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
// Declare with `is` keyword — compiler auto-generates Adjoint/Controlled
operation MyGate(q : Qubit) : Unit is Adj + Ctl { H(q); }

// Use functors
Adjoint MyGate(q);                  // Inverse
Controlled MyGate([ctrl], q);       // Controlled version
Controlled Adjoint MyGate([ctrl], q);

// Manual specializations (when auto-generation isn't sufficient):
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

## Standard Library (Std)

**Always call `qsharpGetLibraryDescriptions`** for authoritative signatures. Key namespaces:

| Namespace                | Purpose         | Key Items                                                                                                                        |
| ------------------------ | --------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `Std.Arrays`             | Array ops       | `Length`, `Mapped`, `Fold`, `Zipped`, `Enumerated`, `Reversed`, `Sorted`, `Head`, `Tail`, `Rest`, `Most`, `Chunks`, `All`, `Any` |
| `Std.Math`               | Math            | `PI()`, `E()`, `Sin`, `Cos`, `Sqrt`, `Log`, `Exp`, `AbsD`, `AbsI`, `Min`, `Max`, `BitSizeI`, `GreatestCommonDivisorI`            |
| `Std.Convert`            | Type conversion | `IntAsDouble`, `IntAsBigInt`, `BoolArrayAsInt`, `IntAsBoolArray`, `ResultAsBool`                                                 |
| `Std.Diagnostics`        | Debug/test      | `Message`, `DumpMachine`, `DumpRegister`, `Fact`, `CheckZero`, `CheckOperationsAreEqual`                                         |
| `Std.Measurement`        | Measurement     | `MResetZ`, `MResetEachZ`, `MeasureAllZ`, `MeasureEachZ`                                                                          |
| `Std.Canon`              | Patterns        | `ApplyToEach`, `ApplyToEachA`, `ApplyToEachCA`                                                                                   |
| `Std.Random`             | RNG             | `DrawRandomInt`, `DrawRandomDouble`, `DrawRandomBool`                                                                            |
| `Std.StatePreparation`   | State prep      | `PreparePureStateD`, `PrepareUniformSuperposition`                                                                               |
| `Std.Arithmetic`         | Quantum arith   | `IncByI`, `IncByLE`, `AddLE`, `ReflectAboutInteger`                                                                              |
| `Std.ResourceEstimation` | RE hints        | `AccountForEstimates`, `BeginEstimateCaching`, `AuxQubitCount`                                                                   |

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

Use the `runTests` tool if available to run Q# tests.

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

For standalone `.qs` files (no `qsharp.json`), declare the profile on the entry point:

```qsharp
@EntryPoint(Adaptive_RI)
operation Main() : Result { ... }
```

| Profile / Attribute             | Description                                                                                                          |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `unrestricted`                  | Full simulation (default)                                                                                            |
| `adaptive_rif` / `Adaptive_RIF` | Adaptive profile with integer & floating-point computation extensions; required for `CircuitGenerationMethod.Static` |
| `adaptive_ri` / `Adaptive_RI`   | Adaptive profile with integer computation extension                                                                  |
| `base` / `Base`                 | Minimal capabilities required to run a quantum program (Base Profile per QIR spec)                                   |

## Running, Estimation, Circuits, and Azure Quantum

Running programs, resource estimation, circuit diagrams, and Azure Quantum submission are available directly via tools. For Python and Jupyter workflows, see [python.md](./python.md).
