# OpenQASM Programming

Write correct OpenQASM 2.0 and 3.0 code. This file covers syntax, standard gates, version differences, and common patterns.

## OpenQASM Versions

OpenQASM 3.0 is the latest version and a superset of 2.0 — all OpenQASM 2.0 code is valid in 3.0.

### File Headers

```qasm
// OpenQASM 3.0
OPENQASM 3.0;
include "stdgates.inc";

// OpenQASM 2.0
OPENQASM 2.0;
include "qelib1.inc";
```

Always start with the version declaration, then include the standard gates.

## Standard Gates

### OpenQASM 3.0 — `stdgates.inc`

| Gate                | Signature          | Description          |
| ------------------- | ------------------ | -------------------- |
| `p`                 | `p(λ) q`           | Phase gate           |
| `x`, `y`, `z`       | `x q`              | Pauli gates          |
| `h`                 | `h q`              | Hadamard             |
| `s`, `sdg`          | `s q`              | S and S-dagger       |
| `t`, `tdg`          | `t q`              | T and T-dagger       |
| `sx`                | `sx q`             | √X gate              |
| `rx`, `ry`, `rz`    | `rx(θ) q`          | Rotation gates       |
| `cx`                | `cx c, t`          | CNOT                 |
| `cy`, `cz`          | `cy c, t`          | Controlled-Y/Z       |
| `cp`                | `cp(λ) c, t`       | Controlled phase     |
| `crx`, `cry`, `crz` | `crx(θ) c, t`      | Controlled rotations |
| `ch`                | `ch c, t`          | Controlled Hadamard  |
| `swap`              | `swap a, b`        | SWAP                 |
| `ccx`               | `ccx c1, c2, t`    | Toffoli              |
| `cswap`             | `cswap c, a, b`    | Fredkin              |
| `cu`                | `cu(θ,φ,λ,γ) c, t` | Controlled-U         |

Compatibility gates (from 2.0): `CX`, `phase`, `cphase`, `id`, `u1`, `u2`, `u3`.

### OpenQASM 2.0 — `qelib1.inc`

Gates: `u3`, `u2`, `u1`, `cx`, `id`, `x`, `y`, `z`, `h`, `s`, `sdg`, `t`, `tdg`, `rx`, `ry`, `rz`, `cz`, `cy`, `ch`, `ccx`, `crz`, `cu1`, `cu3`.

### Built-in Gates (No Include Needed)

- Both versions: `U(θ, ϕ, λ)` — general single-qubit unitary
- OpenQASM 2.0 only: `CX` — CNOT
- OpenQASM 3.0 only: `gphase(γ)` — global phase (zero-qubit gate)

All other gates require an `include` or explicit `gate` definition.

## Syntax Quick Reference

### Qubits and Bits

```qasm
qubit q;              // single qubit
qubit[4] qs;          // qubit register
bit c;                // single classical bit
bit[4] cs;            // classical register
```

### Measurement

```qasm
c = measure q;        // measure qubit into classical bit
cs = measure qs;      // measure register
```

### Conditionals (3.0)

```qasm
bit c;
c = measure q;
if (c) {
    x q;              // apply X if measured One
}
```

### Output Declarations (3.0)

```qasm
// output declarations CANNOT be assigned on the same line
output bit c;
c = measure q;        // separate assignment line required

// INVALID: output bit c = measure q;
```

### Custom Gates

```qasm
gate bell q0, q1 {
    h q0;
    cx q0, q1;
}
```

### Loops (3.0)

```qasm
for int i in [0:3] {
    h qs[i];
}
```

## Target Profiles (Optional)

Use the `#pragma qdk.qir.profile` directive to set the target profile for compilation to QIR. If omitted, the default profile is used.

```qasm
OPENQASM 3.0;
include "stdgates.inc";
#pragma qdk.qir.profile Adaptive_RI
```

| Profile          | Description                                                           |
| ---------------- | --------------------------------------------------------------------- |
| `Unrestricted`   | Full simulation (default when pragma is omitted)                      |
| `Adaptive_RIFLA` | Adaptive profile with integer, floating-point, loops, and arrays      |
| `Adaptive_RIF`   | Adaptive profile with integer & floating-point computation extensions |
| `Adaptive_RI`    | Adaptive profile with integer computation extension                   |
| `Base`           | Minimal capabilities required to run a quantum program                |

## Running, Estimation, Circuits, and Azure Quantum

Running programs, resource estimation, circuit diagrams, and Azure Quantum submission are available directly via tools. For Python and Jupyter workflows, see [python.md](./python.md).
