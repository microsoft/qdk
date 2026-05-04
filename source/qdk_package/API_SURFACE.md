# `qdk` Package — Public API Surface

> **Delete this file before merging.**

## `qdk`

```
qdk
├── code                                  # submodule — dynamic Q# callable namespace
├── Result                                # enum (Zero, One, Loss)
├── TargetProfile                         # enum (Base, Adaptive_RI, Adaptive_RIF, Adaptive_RIFLA, Unrestricted)
├── StateDump                             # class — quantum state snapshot
├── ShotResult                            # TypedDict — single shot result
├── PauliNoise                            # class — (x, y, z) noise tuple
├── DepolarizingNoise                     # class — uniform Pauli noise
├── BitFlipNoise                          # class — X-only noise
├── PhaseFlipNoise                        # class — Z-only noise
├── init()                                # function — initialize the Q# interpreter
├── set_quantum_seed()                    # function
├── set_classical_seed()                  # function
└── dump_machine()                        # function — get current state
```

## `qdk.qsharp`

Q# interpreter — the main entry point for writing and running Q# programs.

```
qdk.qsharp
│
│   # Interpreter lifecycle
├── init()                                # initialize interpreter with target profile, project root, etc.
├── get_interpreter()                     # get the current Interpreter instance
├── get_config()                          # get the current Config
│
│   # Execution
├── eval()                                # evaluate Q# source code
├── run()                                 # run an entry expression for N shots
├── compile()                             # compile to QIR → QirInputData
├── circuit()                             # generate a circuit diagram → Circuit
├── estimate()                            # resource estimation (deprecated — use qdk.qre)
├── logical_counts()                      # get logical resource counts → LogicalCounts
│
│   # State inspection
├── dump_machine()                        # get current quantum state → StateDump
├── dump_circuit()                        # get circuit so far → Circuit
├── dump_operation()                      # get unitary matrix of an operation
├── set_quantum_seed()                    # set quantum RNG seed
├── set_classical_seed()                  # set classical RNG seed
│
│   # Types & Data Classes
├── Config                                # class — interpreter configuration
├── QirInputData                          # class — compiled QIR output
├── StateDump                             # class — quantum state data
├── ShotResult                            # TypedDict — shot result with events/messages
│
│   # Noise types
├── PauliNoise                            # class — (x, y, z) noise specification
├── DepolarizingNoise                     # class — uniform depolarizing
├── BitFlipNoise                          # class — bit-flip noise
├── PhaseFlipNoise                        # class — phase-flip noise
├── NoiseConfig                           # class — per-gate noise configuration
│
│   # Enums
├── Result                                # enum (Zero, One, Loss)
├── Pauli                                 # enum (I, X, Y, Z)
├── TargetProfile                         # enum (Base, Adaptive_RI, ..., Unrestricted)
├── CircuitGenerationMethod               # enum (ClassicalEval, Simulate, Static)
│
│   # Native types
├── Interpreter                           # class — the Q# interpreter
├── Circuit                               # class — circuit representation
├── CircuitConfig                         # class — circuit generation options
├── Output                                # class — interpreter output
├── GlobalCallable                        # class — Q# callable reference
├── Closure                               # class — Q# closure reference
├── StateDumpData                         # class — raw state dump from native
├── QSharpError                           # exception
│
│   # Estimator types (re-exported)
├── EstimatorResult                       # class — resource estimation result
├── EstimatorParams                       # class — resource estimation parameters
└── LogicalCounts                         # class — logical resource counts
```

## `qdk.simulation`

Simulation APIs — neutral atom device, QIR execution, and noisy simulators.

```
qdk.simulation
├── NeutralAtomDevice                     # class — neutral atom device compiler & simulator
│   ├── compile(program, verbose)
│   ├── show_trace(qir)
│   └── simulate(qir, shots, noise, type, seed)
├── NoiseConfig                           # class — per-gate noise tables
│   ├── .x, .y, .z, .h, .s, .t, ...      #   NoiseTable per gate type
│   ├── .intrinsics                       #   NoiseIntrinsicsTable
│   ├── intrinsic(name, num_qubits)
│   └── load_csv_dir(dir_path)
├── run_qir()                             # function — run QIR with optional noise
│
│   # Experimental noisy simulation
├── NoisySimulatorError                   # exception
├── Operation                             # class — Kraus operator representation
├── Instrument                            # class — quantum instrument
├── DensityMatrixSimulator                # class — density matrix simulator
├── StateVectorSimulator                  # class — state vector simulator
├── DensityMatrix                         # class — density matrix state
└── StateVector                           # class — state vector state
```

