This is description of model and assumptions made in TemporalProfiler.
TODO: complete this without using AI.

This profiler is a model of hypothetical hardware, which currently doesn't correspond to
any specific real quantum computer.

### Gates
Hardware supports the following operations:
 * Clifford gates (single-qubit and CX/CY/CZ) take the same time (5000ns by default,
    configurable with `TemporalProfilerConfig.clifford_gate_time_nanos`).
 * Measurement time is configurable with `TemporalProfilerConfig.measurment_time_nanos`,
    1000ns by default.
 * T/Tadj gate is implemented by "InjectT"/"InjectTAdj" instruction, both consuming a
   T-State and taking twice the Clifford duration plus the measurement duration
   (2*5000+1000=11000ns by default). Python requires both durations explicitly.

Operations can be performed in parallel as long as they don't act on the same qubits.

Not supported gates:
* CCX gates are not supported and must be decomposed into Clifford+T before reaching profiler.
* Rotation gates are not supported and must be approximated by Clifford+T gates before reaching profiler.

### Logical qubits

Computer consists of logical qubits:
* Logical qubits used by gates - their number is inferred from QubitId argumentsof instructions.
* Logical qubits used by factories - their number is computed as number of logical qubits
    per factory times the number of factories.

`totals.logical_compute_qubits` reports only the qubits used by gates.
`totals.logical_qubits` includes both compute and factory qubits. The trace viewer
uses the compute count for qubit lanes and displays factories on separate lanes.

### Physical qubits

Each logical qubit is assumed to use a rotated surface code of distance
`d = TemporalProfilerConfig.ecc_distance` (5 by default). A logical qubit requires
`d^2` physical data qubits and `d^2 - 1` syndrome-extraction ancillas, giving
`2*d^2 - 1` physical qubits per logical qubit. This is the same formula used by
QREv3's [SurfaceCode model](../../../qdk_package/qdk/qre/models/qec/_surface_code.py).

The total is computed as:
* Physical qubits used by gates: `gate_logical_qubits * (2*d^2 - 1)`.
* Physical qubits used by factories: the sum of each factory's physical-qubit
  count, currently `5 * (2*d^2 - 1)` per factory.

All factories currently receive the same code distance as the gate qubits, so
the total is equivalent to `logical_qubits * (2*d^2 - 1)`, where `logical_qubits`
already includes factory qubits. At distance 5, this is 49 physical qubits per
logical qubit, or 245 per five-logical-qubit factory.

This is an encoded-patch estimate: no additional routing space, lattice-surgery
workspace, memory buffers, or hardware spares are counted. The distance is
provided by the caller, not selected from a target failure probability.

### Factories

* The profiler has several T-factories whose number is specified in constructor (and 
affects the resource estimation).
* These are distillation factories, and it's assumed that raw T state production is also 
part of these factories.
* In one round, the factory generates T state with output T-state error rate in one of logical
qubits owned by the factory. In the end of round, we check if round succeeded with 
certain probability, and if not, round is repeated.
* If a round succeeded, the factory waits until the state is consumed by InjectT/InjectTAdj, after which
new round starts immediately.
* Each T/TAdj gate uses T-state from a specific factory. If a state is not available,
profiler waits for T-state to be available.
* If a factory generated a T-state and there is no gate to consume it, the factory waits 
and doesn't start new round, because that would destroy prepared T-State. We could copy 
the state to a "memory qubit" instead and start new round, but this is not implemented 
in the current model (under the assumption that demand for T states is higher than the 
production speed).

Formulas we currently use:
* `discard_probability = 15.0 * input_state_error + 356.0 * epsilon`
* `input_state_error = 1e-3`
* `epsilon = 1e-5`. 
