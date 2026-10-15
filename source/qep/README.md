## Universal interface between quantum layers of abstractions

We define a universal interface between 2 components of a quantum system.

### Driver and backend.
* These 2 components are called "Driver" and "Backend".
* Driver sends instructions to Backend.
* Backend receives instructions, executes them and sends results to Driver.
* These names are only in context of this interface, the same component can be Driver
    relative to one component, and backend for other.
* Examples:
    * QIR runner is a Driver which performs all classical instructions and sends 
        instructions from QIR ISA to Backend.
    * A simulator can implement Backend.
    * A profiler can implement Backend. Profiler is similar to simulator, but it doesn't
       maintain wavefunction. Instead, it tracks calls and after execution reports
       resource estimates for program (runtime, physical qubits).
    * Qodec is a Driver relative to some Backend (which can be real quantum computer, profiler or simulator).
    * Qodec is a Backend relative to QIR runner.
    * Qodec can be composed of multiple qodecs connected via Driver-Backend interface.
    * Real quantum computer can implement Backend.
    * We might insert networking layer in place of Backend-Driver interface. For example:
       * We have a client on local machine. It implements Backend interface. 
       * We have QIR runner running on local machine, using the client as its Backend.
       * We have a real quantum engine running on remote machine, implementing Backend Interface.
       * We have a server running on remote machine, using real quantum engine as its Backend.
       * Server and client talk to each other over HTTP.
        
### Two streams.
Interface consists of two streams:
* Stream of `MessageIn` messages - this is stream of instructions, e.g. gates, measurements, resets.
* Stream of `MessageOut` messages - this is stream of measurement results.

`MessageIn` and `MessageOut` are Rust enums that are allowed to grow.

`MessageIn` is designed to represent instruction on any level of abstraction (e.g. logical or physical).
It might have:
 * Abstract gates not tied to any specific hardware, e.g. X(qubit_id).
 * Logical gates from specific ISA, e.g. QIR gates.
 * Physical gates specific to given hardware, e.g. CZ for Atom computing.

`MessageOut` is designed to represent measurement results but specifics can depend on what
exactly is frontend and backend.

Any backend used together with QIR Runner has a following contract:
  * For every QirInstruction::read_result operation it must write a single MeasurmentResult message containing single bit.
  * It must not write anything else.
  * It must write result immediately after receiving read_result instruction.

However, in general, for some Backends:
 * measurement results might contain other pieces of data (e.g. qubit_id or result_id),
 * results might come in batches or be allowed to be out of order,
 * results might have more than 2 outcomes (e.g. 0,1,Loss).
This is why we do not fix MessageOut schema, instead we make it enum which can have more
values added later.

The two streams are implemented as 2 channels, and there are 2 endpoints:
* `QuantumDriverEndpoint` consists of instruction sender (`Sender<MessageIn>`) and results receiver  (`Receiver<MessageOut>`)
* `QuantumBackendEndpoint` consists of instruction receiver (`Receiver<MessageIn>`) and results sender  (`Sender<MessageOut>`)

### Quantum Backend trait

Backend must implement the following trait:

```
trait QuantumBackend {
    async fn run(&mut self, endpoint: QuantumBackendEndpoint);
}
```

The function `run` must run a while loop that:
  * Processed received instructions until instructions channel is closed.
  * Writes results to the results channel.
   

### Implementing Quantum Driver

In rust, Quantum Driver will be anything that owns QuantumDriverEndpoint. 
We do not define a trait for that.


## QIR Runner

QIR Runner is a Driver.
 * It reads QIR from file, and executes it. 
 * It owns a Backend. 
 * It forwards quantum instructions from QIR to Backend.

Measurements are done in somewhat weird way, according to QIR spec:
 * There is a "measurement array" and it must be maintained by Backend. 
 * When QIR Runner sees measurements, it enqueues corresponding operation (m,mz or mreset) 
     and doesn't wait for it, as per QIR spec. Backend will perform measurement and write result to 
     specified index at measurement array.
 * When it sees QIR instruction that instructs it to read result from result register (read_result or *_record_output),
    it issues Read command containing result_id and blocks until it receives output. Then it uses output in 
    classical flow.
 * It is responsibility of Backend to perform measurement that writes to result_id before responding to ReadResult.
 * There is __quantum__rt__write_result operation that technically means that we send classical bit from Driver to Backend
    and Backend must store it at specified index in results array. It exists only for Stim simulator.
    Our QIR runner doesn't implement it, it will fail during execution when encounters this command.
    So write_result is not included in the enum QirInstruction.

"QIR ABI" refers to specific set of instructions which roughly corresponds to library/std/src/Intrinsic.qs
and for its exact definition see enum QirInstruction in this package.

## Quantum Execution Profiler

A Profiler is a Backend that models a specific QPU (Quantum processing unit/quantum engine).

Profiler can be implemented as:
 * Rust struct implementing `trait QuantumBackend`.
 * A library with any language that implements QIR ABI (or other ISA for physical profilers)
     that is then statically linked with QIR Runner, which will require a shim. That shim will be a rust implementation
     of `trait Backend` that implements instructions as function calls.
     (TODO: implement this shim and then just refer to it here)
 * A web server that implements QIR ABI (or other ISA for physical profilers) as HTTP endpoints
   (or one endpoint where instruction name and args are in request body). This again requires a shim
   similar to previous item. This shim will be a rust implementation of `trait Backend` which will be a client
   to that server that will implement instructions by sending web requests.

