---
geometry:
  - left=1cm
  - right=1cm
  - top=1cm
  - bottom=2cm
classoption:
  - 10pt
...

# Output data

In this document, we describe all output parameters which are printed by the Microsoft Quantum Resource Estimator. Second-level headings contain one group in the output that consists of a header and a table with resource estimation data. Each third-level heading corresponds to an entry, which is preceded by a short description text in italic and a long description text. The short description text will be always visible in the third column next to the value, whereas the long description text will be visible as a tooltip when hovering over the corresponding row in the table. The description texts may contain some variable names such as `variable` or $\mathtt{variable}$ in formulas. These will be substituted by the actual computed values extracted from the resource estimates.

## Physical resource estimates

### Runtime

[//]: # "physicalCountsFormatted/runtime"

_Total runtime_

This is a runtime estimate for the execution time of the algorithm. In general, the execution time corresponds to the duration of one logical cycle (`logicalQubit/logicalCycleTime` nanosecs) multiplied by the `physicalCounts/breakdown/algorithmicLogicalDepth` logical cycles to run the algorithm. If however the duration of a single T factory (here: `tfactory/runtime` nanosecs) is larger than the algorithm runtime, we extend the number of logical cycles artificially in order to exceed the runtime of a single T factory.

### rQOPS

[//]: # "physicalCountsFormatted/rqops"

_Reliable quantum operations per second_

The value is computed as the number of logical qubits after layout (`physicalCounts/breakdown/algorithmicLogicalQubits`) (with a logical error rate of `physicalCountsFormatted/requiredLogicalQubitErrorRate`) multiplied by the clock frequency (`logicalQubit/logicalCyclesPerSecond`), which is the number of logical cycles per second.

### Physical qubits

[//]: # "physicalCountsFormatted/physicalQubits"

_Number of physical qubits_

This value represents the total number of physical qubits, which is the sum of `physicalCounts/breakdown/physicalQubitsForAlgorithm` physical qubits to implement the algorithm logic, and `physicalCounts/breakdown/physicalQubitsForTfactories` physical qubits to execute the T factories that are responsible to produce the T states that are consumed by the algorithm.

## Resource estimates breakdown

### Logical algorithmic qubits

[//]: # "physicalCountsFormatted/algorithmicLogicalQubits"

_Number of logical qubits for the algorithm after layout_

Laying out the logical qubits in the presence of nearest-neighbor constraints requires additional logical qubits. In particular, to layout the $Q_{\rm alg} = \mathtt{logicalCounts/numQubits}$ logical qubits in the input algorithm, we require in total $2 \cdot Q_{\rm alg} + \lceil \sqrt{8 \cdot Q_{\rm alg}}\rceil + 1 = \mathtt{physicalCounts/breakdown/algorithmicLogicalQubits}$ logical qubits.

### Algorithmic depth

[//]: # "physicalCountsFormatted/algorithmicLogicalDepth"

_Number of logical cycles for the algorithm_

To execute the algorithm using _Parallel Synthesis Sequential Pauli Computation_ (PSSPC), operations are scheduled in terms of multi-qubit Pauli measurements, for which assume an execution time of one logical cycle. Based on the input algorithm, we require one multi-qubit measurement for the `logicalCounts/measurementCount` single-qubit measurements, the `logicalCounts/rotationCount` arbitrary single-qubit rotations, and the `logicalCounts/tCount` T gates, three multi-qubit measurements for each of the `logicalCounts/cczCount` CCZ and `logicalCounts/ccixCount` CCiX gates in the input program, as well as `physicalCountsFormatted/numTsPerRotation` multi-qubit measurements for each of the `logicalCounts/rotationDepth` non-Clifford layers in which there is at least one single-qubit rotation with an arbitrary angle rotation.

### Logical depth

[//]: # "physicalCountsFormatted/logicalDepth"

_Number of logical cycles performed_

This number is usually equal to the logical depth of the algorithm, which is `physicalCounts/breakdown/algorithmicLogicalDepth`. However, in the case in which a single T factory is slower than the execution time of the algorithm, we adjust the logical cycle depth to exceed the T factory's execution time.

### Clock frequency

[//]: # "physicalCountsFormatted/clockFrequency"

_Number of logical cycles per second_

This is the number of logical cycles that can be performed within one second. The logical cycle time is `physicalCountsFormatted/logicalCycleTime`.

### Number of T states

[//]: # "physicalCountsFormatted/numTstates"

_Number of T states consumed by the algorithm_

To execute the algorithm, we require one T state for each of the `logicalCounts/tCount` T gates, four T states for each of the `logicalCounts/cczCount` CCZ and `logicalCounts/ccixCount` CCiX gates, as well as `physicalCountsFormatted/numTsPerRotation` for each of the `logicalCounts/rotationCount` single-qubit rotation gates with arbitrary angle rotation.

### Number of T factories

[//]: # "physicalCountsFormatted/numTfactories"

_Number of T factories capable of producing the demanded `physicalCounts/breakdown/numTstates` T states during the algorithm's runtime_

The total number of T factories `physicalCounts/breakdown/numTfactories` that are executed in parallel is computed as $\left\lceil\dfrac{\text{T states}\cdot\text{T factory duration}}{\text{T states per T factory}\cdot\text{algorithm runtime}}\right\rceil = \left\lceil\dfrac{\mathtt{physicalCounts/breakdown/numTstates} \cdot \mathtt{tfactory/runtime}\;\text{ns}}{\mathtt{tfactory/numTstates} \cdot \mathtt{physicalCounts/runtime}\;\text{ns}}\right\rceil$

### Number of T factory invocations

[//]: # "physicalCountsFormatted/numTfactoryRuns"

_Number of times all T factories are invoked_

In order to prepare the `physicalCounts/breakdown/numTstates` T states, the `physicalCounts/breakdown/numTfactories` copies of the T factory are repeatedly invoked `physicalCounts/breakdown/numTfactoryRuns` times.

### Physical algorithmic qubits

[//]: # "physicalCountsFormatted/physicalQubitsForAlgorithm"

_Number of physical qubits for the algorithm after layout_

The `physicalCounts/breakdown/physicalQubitsForAlgorithm` are the product of the `physicalCounts/breakdown/algorithmicLogicalQubits` logical qubits after layout and the `logicalQubit/physicalQubits` physical qubits that encode a single logical qubit.

### Physical T factory qubits

[//]: # "physicalCountsFormatted/physicalQubitsForTfactories"

_Number of physical qubits for the T factories_

Each T factory requires `tfactory/physicalQubits` physical qubits and we run `physicalCounts/breakdown/numTfactories` in parallel, therefore we need $\mathtt{physicalCounts/breakdown/physicalQubitsForTfactories} = \mathtt{tfactory/physicalQubits} \cdot \mathtt{physicalCounts/breakdown/numTfactories}$ qubits.

### Required logical qubit error rate

[//]: # "physicalCountsFormatted/requiredLogicalQubitErrorRate"

_The minimum logical qubit error rate required to run the algorithm within the error budget_

The minimum logical qubit error rate is obtained by dividing the logical error probability `physicalCountsFormatted/errorBudgetLogical` by the product of `physicalCounts/breakdown/algorithmicLogicalQubits` logical qubits and the total cycle count `physicalCounts/breakdown/logicalDepth`.

### Required logical T state error rate

[//]: # "physicalCountsFormatted/requiredLogicalTstateErrorRate"

_The minimum T state error rate required for distilled T states_

The minimum T state error rate is obtained by dividing the T distillation error probability `physicalCountsFormatted/errorBudgetTstates` by the total number of T states `physicalCounts/breakdown/numTstates`.

### Number of T states per rotation

[//]: # "physicalCountsFormatted/numTsPerRotation"

_Number of T states to implement a rotation with an arbitrary angle_

The number of T states to implement a rotation with an arbitrary angle is $\lceil 0.53 \log_2(\mathtt{logicalCounts/rotationCount} / \mathtt{errorBudget/rotations}) + 4.86\rceil$ [[arXiv:2203.10064](https://arxiv.org/abs/2203.10064)]. For simplicity, we use this formula for all single-qubit arbitrary angle rotations, and do not distinguish between best, worst, and average cases.

## Logical qubit parameters

### QEC scheme

[//]: # "jobParams/qecScheme/name"

_Name of QEC scheme_

You can load pre-defined QEC schemes by using the name `surface_code` or `floquet_code`. The latter only works with Majorana qubits.

### Code distance

[//]: # "logicalQubit/codeDistance"

_Required code distance for error correction_

The code distance is the smallest odd integer greater or equal to $\dfrac{2\log(\mathtt{jobParams/qecScheme/crossingPrefactor} / \mathtt{physicalCounts/breakdown/requiredLogicalQubitErrorRate})}{\log(\mathtt{jobParams/qecScheme/errorCorrectionThreshold}/\mathtt{physicalCounts/breakdown/cliffordErrorRate})} - 1$

### Physical qubits

[//]: # "physicalCountsFormatted/physicalQubitsPerLogicalQubit"

_Number of physical qubits per logical qubit_

The number of physical qubits per logical qubit are evaluated using the formula `jobParams/qecScheme/physicalQubitsPerLogicalQubit` that can be user-specified.

### Logical cycle time

[//]: # "physicalCountsFormatted/logicalCycleTime"

_Duration of a logical cycle in nanoseconds_

The runtime of one logical cycle in nanoseconds is evaluated using the formula `jobParams/qecScheme/logicalCycleTime` that can be user-specified.

### Logical qubit error rate

[//]: # "physicalCountsFormatted/logicalErrorRate"

_Logical qubit error rate_

The logical qubit error rate is computed as $\mathtt{jobParams/qecScheme/crossingPrefactor} \cdot \left(\dfrac{\mathtt{physicalCounts/breakdown/cliffordErrorRate}}{\mathtt{jobParams/qecScheme/errorCorrectionThreshold}}\right)^\frac{\mathtt{logicalQubit/codeDistance} + 1}{2}$

### Crossing prefactor

[//]: # "jobParams/qecScheme/crossingPrefactor"

_Crossing prefactor used in QEC scheme_

The crossing prefactor is usually extracted numerically from simulations when fitting an exponential curve to model the relationship between logical and physical error rate.

### Error correction threshold

[//]: # "jobParams/qecScheme/errorCorrectionThreshold"

_Error correction threshold used in QEC scheme_

The error correction threshold is the physical error rate below which the error rate of the logical qubit is less than the error rate of the physical qubit that constitute it. This value is usually extracted numerically from simulations of the logical error rate.

### Logical cycle time formula

[//]: # "jobParams/qecScheme/logicalCycleTime"

_QEC scheme formula used to compute logical cycle time_

This is the formula that is used to compute the logical cycle time `logicalQubit/logicalCycleTime` ns.

### Physical qubits formula

[//]: # "jobParams/qecScheme/physicalQubitsPerLogicalQubit"

_QEC scheme formula used to compute number of physical qubits per logical qubit_

This is the formula that is used to compute the number of physical qubits per logical qubits `logicalQubit/physicalQubits`.

## T factory parameters

### Physical qubits

[//]: # "physicalCountsFormatted/tfactoryPhysicalQubits"

_Number of physical qubits for a single T factory_

This corresponds to the maximum number of physical qubits over all rounds of T distillation units in a T factory. A round of distillation contains of multiple copies of distillation units to achieve the required success probability of producing a T state with the expected logical T state error rate.

### Runtime

[//]: # "physicalCountsFormatted/tfactoryRuntime"

_Runtime of a single T factory_

The runtime of a single T factory is the accumulated runtime of executing each round in a T factory.

### Number of output T states per run

[//]: # "tfactory/numTstates"

_Number of output T states produced in a single run of T factory_

The T factory takes as input `tfactory/numInputTstates` noisy physical T states with an error rate of `jobParams/qubitParams/tGateErrorRate` and produces `tfactory/numTstates` T states with an error rate of `physicalCountsFormatted/tstateLogicalErrorRate`.

### Number of input T states per run

[//]: # "physicalCountsFormatted/numInputTstates"

_Number of physical input T states consumed in a single run of a T factory_

This value includes the physical input T states of all copies of the distillation unit in the first round.

### Distillation rounds

[//]: # "tfactory/numRounds"

_The number of distillation rounds_

This is the number of distillation rounds. In each round one or multiple copies of some distillation unit is executed.

### Distillation units per round

[//]: # "physicalCountsFormatted/numUnitsPerRound"

_The number of units in each round of distillation_

This is the number of copies for the distillation units per round.

### Distillation units

[//]: # "physicalCountsFormatted/unitNamePerRound"

_The types of distillation units_

These are the types of distillation units that are executed in each round. The units can be either physical or logical, depending on what type of qubit they are operating. Space-efficient units require fewer qubits for the cost of longer runtime compared to Reed-Muller preparation units.

### Distillation code distances

[//]: # "physicalCountsFormatted/codeDistancePerRound"

_The code distance in each round of distillation_

This is the code distance used for the units in each round. If the code distance is 1, then the distillation unit operates on physical qubits instead of error-corrected logical qubits.

### Number of physical qubits per round

[//]: # "physicalCountsFormatted/physicalQubitsPerRound"

_The number of physical qubits used in each round of distillation_

The maximum number of physical qubits over all rounds is the number of physical qubits for the T factory, since qubits are reused by different rounds.

### Runtime per round

[//]: # "physicalCountsFormatted/tfactoryRuntimePerRound"

_The runtime of each distillation round_

The runtime of the T factory is the sum of the runtimes in all rounds.

### Logical T state error rate

[//]: # "physicalCountsFormatted/tstateLogicalErrorRate"

_Logical T state error rate_

This is the logical T state error rate achieved by the T factory which is equal or smaller than the required error rate `physicalCountsFormatted/requiredLogicalTstateErrorRate`.

## Pre-layout logical resources

### Logical qubits (pre-layout)

[//]: # "physicalCountsFormatted/logicalCountsNumQubits"

_Number of logical qubits in the input quantum program_

We determine `physicalCounts/breakdown/algorithmicLogicalQubits` algorithmic logical qubits from this number by assuming to align them in a 2D grid. Auxiliary qubits are added to allow for sufficient space to execute multi-qubit Pauli measurements on all or a subset of the logical qubits.

### T gates

[//]: # "physicalCountsFormatted/logicalCountsTCount"

_Number of T gates in the input quantum program_

This includes all T gates and adjoint T gates, but not T gates used to implement rotation gates with arbitrary angle, CCZ gates, or CCiX gates.

### Rotation gates

[//]: # "physicalCountsFormatted/logicalCountsRotationCount"

_Number of rotation gates in the input quantum program_

This is the number of all rotation gates. If an angle corresponds to a Pauli, Clifford, or T gate, it is not accounted for in this number.

### Rotation depth

[//]: # "physicalCountsFormatted/logicalCountsRotationDepth"

_Depth of rotation gates in the input quantum program_

This is the number of all non-Clifford layers that include at least one single-qubit rotation gate with an arbitrary angle.

### CCZ gates

[//]: # "physicalCountsFormatted/logicalCountsCczCount"

_Number of CCZ-gates in the input quantum program_

This is the number of CCZ gates.

### CCiX gates

[//]: # "physicalCountsFormatted/logicalCountsCcixCount"

_Number of CCiX-gates in the input quantum program_

This is the number of CCiX gates, which applies $-iX$ controlled on two control qubits [[1212.5069](https://arxiv.org/abs/1212.5069)].

### Measurement operations

[//]: # "physicalCountsFormatted/logicalCountsMeasurementCount"

_Number of single qubit measurements in the input quantum program_

This is the number of single qubit measurements in Pauli basis that are used in the input program. Note that all measurements are counted, however, the measurement result is is determined randomly (with a fixed seed) to be 0 or 1 with a probability of 50%.

## Assumed error budget

### Total error budget

[//]: # "physicalCountsFormatted/errorBudget"

_Total error budget for the algorithm_

The total error budget sets the overall allowed error for the algorithm, i.e., the number of times it is allowed to fail. Its value must be between 0 and 1 and the default value is 0.001, which corresponds to 0.1%, and means that the algorithm is allowed to fail once in 1000 executions. This parameter is highly application specific. For example, if one is running Shor's algorithm for factoring integers, a large value for the error budget may be tolerated as one can check that the output are indeed the prime factors of the input. On the other hand, a much smaller error budget may be needed for an algorithm solving a problem with a solution which cannot be efficiently verified. This budget $\epsilon = \epsilon_{\log} + \epsilon_{\rm dis} + \epsilon_{\rm syn}$ is uniformly distributed and applies to errors $\epsilon_{\log}$ to implement logical qubits, an error budget $\epsilon_{\rm dis}$ to produce T states through distillation, and an error budget $\epsilon_{\rm syn}$ to synthesize rotation gates with arbitrary angles. Note that for distillation and rotation synthesis, the respective error budgets $\epsilon_{\rm dis}$ and $\epsilon_{\rm syn}$ are uniformly distributed among all T states and all rotation gates, respectively. If there are no rotation gates in the input algorithm, the error budget is uniformly distributed to logical errors and T state errors.

### Logical error probability

[//]: # "physicalCountsFormatted/errorBudgetLogical"

_Probability of at least one logical error_

This is one third of the total error budget `physicalCountsFormatted/errorBudget` if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise.

### T distillation error probability

[//]: # "physicalCountsFormatted/errorBudgetTstates"

_Probability of at least one faulty T distillation_

This is one third of the total error budget `physicalCountsFormatted/errorBudget` if the input algorithm contains rotation with gates with arbitrary angles, or one half of it, otherwise.

### Rotation synthesis error probability

[//]: # "physicalCountsFormatted/errorBudgetRotations"

_Probability of at least one failed rotation synthesis_

This is one third of the total error budget `physicalCountsFormatted/errorBudget`.

## Physical qubit parameters

_Info:_ Note that not all values are shown depending on the instruction set of the qubit parameters.

### Qubit name

[//]: # "jobParams/qubitParams/name"

_Some descriptive name for the qubit model_

You can load pre-defined qubit parameters by using the names `qubit_gate_ns_e3`, `qubit_gate_ns_e4`, `qubit_gate_us_e3`, `qubit_gate_us_e4`, `qubit_maj_ns_e4`, or `qubit_maj_ns_e6`. The names of these pre-defined qubit parameters indicate the instruction set (gate-based or Majorana), the operation speed (ns or µs regime), as well as the fidelity (e.g., e3 for $10^{-3}$ gate error rates).

### Instruction set

[//]: # "jobParams/qubitParams/instructionSet"

_Underlying qubit technology (gate-based or Majorana)_

When modeling the physical qubit abstractions, we distinguish between two different physical instruction sets that are used to operate the qubits. The physical instruction set can be either _gate-based_ or _Majorana_. A gate-based instruction set provides single-qubit measurement, single-qubit gates (incl. T gates), and two-qubit gates. A Majorana instruction set provides a physical T gate, single-qubit measurement and two-qubit joint measurement operations.

### Single-qubit measurement time

[//]: # "jobParams/qubitParams/oneQubitMeasurementTime"

_Operation time for single-qubit measurement (t_meas) in ns_

This is the operation time in nanoseconds to perform a single-qubit measurement in the Pauli basis.

### Two-qubit measurement time

[//]: # "jobParams/qubitParams/twoQubitJointMeasurementTime"

_Operation time for two-qubit measurement in ns_

This is the operation time in nanoseconds to perform a non-destructive two-qubit joint Pauli measurement.

### Single-qubit gate time

[//]: # "jobParams/qubitParams/oneQubitGateTime"

_Operation time for single-qubit gate (t_gate) in ns_

This is the operation time in nanoseconds to perform a single-qubit Clifford operation, e.g., Hadamard or Phase gates.

### Two-qubit gate time

[//]: # "jobParams/qubitParams/twoQubitGateTime"

_Operation time for two-qubit gate in ns_

This is the operation time in nanoseconds to perform a two-qubit Clifford operation, e.g., a CNOT or CZ gate.

### T gate time

[//]: # "jobParams/qubitParams/tGateTime"

_Operation time for a T gate_

This is the operation time in nanoseconds to execute a T gate.

### Single-qubit measurement error rate

[//]: # "jobParams/qubitParams/oneQubitMeasurementErrorRate"

_Error rate for single-qubit measurement_

This is the probability in which a single-qubit measurement in the Pauli basis may fail.

### Two-qubit measurement error rate

[//]: # "jobParams/qubitParams/twoQubitJointMeasurementErrorRate"

_Error rate for two-qubit measurement_

This is the probability in which a non-destructive two-qubit joint Pauli measurement may fail.

### Single-qubit error rate

[//]: # "jobParams/qubitParams/oneQubitGateErrorRate"

_Error rate for single-qubit Clifford gate (p)_

This is the probability in which a single-qubit Clifford operation, e.g., Hadamard or Phase gates, may fail.

### Two-qubit error rate

[//]: # "jobParams/qubitParams/twoQubitGateErrorRate"

_Error rate for two-qubit Clifford gate_

This is the probability in which a two-qubit Clifford operation, e.g., CNOT or CZ gates, may fail.

### T gate error rate

[//]: # "jobParams/qubitParams/tGateErrorRate"

_Error rate to prepare single-qubit T state or apply a T gate (p_T)_

This is the probability in which executing a single T gate may fail.

## Constraints

### Logical depth factor

[//]: # "physicalCountsFormatted/logicalDepthFactor"

_Factor the initial number of logical cycles is multiplied by_

This is the factor takes into account a potential overhead to the initial number of logical cycles.

### Maximum number of T factories

[//]: # "physicalCountsFormatted/maxTFactories"

_The maximum number of T factories can be utilized during the algorithm's runtime_

This is the maximum number of T factories used for producing the demanded T states, which can be created and executed by the algorithm in parallel.

### Maximum runtime duration

[//]: # "physicalCountsFormatted/maxDuration"

_The maximum runtime duration allowed for the algorithm runtime_

This is the maximum time allowed to the algorithm. If specified, the estimator targets to minimize the number of physical qubits consumed by the algorithm for runtimes under the maximum allowed.

### Maximum number of physical qubits

[//]: # "physicalCountsFormatted/maxPhysicalQubits"

_The maximum number of physical qubits allowed for utilization to the algorithm_

This is the maximum number of physical qubits available to the algorithm. If specified, the estimator targets to minimize the runtime of the algorithm with number of physical qubits consumed not exceeding this maximum.

## Assumptions

- _More details on the following lists of assumptions can be found in the paper [Accessing requirements for scaling quantum computers and their applications](https://aka.ms/AQ/RE/Paper)._
- **Uniform independent physical noise.** We assume that the noise on physical qubits and physical qubit operations is the standard circuit noise model. In particular we assume error events at different space-time locations are independent and that error rates are uniform across the system in time and space.
- **Efficient classical computation.** We assume that classical overhead (compilation, control, feedback, readout, decoding, etc.) does not dominate the overall cost of implementing the full quantum algorithm.
- **Extraction circuits for planar quantum ISA.** We assume that stabilizer extraction circuits with similar depth and error correction performance to those for standard surface and Hastings-Haah code patches can be constructed to implement all operations of the planar quantum ISA (instruction set architecture).
- **Uniform independent logical noise.** We assume that the error rate of a logical operation is approximately equal to its space-time volume (the number of tiles multiplied by the number of logical time steps) multiplied by the error rate of a logical qubit in a standard one-tile patch in one logical time step.
- **Negligible Clifford costs for synthesis.** We assume that the space overhead for synthesis and space and time overhead for transport of magic states within magic state factories and to synthesis qubits are all negligible.
- **Smooth magic state consumption rate.** We assume that the rate of T state consumption throughout the compiled algorithm is almost constant, or can be made almost constant without significantly increasing the number of logical time steps for the algorithm.
