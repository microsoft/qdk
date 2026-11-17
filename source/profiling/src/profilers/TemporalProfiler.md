# Temporal profiler

`TemporalProfiler` runs against a hardware model. The settings and assumptions
below are those of `DemoModel1`. For the general profiling workflow and Python
API, see the [profiling README](../../README.md).

## Scheduler

[TemporalProfiler](temporal_profiler.rs) maintains the simulated clock,
dependencies between operations, available resources and a queue of completion
events. The hardware model decides which native operations to schedule and what
happens when they start and finish.

The [QpuModel](../models.rs) callbacks are:

- `on_start`: initialize the model and schedule initial work.
- `on_instruction_received`: convert a QSP instruction into native operations,
  with targets, resource requirements, durations and result requirements.
- `on_operation_started`: assign available resources and update the operation
  before it is logged.
- `on_operation_finished`: produce a result, publish resources or schedule
  follow-up work.
- `get_total_counts`: compute resource estimates from the current state.

`TemporalProfilerState::schedule_operation()` automatically orders operations
sharing a target functional unit. Independent operations can run concurrently
when their resources are available. Publishing a resource through
`add_resource()` wakes waiting operations. Completion events are ordered by
timestamp; ties are processed FIFO.

Factory rounds are scheduled in the same way as gates. For example, a T
injection can wait for a factory to publish a T state, consume that state when
the injection starts, and start the next factory round when injection finishes.

## Execution boundaries

For each batch, the profiler schedules instructions and processes completion
events until all requested measurement results are available. Measurements can
finish out of order; the profiler returns their results in submission order.
Dependencies and resource state persist across batches.

A batch without results, including an empty batch, can start ready operations.
It does not process completion events or advance simulated time. Neither the
end of program execution nor `get_result()` drains pending work. Trailing
result-free work may remain unfinished, so the reported runtime covers only
work processed so far.

## DemoModel1

[DemoModel1](../models/demo_model1.rs) describes hypothetical hardware. Its
numbers are demo assumptions and are not calibrated to a real machine.
Its functional-unit implementations are [LogicalQubit](../models/qubits.rs) and
[TFactory](../models/factories.rs).

### Native instructions and timings

The model accepts `cx`, `h`, `s`, `sadj`, `x`, `y`, `z`, `t`, `tadj`, `mz` and
`reset`. T and T-adjoint become native `inject_t` and `inject_tadj` operations.
Other instructions need adapters before reaching the model.

`rx`, `ry`, `rxx`, `ryy`, `rzz` and `and_adj` are not decomposed by the current
transpilers and are not supported by the model. Programs using the temporal
profiler therefore cannot use these gates.

TODO: add transpilers for these gates.

The timings are:

- Clifford gates use `clifford_gate_time_nanos`.
- Measurement and reset each use `measurment_time_nanos`.
- T-state injection takes twice the Clifford duration plus the measurement
  duration.
- A factory round takes 11 logical time steps.

The physical syndrome cycle is 1,000 ns. One logical time step is $d$ syndrome
cycles, where $d$ is the code distance. Rust's default model uses distance 9,
ten factories, a three-step Clifford duration of 27,000 ns, and measurement and
reset durations of 1,000 ns. Injection therefore takes 55,000 ns, and a factory
round takes 99,000 ns.

Python requires both gate timing arguments explicitly. For example, setting
the Clifford duration to 5,000 ns makes injection take 11,000 ns. Explicit gate
timings do not automatically change with code distance. Factory duration still
depends on distance, independently of those timing overrides.

### Logical and physical qubits

The compute-qubit register grows to include the largest submitted qubit ID.
`totals.logical_compute_qubits` is the size of that register, including any gaps
in the IDs. Factories contribute their own logical qubits:

- Each factory uses five logical patches.
- `totals.logical_qubits` includes compute qubits and factory patches.
- The trace viewer uses the compute count for qubit lanes and shows factories
  on separate lanes.

Each logical patch uses a rotated surface code with $d^2$ data qubits and
$d^2 - 1$ syndrome-extraction ancillas. This is the same patch-size formula as
QRE's [SurfaceCode model](../../../qdk_package/qdk/qre/models/qec/_surface_code.py).
Compute qubits and factories currently use the same distance:

$$
N_{\mathrm{physical}} =
(N_{\mathrm{compute}} + 5N_{\mathrm{factories}})(2d^2 - 1).
$$

For example, a distance-5 patch needs 49 physical qubits, and a factory needs
245. The estimate excludes routing space, lattice-surgery workspace, memory
buffers and hardware spares. The caller supplies the distance; the model does
not choose it from a target failure probability.

### Factories

Each factory starts a distillation round at startup. When a round finishes:

- If it fails, the factory immediately starts another attempt.
- If it succeeds, the factory holds one ready T state and pauses.
- A T or T-adjoint injection reserves a ready factory. If none is ready, the
  injection waits. Ready factories are selected FIFO.
- The factory remains reserved during injection and starts its next round when
  injection finishes.

We include raw T-state production in the factory. A successful factory holds
its output until an injection consumes it. There is no separate output buffer,
so the factory cannot start another round while it waits.

