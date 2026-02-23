---
applyTo: "**/*.{qasm,inc,ipynb}"
---

# OpenQASM coding instructions

Follow these instructions when generating OpenQASM code in .qasm files.

## Syntax

OpenQASM has two versions: 2.0 and 3.0. The latest version is 3.0, which adds many new features.

OpenQASM 3.0 syntax is a superset of OpenQASM 2.0, so all OpenQASM 2.0 code is valid in OpenQASM 3.0.

OpenQASM files can include other files using the `include` statement. This is similar to C/C++ `#include` directives: `include "stdgates.inc";`

OpenQASM 2.0 has a built-in include file `"qelib1.inc"` which defines the standard library for OpenQASM 2.0 programs.
The `"qelib1.inc"` file in OpenQASM 2.0 defines the standard gates and operations: `u3`, `u2`, `u1`, `cx`, `id`, `x`, `y`, `z`, `h`, `s`, `sdg`, `t`, `tdg`, `rx`, `ry`, `rz`, `cz`, `cy`, `ch`, `ccx`, `crz`, `cu1`, and `cu3`.
OpenQASM 2.0 has two built-in gates being the one-qubit gate `U(θ, ϕ, λ)` and the two-qubit gate `CX`.

OpenQASM 3.0 has a built-in include file `"stdgates.inc"` which defines the standard library for OpenQASM 3.0 programs.
The `"stdgates.inc"` file in OpenQASM 3.0 defines the standard gates and operations: `p`, `x`, `y`, `z`, `h`, `s`, `sdg`, `t`, `tdg`, `sx`, `rx`, `ry`, `rz`, `cx`, `cy`, `cz`, `cp`, `crx`, `cry`, `crz`, `ch`, `swap`, `ccx`, `cswap`, and `cu`.
The `"stdgates.inc"` file also defines the following gates for OpenQASM 2.0 compatibility: `CX`, `phase`, `cphase`, `id`, `u1`, `u2` and `u3`.
OpenQASM 3.0 has two built-in gates being the one-qubit gate `U(θ, ϕ, λ)` and the zero-qubit gate `gphase(γ)`.

Only the built-in gates are allowed without including them from another file or defining them in the program.

When generating OpenQASM 3.0 file, start with the version declaration: `OPENQASM 3.0;` followed by `include "stdgates.inc";`.

When generating OpenQASM 2.0 file, start with the version declaration: `OPENQASM 2.0;` followed by `include "qelib1.inc";`.

`output` declarations, only available in OpenQASM 3.0, cannot be assigned to in the same line that they are declared. For example, the following is invalid: `output bit c = measure q;`. Instead, use a separate line for the assignment: `output bit c; c = measure q;`

## Python interop

The `qsharp` package provides a way to compile, run, and resource estimate OpenQASM 3.0 programs in Python. The functions can be imported from the `qsharp.openqasm` module and are named `compile`, `run`, and `estimate`.

OpenQASM 3.0 programs can be imported into Python using the `qsharp.openqasm.import_openqasm` function and accessed via the `qsharp.code.qasm_import` module: `qsharp.openqasm.import_openqasm("qubit q; bit c; c = measure q;", name="Foo", program_type=qsharp.openqasm.ProgramType.File); qsharp.code.qasm_import.Foo()`.

OpenQASM 3.0 programs can also be imported as operations into Q#, where the qubits used become paramters to the resulting operation: `qsharp.openqasm.import_openqasm("qubit q; bit c; c = measure q;", name="Foo", program_type=qsharp.openqasm.ProgramType.Operation); qsharp.eval("{ use q = Qubit(); Foo(q) }")`.

OpenQASM 3.0 programs can be run or simulated via Python using the `qsharp.openqasm.run` function: `qsharp.openqasm.run("qubit q; bit c; c = measure q;");`
Quantum resource estimation can be done to OpenQASM 3.0 programs using the `qsharp.openqasm.estimate` function: `qsharp.openqasm.estimate("qubit q; bit c; c = measure q;");`
OpenQASM 3.0 programs can compiled to Quantum Intermediate Representation (QIR) using the `qsharp.openqasm.compile` function: `qsharp.openqasm.compile("qubit q; bit c; c = measure q;");`

## Response formatting

Avoid using LaTeX in your responses to the user.
