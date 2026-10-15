# QEP profiling performance analysis

## Summary

This analysis covers the synchronous batched QIR runner. The runner now buffers
instructions and calls `QuantumBackend::execute_batch` directly. It flushes at
`read_result`, at the configured size limit, or when execution ends. The
default maximum batch size is 10,000 instructions.

The redesign removes Tokio tasks, channels, and per-instruction message
traffic. On the Shor fixture, end-to-end profiling improved from approximately
4.47-4.51 seconds to 96-100 milliseconds: about a 46x speedup. Native execution
improved from approximately 4.44-4.48 seconds to 70-72 milliseconds: about a
63x speedup.

The remaining costs differ by workload:

- Shor is dominated by executing JIT callbacks and by approximately 41,000
  synchronous result barriers.
- The current 1,000-bit adder is small enough that fixed parsing and JIT setup
  dominate. Its quantum operations fit in one backend batch.

## Methodology

### Workloads

The measurements use the two tests in `source/qep/src/tests.rs`.

#### 1,000-bit Q# adder

`profiles_adder` compiles this program with `compile_qsharp_to_qir`:

```qsharp
let n = 1000;
use xs = Qubit[n];
use ys = Qubit[n];
use sum = Qubit[n];
Std.Arithmetic.RippleCarryCGAddLE(xs, ys, sum);
```

Q# compilation happens before the profiling timer starts. With the current
compiler, the generated QIR is 98,626 bytes and executes 5,996 quantum
operations. It executes no `read_result` instructions, so all operations fit
in one backend batch with the default maximum size.

#### Shor QIR

`profiles_shors_16_qir` loads `source/qep/tmp/shors_16.ll`. The fixture is
160,205 bytes and executes approximately 540,000 operations. The exact count
varies because the counting profiler returns random measurement results and
the program contains adaptive control flow.

One measured run executed 539,379 operations, including 41,452
`read_result` instructions. Each `read_result` ends a batch and requires an
immediate backend response. The average batch therefore contains roughly 13
instructions; increasing the maximum batch size does not remove these semantic
barriers.

### Profiler configuration

Both tests use `SequentialCountingProfiler` with:

- One physical qubit per logical qubit.
- No configured gate times.
- The default maximum batch size of 10,000.

The timer starts immediately before `profile_qir` and stops immediately after
it returns. The operation count is the sum of the values in
`ProfilerResult.calls_per_operation`.

The tests were run with:

```text
cargo test -p qep tests::profiles_adder -- --nocapture
cargo test -p qep tests::profiles_shors_16_qir -- --nocapture
```

These are development-profile tests, not statistical benchmarks. Absolute
timings depend on machine load.

### Phase instrumentation

Temporary `Instant` measurements were added around:

1. QIR parsing.
2. Initial LLVM verification.
3. Supported-subset validation.
4. Execution-state preparation.
5. LLVM JIT creation.
6. Callback binding.
7. Final LLVM verification.
8. Entry-point compilation.
9. Entry-point execution.

Each workload was measured twice. Extra temporary output also recorded the QIR
size and `read_result` count. The instrumentation was removed after collecting
the results.

## Results

### 1,000-bit Q# adder

The adder executed 5,996 operations. End-to-end profiling took 8.2-11.2 ms in
the two isolated runs, or 1,370-1,872 ns per operation. A subsequent run took
6.1 ms, or 1,009 ns per operation, illustrating the variability of these
short development-profile measurements.

| Phase | Sample 1 | Sample 2 |
|---|---:|---:|
| Parse QIR | 2.24 ms | 2.74 ms |
| Initial verification | 0.29 ms | 0.28 ms |
| Validate QIR | 0.04 ms | 0.04 ms |
| Prepare execution state | 0.006 ms | 0.006 ms |
| Create JIT | 0.77 ms | 0.50 ms |
| Bind callbacks | 0.11 ms | 0.12 ms |
| Final verification | 0.25 ms | 0.47 ms |
| Compile entry point | 2.62 ms | 3.00 ms |
| Execute entry point | 0.51 ms | 0.35 ms |
| Total `profile_qir` | 11.23 ms | 8.22 ms |

Native execution costs only 59-86 ns per operation. The backend receives one
batch, so backend call overhead is negligible. Parsing, JIT construction, and
entry-point compilation dominate.

The previous report measured a 1.64 MB adder module with 15,986 operations,
whereas the current checked-out compiler emits a 98.6 KB module with 5,996
operations for the test source. Because the generated artifacts differ, the
old and new adder totals cannot be used to isolate the gain from batching.

### Shor QIR

The two isolated runs executed 540,826 and 540,170 operations. End-to-end
profiling took 96.0-97.8 ms, or 177-181 ns per operation.

| Phase | Sample 1 | Sample 2 |
|---|---:|---:|
| Parse QIR | 3.20 ms | 3.00 ms |
| Initial verification | 0.43 ms | 0.40 ms |
| Validate QIR | 0.22 ms | 0.21 ms |
| Prepare execution state | 0.03 ms | 0.02 ms |
| Create JIT | 0.33 ms | 0.25 ms |
| Bind callbacks | 0.17 ms | 0.11 ms |
| Final verification | 0.42 ms | 0.34 ms |
| Compile entry point | 20.12 ms | 21.92 ms |
| Execute entry point | 69.69 ms | 70.59 ms |
| Total `profile_qir` | 95.95 ms | 97.83 ms |