## `qdk.estimator`

Resource estimation (v1) — physical qubit and QEC parameter estimation.

```
qdk.estimator
├── EstimatorParams                       # class — estimation input parameters
├── EstimatorInputParamsItem              # class — single parameter set
├── EstimatorResult                       # class — estimation output (dict subclass)
│   ├── data(idx)
│   ├── summary, diagram, plot()
│   └── summary_data_frame()
├── LogicalCounts                         # class — logical resource counts (dict subclass)
│   └── estimate(params) → EstimatorResult
├── EstimatorError                        # exception
│
│   # Parameter building blocks
├── QubitParams                           # class — predefined qubit models
│   └── GATE_US_E3, GATE_US_E4, GATE_NS_E3, GATE_NS_E4, MAJ_NS_E4, MAJ_NS_E6
├── QECScheme                             # class — predefined QEC schemes
│   └── SURFACE_CODE, FLOQUET_CODE
├── MeasurementErrorRate                  # dataclass
├── EstimatorQubitParams                  # dataclass
├── EstimatorQecScheme                    # dataclass
├── ProtocolSpecificDistillationUnitSpecification  # dataclass
├── DistillationUnitSpecification         # dataclass
├── ErrorBudgetPartition                  # dataclass
└── EstimatorConstraints                  # dataclass
```

## `qdk.openqasm`

OpenQASM 3.0 compilation and execution.

```
qdk.openqasm
├── run()                                 # function — run OpenQASM program
├── compile()                             # function — compile to QIR
├── circuit()                             # function — generate circuit diagram
├── estimate()                            # function — resource estimation (deprecated)
├── import_openqasm()                     # function — import OpenQASM into interpreter
├── ProgramType                           # enum (File, Operation, Fragments)
├── OutputSemantics                       # enum (Qiskit, OpenQasm, ResourceEstimation)
└── QasmError                             # exception
```

## `qdk.qiskit`

Qiskit interop — backends, jobs, and resource estimation.

```
qdk.qiskit
├── QSharpBackend                         # class — Qiskit BackendV2 for Q# simulation
├── NeutralAtomBackend                    # class — Qiskit BackendV2 for neutral atom
├── ResourceEstimatorBackend              # class — Qiskit BackendV2 for resource estimation
├── QirTarget                             # class — Qiskit Target helper
├── estimate()                            # function — estimate a QuantumCircuit
├── EstimatorParams                       # class (re-exported from qdk.estimator)
├── EstimatorResult                       # class (re-exported from qdk.estimator)
├── QasmError                             # exception
│
│   # Jobs
├── QsJob                                 # class — abstract job base
├── QsSimJob                              # class — simulation job
├── ReJob                                 # class — resource estimation job
├── QsJobSet                              # class — multi-circuit job set
│
│   # Submodules
├── backends/                             # backend implementations
│   ├── Compilation, Errors
│   ├── NeutralAtomTarget
│   └── RemoveDelays (pass)
├── jobs/                                 # job implementations
├── execution/                            # execution helpers
└── passes/                               # transpiler passes
```

## `qdk.cirq`

Cirq interop — neutral atom sampler.

```
qdk.cirq
├── NeutralAtomSampler                    # class — cirq.Sampler for neutral atom simulation
│   └── run_sweep(program, params, repetitions)
└── NeutralAtomCirqResult                 # class — cirq.ResultDict with raw shot access
```

## `qdk.qre`

Quantum Resource Estimation v3.