Profiler can be logical or physical:
 * Logical profiler that implements QIR ABI (or other logical ISA). This profiler will rely on hardcoded
    information on:
    * how long each gate takes
    * how many physical qubits are needed to encode each logical qubit
    * How many physical qubits are needed for factories
 * Physical profiler will implement ISA of specific real quantum hardware, which is typically very limited
   (e.g. CZ, MZ and few single-qubit gates for Atom Computing). It must be used together with a Qodec. 

### Implementing the Profiler

Profiler can be written:
 * By a human expert with knowledge about specific quantum computer who understand it physics etc.
   * In theory, we can ask vendors to supply such a profiler for each machine model.
 * Or by an agent, given a paper with detailed enough description of specific hardware.
   * For good results, this agent will also require good documentation of the framework, 
       guidelines for implementing profilers and several high quality examples.


### Implementing measurements
We cannot correctly predict probability of measurement outcomes without keeping track of 
wavefunction which takes in general O(2^n) memory. We can do this only for very small 
number of qubits, and resource estimation is typically done when number of qubits is so large
that simulation is impossible.

So, we suggest implementing a simple heuristic:
* Every qubit is in state ZERO, ONE or ENTANGLED.
* Initially or after reset it is in state ZERO.
* Applying X moves it to ONE.
* Applying any gate moves all qubits it applies to, to ENTANGLED.
* Measurement on ZERO or ONE returns result equal to state and doesn't change qubit.
* Measurement in ENTANGLED state will with probability 50% move qubit to state ZERO and
    with probability 50% to state ONE, and move qubit to corresponding state (ZERO or ONE).

This strategy should be close to truth when 
* We only do measurements in the end.
* Only measurements inside circuit are for measurement-based uncompute.

There are cases when this strategy will significantly impact estimation.
* For example, if we have repeat-until-success loop where success is defined by 
measuring single qubit in state 0, and actual success rate is e.g. 1%. Our model will get
success rate 50% and underestimate time by factor of 50x.
* Such cases should not be estimated with this profiler or we should use different strategies.


There may be other ways to do it:
* Always return 0 or 1 w.p. 50% from read_result, don't do any measurements at all.
* Do Clifford simulation and give up when encountering non-Clifford gate.
* Maintain connected components and simulate state only within connected components, and
    replace cross-component gates with classical approximations (in style of Semiclassical QFT).
* Annotate measurement with expected probability of outcome |1>. This probability can be sent to backend
    by instruction "set_probability_of_next_measurement".

Anyway, all these are implementation details of the Profiler. While we will provide some
reusable modules to implement "reduced simulation" in Profilers, implementers are free to do
it any way they want.



### Simple profiler: SequentialProfiler

We will implement a simple generic profiler called SequentialProfiler, based on following assumptions:
* All instructions are executed sequentially (no parallelism).
* Each instruction takes fixed time which only depends on instruction types.
* Physical qubit usage is equal to logical qubits usage times some constant.

In implementation this profiler will maintain call counter for every instruction.
In the end total runtime will be computed as `sum (gate_time[i]*gate_calls[i])`.

It's not doing much more than logical counter (`counts.rs`), but it's a starting point 
and a proof-of-concept for the whole QEP project.


### Advanced profiler: event queue based profiler.

This section is WIP, but on high level:
* There is event queue, event loop, and a clock.
  * Queue is a priority queue keyed by (time, instruction_id)
  * `instruction_id` is a counter needed so we process instructions coming at the same time in correct order.
* Every incoming instruction enqueues event "start gate".
* Event loop will:
  * When sees "start gate", enqueue "end gate" at "now + gate time", if that qubit is free.
  * If qubit is busy, put this event in another queue of events blocked by this qubit.
* This model can also model how T-factories generates T states at fixed output, and T gates
    might be blocked on T factories (then T factory is a bottleneck), or T gates might be too slow
    then T factories must be slowed down. 


### Resource estimation workflow.

On a high level, resource estimation will look like this:

```
    let qir_path = "program.ll";
    let mut profiler = Profiler::default();
    let mut qir_runner = QirRunner::default();
    qir_runner.run(qir_path, &mut backend).await;
    println!(profiler.get_results());
```

That is:
* Application or profile is compiled into QIR
  * We can support any other format, and we only will have to swap QirRunner with 
      different interpreter that implements the above described interface.
* We instantiate Profiler.
* We run QIR Runner with profiler as backend.
* Profiler keeps track of resources in its internal state.
* After successfully running the program, profiler object contains estimate we need.

### Potential Python API

For Python users we might expose it as follows:
* `Application` class, which is a wrapper around Q#/QASM:
  * Support Cirq/Qiskit via QASM.
  * Internally, it has `to_qir` which writes QIR to file.
* `QPUModel` (or simply `QPU`) which will be a wrapper around Profiler (impl Backend).

Then main resource estimation function can look like:
```
def estimate(app:Application, model: QPUModel) -> EstimationResult:
    qir_file_path = app.to_qir()
    _estimate_internal(qir_file_path, model.profiler)
    return EstimationResult.from_profile(model.profiler.profile())
```