Native execution remains the largest phase at approximately 72% of total
time. Entry-point compilation is approximately 22%. All other setup phases
together are under 5 ms.

### Before-and-after comparison for Shor

| Metric | Channel runner | Batched runner | Improvement |
|---|---:|---:|---:|
| End-to-end time | 4.47-4.51 s | 96.0-97.8 ms | about 46x |
| End-to-end ns/op | 8,279-8,369 | 177-181 | about 46x |
| Native execution | 4.44-4.48 s | 69.7-70.6 ms | about 63x |
| Execution-only ns/op | about 8,250 | about 130 | about 63x |

## Findings

### Channel traffic was the main Shor bottleneck

The old runner sent every instruction through a bounded Tokio channel to an
asynchronous backend task. A result read also required a response on a second
channel. Task wakeups, synchronization, and message traffic accounted for most
of the previous execution time.

The new runner appends ordinary instructions to a reusable `Vec` and invokes
the backend directly only at a flush boundary. This reduced Shor's
execution-only cost from approximately 8,250 ns to approximately 130 ns per
instruction.

### Batching cannot cross adaptive result barriers

Shor performs approximately 41,000 `read_result` operations. The JIT program
may use each returned bit immediately to choose control flow, so the runner
must flush and obtain the result synchronously. This creates tens of thousands
of backend calls despite the 10,000-instruction capacity.

Representing a batch result as multiple bits would help only when reads can be
proven independent of intervening control flow. It would not automatically
batch adaptive reads.

### A large maximum batch primarily helps non-adaptive programs

The adder has no result reads and its 5,996 operations fit in one batch.
Changing the limit above 5,996 cannot improve this workload. A lower limit
would add backend calls but would still avoid per-instruction channel traffic.

For Shor, result barriers determine nearly all batch boundaries, so increasing
the limit above the current default has little expected effect.

### Callback dispatch now dominates dynamic execution

Every dynamic operation still:

1. Calls a native wrapper generated for the QIR intrinsic.
2. Recovers and validates the execution-state pointer.
3. Enters `catch_unwind`.
4. Validates qubit and result IDs.
5. Constructs and appends a `QirInstruction`.

This work is now inexpensive compared with the old channel path, but it is
performed approximately 540,000 times for Shor and therefore dominates its
70 ms execution phase.

### The counting profiler remains inexpensive

For each instruction in a batch, `SequentialCountingProfiler` maps the
instruction variant to an array index, updates the largest qubit ID, and
increments a counter. It generates a random Boolean only for `read_result`.
The final result conversion iterates over the fixed set of supported
instruction kinds.

## Recommendations

The recommendations are ordered by expected gain for the measured workloads.

### 1. Cache prepared and JIT-compiled QIR

Add a prepared-program API that parses, validates, binds, and compiles a module
once, then executes it with multiple backend instances.

This could remove most of the 20-22 ms compilation cost from repeated Shor
runs and most of the 2-3 ms entry compilation plus setup cost from repeated
adder runs. The current callback wrappers embed an execution-state pointer, so
reuse requires a per-execution state lookup rather than a pointer fixed during
binding.

Expected gain:

- Shor: approximately 20-25% end-to-end after the first execution.
- Current adder: approximately 40-70%, depending on whether parsing and other
  setup are cached as well.

### 2. Reduce per-instruction callback overhead

Investigate generating thinner wrappers, moving checks out of hot callbacks
when they can be established statically, and reducing repeated panic-boundary
work without weakening error reporting. Measure each change because the
current path is already around 130 ns per operation for Shor.

This targets the 70 ms Shor execution phase and therefore has the largest
remaining opportunity for one-shot, execution-heavy programs.

### 3. Add a specialized in-process profiling path if needed

If profiling throughput is more important than maintaining one universal
execution path, the runner could bind callbacks that update counting state
directly instead of materializing `QirInstruction` values in a batch. This
would avoid buffer writes and batch iteration, but it duplicates backend
semantics and should be considered only after measuring callback-level
optimizations.

This can improve Shor-like profiling but provides little benefit to the
current adder, whose execution phase is already below 1 ms.

### 4. Coalesce only demonstrably non-adaptive result reads

A future protocol could return multiple result bits for batches where the
runner proves that earlier values are not consumed before later reads.
Output-recording sequences may offer such cases. Adaptive branches cannot be
coalesced safely.

The upper bound is limited because only part of Shor's approximately 41,000
result barriers will be independent. This adds protocol and bookkeeping
complexity, so it should follow profiling of the remaining barrier cost.

### 5. Evaluate LLVM optimization and code-generation choices

Higher JIT optimization might reduce callback and loop overhead for Shor but
will increase compilation time. Benchmark total time rather than execution
alone. For large generated programs, retaining loops and reusable functions
can reduce both QIR size and compilation time.

### 6. Establish repeatable release-mode benchmarks

Keep the functional tests, but add benchmarks that separately report:

- Parsing, validation, JIT compilation, and execution.
- Backend batch count and average batch length.
- Total and execution-only ns/op.
- Development and release Rust builds.
- Cold and prepared/cached execution.
- Straight-line, loop-heavy, non-adaptive, and measurement-heavy QIR.

This will distinguish compiler-output changes from runner improvements and
make regressions easier to attribute.