```
qdk.qre
│
│   # Top-level functions
├── estimate()                            # function — run full estimation pipeline
├── constraint()                          # function — create an ISA constraint
├── plot_estimates()                      # function — visualize estimation results
├── instruction_name()                    # function — ID → name lookup
├── property_name()                       # function — ID → name lookup
├── property_name_to_key()                # function — name → ID lookup
│
│   # Function builders (for ISA properties)
├── block_linear_function()               # function
├── constant_function()                   # function
├── linear_function()                     # function
├── generic_function()                    # function
│
│   # Core types (from Rust)
├── ISA                                   # class — instruction set architecture
├── ISARequirements                       # class — ISA constraint set
├── Instruction                           # class — single instruction definition
├── InstructionFrontier                   # class — Pareto frontier of instructions
├── Constraint                            # class — ISA constraint
├── ConstraintBound                       # class — comparison bound (lt, le, eq, gt, ge)
├── EstimationResult                      # class — single estimation result
├── FactoryResult                         # class — factory estimation result
├── Trace                                 # class — algorithm execution trace
├── Block                                 # class — trace block
│
│   # Python framework types
├── Application                           # abstract class — algorithm definition
├── Architecture                          # abstract class — hardware model
├── ISAContext                            # class — enumeration context
├── ISATransform                          # abstract class — ISA transformation
├── ISAQuery                              # abstract class — ISA enumeration query
├── ISARefNode                            # class — enumeration leaf node
├── ISA_ROOT                              # constant — root enumeration node
├── TraceQuery                            # class — trace enumeration query
├── TraceTransform                        # abstract class — trace transformation
├── PSSPC                                 # dataclass — Pauli-based rotation synthesis
├── LatticeSurgery                        # dataclass — lattice surgery transform
├── Encoding                              # IntEnum (PHYSICAL=0, LOGICAL=1)
├── LOGICAL                               # constant
├── PHYSICAL                              # constant
├── InstructionSource                     # class — instruction provenance
│
│   # Result types
├── EstimationTable                       # class — tabular estimation results
├── EstimationTableEntry                  # frozen dataclass — single result row
├── EstimationTableColumn                 # frozen dataclass — column definition
│
│   # Submodules
├── instruction_ids                       # module — integer constants (PAULI_X, H, CNOT, T, ...)
├── property_keys                         # module — integer constants (DISTANCE, RUNTIME, ...)
│
├── application/                          # application definitions
│   ├── CirqApplication                   # dataclass
│   ├── QIRApplication                    # dataclass
│   ├── QSharpApplication                 # dataclass
│   └── OpenQASMApplication               # dataclass
│
├── interop/                              # trace builders
│   ├── trace_from_cirq()                 # function
│   ├── trace_from_entry_expr()           # function
│   ├── trace_from_entry_expr_cached()    # function
│   ├── trace_from_qir()                  # function
│   ├── PushBlock, PopBlock               # classes — Cirq custom gates
│   ├── QubitType, TypedQubit             # classes — typed qubits
│   ├── PeakUsageGreedyQubitManager       # class — qubit manager
│   ├── ReadFromMemoryGate                # class
│   ├── WriteToMemoryGate                 # class
│   ├── write_to_memory()                 # function
│   ├── read_from_memory()                # function
│   └── assert_qubits_type()              # function
│
└── models/                               # hardware models
    ├── GateBased                          # class — gate-based qubit architecture
    ├── Majorana                           # class — Majorana qubit architecture
    ├── SurfaceCode                        # class — surface code QEC
    ├── ThreeAux                           # class — 3-auxiliary QEC
    ├── OneDimensionalYokedSurfaceCode    # class — yoked surface code (1D)
    ├── TwoDimensionalYokedSurfaceCode    # class — yoked surface code (2D)
    ├── Litinski19Factory                  # class — magic state factory
    ├── MagicUpToClifford                  # class — factory utility
    └── RoundBasedFactory                  # class — round-based factory
```

## `qdk.applications`

Domain-specific quantum applications.

```
qdk.applications
└── magnets/                              # quantum magnetism
    │
    │   # Geometry
    ├── CompleteBipartiteGraph             # class
    ├── CompleteGraph                       # class
    ├── Chain1D                            # class
    ├── Ring1D                             # class
    ├── Patch2D                            # class
    ├── Torus2D                            # class
    │
    │   # Models
    ├── Model                              # class — base model
    ├── IsingModel                         # class
    ├── HeisenbergModel                    # class
    │
    │   # Trotter
    ├── TrotterStep                        # class
    ├── TrotterExpansion                   # class
    ├── strang_splitting()                 # function
    ├── suzuki_recursion()                 # function
    ├── yoshida_recursion()                # function
    ├── fourth_order_trotter_suzuki()      # function
    │
    │   # Utilities
    ├── Hyperedge                           # class
    ├── Hypergraph                          # class
    ├── HypergraphEdgeColoring             # class
    ├── Pauli                              # class
    ├── PauliString                        # class
    ├── PauliX                             # constant
    ├── PauliY                             # constant
    └── PauliZ                             # constant
```

## `qdk.azure`

Azure Quantum integration (requires `pip install qdk[azure]`).
Re-exports `azure.quantum.*`.

## `qdk.widgets`

Jupyter widgets (requires `pip install qdk[jupyter]`).
Re-exports `qsharp_widgets.*`.

## `qdk.code`

Dynamic namespace populated at runtime by the Q# interpreter.
Q# callables and types become attributes (e.g., `qdk.code.Microsoft.Quantum.*`).
