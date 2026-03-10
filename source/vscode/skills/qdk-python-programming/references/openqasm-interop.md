# OpenQASM Interop

Run, compile, and estimate OpenQASM 3.0 programs from Python.

## Running OpenQASM

```python
from qsharp.openqasm import run

source = """
include "stdgates.inc";
bit[2] c;
qubit[2] q;
h q[0];
cx q[0], q[1];
c = measure q;
"""

results = run(source, shots=100, as_bitstring=True)
```

## With Noise

```python
from qsharp import DepolarizingNoise
results = run(source, shots=1000, noise=DepolarizingNoise(0.01), as_bitstring=True)
```

## Compiling OpenQASM to QIR

```python
from qsharp.openqasm import compile
from qsharp import TargetProfile

qir = compile(source, target_profile=TargetProfile.Base)
```

## Importing OpenQASM as Q# Callable

```python
from qsharp.openqasm import import_openqasm, ProgramType

import_openqasm(source, name="bell", program_type=ProgramType.File)
from qsharp.code.qasm_import import bell
result = bell()  # call it like a Q# operation
```

## Parameterized OpenQASM

```python
source = """
include "stdgates.inc";
input float theta;
qubit q;
rx(theta) q;
"""
import_openqasm(source, name="rotated", program_type=ProgramType.File)
from qsharp.code.qasm_import import rotated
qir = compile(rotated, 1.57)  # bind parameter
```

## Resource Estimation from OpenQASM

```python
from qsharp.openqasm import estimate
result = estimate(source, {"qubitParams": {"name": "qubit_gate_ns_e3"}})
```

## Loss Detection with qdk.inc

```python
source = """
include "stdgates.inc";
include "qdk.inc";
qubit q;
output int res;
h q;
res = mresetz_checked(q);  // returns 0 (Zero), 1 (One), or 2 (Loss)
"""
results = run(source, shots=1000, qubit_loss=0.1)
```
