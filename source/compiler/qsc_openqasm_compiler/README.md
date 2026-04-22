# OpenQASM 3 to Q# Compiler

Compiles [OpenQASM 3](https://openqasm.com/) programs into Q# AST, part of the [Microsoft Quantum Development Kit](https://github.com/microsoft/qdk).

## Overview

This crate transforms OpenQASM 3 semantic AST (produced by `qsc_openqasm_parser`) into a Q# AST (`Package`). After this transformation, the result is indistinguishable from native Q# source input, enabling the full Q# compilation pipeline—capability analysis, runtime targeting, partial evaluation, and QIR code generation—to be applied.

## Compiler Configuration

The compilation behavior is controlled by `CompilerConfig`:

- **`QubitSemantics`** — `QSharp` or `Qiskit`: controls qubit allocation and reset semantics
- **`OutputSemantics`** — `Qiskit`, `OpenQasm`, or `ResourceEstimation`: controls how program outputs are structured
- **`ProgramType`** — `File`, `Operation`, or `Fragments`: controls the shape of the generated Q# AST

## Semantic Translation

The two languages have significant differences that the compiler handles during transformation:

- **Qubit management:**
  - Q# assumes qubits are in the |0⟩ state when allocated; OpenQASM does not
  - Q# requires qubits to be reset to |0⟩ when released; OpenQASM does not
- **Variable initialization:** Q# requires explicit initialization; OpenQASM allows implicit initialization
- **Type casting:** Q# requires explicit conversions; OpenQASM allows implicit C99-style casting and promotion
- **Type system:** Q# has no unsigned integers or angle type; all integers are signed

### QIR Constraints

OpenQASM output registers may have unpopulated measurement indexes. In QIR, `Result`s can only be acquired through measurement, so all output register entries must be measured into or a code generation error occurs.

### Implementation Details

- Gates are implemented as lambda expressions capturing `const` variables from the global scope
  - Exception: `@SimulatableIntrinsic`-annotated gates are defined as full local `operation`s (lambdas cannot capture in that context)
- OpenQASM `const` is modeled as Q# immutable `let` bindings

## Relationship to Other Crates

```text
OpenQASM source → qsc_openqasm_parser → Semantic AST → [qsc_openqasm_compiler] → Q# AST → qsc
```

This crate depends on `qsc_openqasm_parser` for input and is consumed by:

- `qsc` — Core compiler facade

## License

MIT
