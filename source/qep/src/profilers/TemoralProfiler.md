This is description of model and assumptions made in TemporalProfiler.
TODO: complete this without using AI.

This profiler is a model of hypothetical hardware, which currently doesn't correspond to
any specific real quantum computer.

### Gates
Hardware supports the following operations:
 * Clifford gates (single-qubit and CX/CY/CZ) take `ProfilerConfig.clifford_gate_time_nanos`
   (5000ns by default in Rust).
 * Measurement, reset, or their combination takes `ProfilerConfig.measurment_time_nanos`
   (1000ns by default in Rust).
 * T/Tadj gate is implemented by "InjectT"/"InjectTAdj" instruction, both consuming a
   T-State and taking twice the Clifford duration plus the measurement duration
   (2*5000+1000=11000ns by default). Python requires both durations explicitly.

Operations can be performed in parallel as long as they don't act on the same qubits.

Not supported gates:
* CCX gates are not supported and must be decomposed into Clifford+T before reaching profiler.
* Rotation gates are not supported and must be approximated by Clifford+T gates before reaching profiler.

### Qubits

Computer consists of logical qubits:
* Logical qubits used by gates - their number is inferred from QubitId argumentsof instructions.
* Logical qubits used by factories - their number is computed as number of logical qubits
    per factory times the number of factories.

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