The five-patch, 11-step single-round 15-to-1 layout follows
[Litinski, arXiv:1808.02892](https://arxiv.org/abs/1808.02892).
The acceptance and output-error coefficients come from
[arXiv:2211.07629, Appendix C / Table 6](https://arxiv.org/abs/2211.07629).
Using those coefficients with the compact layout is a demo approximation. They
are not calibrated to that layout.

### Error model and measurements

The [surface-code assumptions](../models/surface_code.rs) are:

- Physical error rate and raw T-state error $p_T$ are both $10^{-4}$.
- The threshold is $0.01$.
- The logical error budget per logical time step is:

$$
\epsilon(d) = 0.03 \left(\frac{10^{-4}}{0.01}\right)^{(d+1)/2}.
$$

This uses the logical-step convention of
[arXiv:2211.07629, Appendix B / Table 5](https://arxiv.org/abs/2211.07629).
The memory error rate per nanosecond is $\epsilon(d)/(1000d)$. It should not be
mixed with fits quoted per physical syndrome cycle.

For each distillation attempt:

$$
p_{\mathrm{discard}} = 15p_T + 356\epsilon(d),
$$

$$
p_{\mathrm{output}} = 35p_T^3 + 7.1\epsilon(d).
$$

The scheduler samples success and retries failures, rather than replacing rounds
with an average duration. Produced T states accrue memory error while waiting
and during injection. Injection adds that state's error to the compute qubit.

Compute qubits accumulate memory error from the time they are added to the model.
Measurement adds previously unreported memory and injected-state contributions
to `totals.error`. Repeated measurements do not charge the same interval twice;
the model is accounting for error, not physically correcting it. Reset starts
a fresh interval with no accumulated error.

Measurements return Zero or One with equal probability. The model does not
maintain a wavefunction or correlations between measurement results.

`totals.error` sums first-order fault contributions and can exceed one. It is not
an exact probability. We assume reset is perfect. The model does not simulate
error propagation, routing, decoder delays, rotation-synthesis error or intrinsic
algorithm success. Its physical error assumptions describe a future hardware
target; they are not claims about current hardware.

## Construction and adapters

Use `DemoModel1` to configure the hardware and `TemporalProfilerConfig` to
configure recording and randomness:

- `aggregation_window_nanos`: window size for time series, one second by default.
- `seed`: optional random seed, shared by distillation and measurement sampling.
- `record_trace`: whether to retain instruction and factory-round intervals,
  false by default.

Factory count, code distance and aggregation window must be positive. A fixed
seed reproduces the sampled outcomes for the same execution.

In Python:

```python
from qdk.qre import DemoModel1, TemporalProfiler, TemporalProfilerConfig

model = DemoModel1(
    clifford_gate_time_nanos=27_000,
    measurment_time_nanos=1_000,
    t_factories_count=10,
    ecc_distance=9,
)
config = TemporalProfilerConfig(seed=42, record_trace=True)
profiler = TemporalProfiler(model, config)
profiler.add_adapter("transpiler")
profiler.add_adapter("result_adapter")
```

Adding adapters in this order puts `ResultAdapter` outside `TranspilingAdapter`.
The result adapter maintains result registers across batches. It converts stored
measurements to immediate measurements and answers `read_result` locally.
The transpiling adapter decomposes composite gates and Rz rotations into native
gates. It also turns `mz_reset` into separate `mz` and `reset` operations.
Profiles describe the decomposed operations. They do not include the original
composite gates or result-register accesses. See
[QSP utilities](../../../qsp/README.md#utilities) for the adapter implementations.

In Rust, the equivalent arrangement with default hardware settings is:

```rust
ResultAdapter::new(TranspilingAdapter::with_defaults(
    make_profiler_for_demo_model(config),
))
```

Use `TemporalProfiler::new(model, config)` for an explicit hardware model.
Both native temporal-profiler variants install the adapters automatically.

## Recorded results

[EventRecorder](event_recorder.rs) collects event rates, factory utilization
and optional execution intervals. Native operations implement
`LoggableOperation` to supply metric names and trace parameters.

Operations are logged when they start, with their scheduled completion times.
Factory rounds appear as `round`; T-state injections appear as `inject_t` or
`inject_tadj`, with factory and qubit operands. A started round can have a trace
interval extending beyond the reported runtime because of the execution
boundaries described above.

`totals.total_operations_count` counts recorded events, including factory
events, not just input gate calls. The demo model leaves the per-operation
count and runtime maps empty. Use the event profiles and trace to inspect native
operations.

Event rates and factory utilization use separate `TumblingWindowAggregator`
instances. Only the event-rate aggregator contributes to the total event count.
Their time series are combined in the result.

`Factory utilization` is the sum of distillation durations divided by the
aggregation window duration. It gives the average number of factories in use,
in units of `factories`, not a percentage. It excludes waiting time and charges
each round to the window in which it starts, without splitting the duration
across windows. Rates use the full window duration even for the final partial
window.

The [profile viewer](../../profile_viewer/README.md) displays the recorded
series and intervals without recomputing these rates.
