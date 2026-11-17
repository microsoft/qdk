# QSPNLib

QSPNLib (Quantum Service Protocol Native Library) allows native programs in
various languages to send quantum instructions using QSP.

It allows you to write code that drives QuantumBackend by calling C-style functions.

All exposed functions use the `qdk_` prefix (for example, `qdk_x` applies an X gate).

Typical usage:

- Write code for a quantum application in a language (e.g., C, C++, or Fortran)
  that calls the corresponding function exposed by the library for each quantum
  instruction (gate or measurement).
- By default, instructions are streamed to stdout and results are read from stdin.
  Optionally call `qdk_init()` to select a simulator or local profiler instead.
- When using a profiler, call `qdk_save_profile()` at the end to save the profile to a
  JSON file (which will contain a resource estimate based on profiling).
- Compile your code, statically linking it against the library provided by this package.
- Execute the resulting binary.

At runtime, function calls will be turned into calls to the QuantumBackend interface.

See [qspnlib.h](include/qspnlib.h) for the full list of exposed functions.

## Supported backends in qdk_init()

The following backends can be selected by name:

- "stdio" - streams instructions to stdout and reads results from stdin. This is
  also the default when `qdk_init()` has not been called. On Unix, setting both
  `QDK_INSTRUCTION_FD` and `QDK_RESULT_FD` selects dedicated protocol descriptors
  instead. Setting just one, or an invalid descriptor, is an error.
- "sparse_sim" - a sparse simulator.
- "counting_profiler" - CountingProfiler with default parameters.
- "temporal_profiler" - TemporalProfiler with default parameters, result-register support,
  and default instruction transpilers. Combined measurement/reset is profiled as separate
  measurement and reset operations.

The stdio backend uses the [QSP stream protocol](../qsp/README.md#interprocess-communication-protocol)
and can connect to a `StreamDriver` wrapping a simulator or profiler. Without
dedicated descriptors, reserve stdout for instructions and stdin for results;
print application diagnostics to stderr.
Instructions are buffered until a measurement/result read or the batch capacity
is reached. `qdk_save_profile()` in stdio mode only prints an informational message
to stderr: it does not flush, inspect the path, or write a file. Profiling and
saving must be handled by the profiler receiving the instruction stream.

### Compiling and profiling C++ from Python

In a QDK source checkout on Linux or macOS, build the release interop archive
once with `cargo build --release -p qspnlib`, then use:

```python
from qdk.qre.application import QspApplication
from qdk.qre.profiler import QREProfiler

result = QREProfiler.counting().profile(QspApplication("my_program.cpp"))
print(result.totals.calls_per_operation)
```

`QspApplication.compile()` invokes `cc` (or `CC`) for a `.c` main source using C11,
and `c++` (or `CXX`) for other sources using C++17. It links the
release archive, and returns the absolute path of an executable named
`<source-stem>_qsp` beside the source. Profiling compiles and runs it, connecting
two dedicated inherited pipes through the environment variables above. Neither
process's stdin/stdout is redirected, so the C++ program can print normal logs
to stdout. Compiler failures, nonzero program exits, and protocol errors raise
exceptions.

Pass `extra_sources=[...]` to compile additional translation units alongside the
main source, `args=[...]` for executable arguments, and `compiler_args=[...]` for
additional compiler/linker flags (including `-I`, `-D`, and `-l` options).
Source paths are resolved from the current working directory. Arguments are passed
as separate tokens, not split as shell commands. Extra compiler arguments follow
the interop archive and system libraries so additional libraries link correctly.

The program should call `qdk_` instructions directly, without `qdk_init()` or
`qdk_save_profile()`. Finish with a measurement/result read to submit buffered
gates. Running the executable outside the profiler, with neither environment
variable set, still uses stdin/stdout for the protocol.

`QspApplication.get_trace()` compiles and runs the same application through a
trace-building backend instead. It returns a fresh `Trace` containing the gates
executed by that run, suitable for resource-estimation transformations. Measurement
outcomes are random, not simulated quantum-state outcomes; conditional programs
record only the path taken. Tracing and profiling share the same dedicated-pipe
transport, subprocess cleanup, and error handling.

### Profiling an external program from Python

On Unix, `Profiler.profile_fds(instruction_fd, result_fd)` reads instruction
batches from one descriptor and writes replies to the other, until instruction
EOF. It duplicates the descriptors, leaves ownership of the originals with the
caller, and releases the GIL while serving.

For manual subprocess wiring, create two `os.pipe()` pairs and pass these ends
with `subprocess.Popen(pass_fds=..., env=...)`:

| Pipe | Child environment | Profiler argument |
| --- | --- | --- |
| Instructions | `QDK_INSTRUCTION_FD`: write end | `instruction_fd`: read end |
| Results | `QDK_RESULT_FD`: read end | `result_fd`: write end |

Close the parent's copies of the child ends after spawning, and close the
remaining ends on completion or failure so EOF can be observed. The C++ program
may use normal stdin/stdout/stderr independently of these pipes.
`Profiler.profile_stdio()` remains available for explicitly serving the protocol
on the profiler process's stdin/stdout instead.

End the workload with a measurement/result read so buffered instructions are
submitted, then let the program exit (or close its instruction pipe) to signal EOF. Trailing
instructions that never reach a batch boundary are discarded;
`qdk_save_profile()` does not flush them in stdio mode.

Coming soon: profilers will load configuration from a file. Meanwhile, if you need
to instantiate backend with custom parameters, define custom backend name (e.g.
"custom_profiler") and add implementation for it in `qdk_init()` in
[src/lib.rs](src/lib.rs).

## Supported languages

### C/C++

1. Install Rust/Cargo and a C or C++ compiler. LLVM development libraries are not required.
2. Include `qspnlib.h` in your source file. Add this package's `include`
   directory to your compiler's include path.
3. Optionally call `qdk_init(...)` to replace the default stdio backend before
   executing quantum instructions.
4. Call the instruction functions directly. Qubit and result IDs use `size_t`;
   rotation angles use `double` and are in radians. `qdk_mz()`, `qdk_mz_reset()`, and
   `qdk_read_result()` return `size_t`: 0 for zero, 1 for one, or 2 for a
   backend-reported error. `qdk_mz_store()` and `qdk_mz_store_reset()` store their
   outcomes in a result ID for later access through `qdk_read_result()`.
5. When using a profiler, call `qdk_save_profile("profile.json")` to save the
   accumulated profile.
6. Build the Rust static library with `cargo build --release -p qspnlib`,
   then compile and link your program with `target/release/libqspnlib.a`.
   On Linux, include the system libraries shown in the command below, after
   the Rust archive in the link command.

For example, [examples/ghz.c](examples/ghz.c) demonstrates preparing and measuring a GHZ
state, first using a simulator and then a profiler.

Build and run it with the following command:

```
cargo build --release -p qspnlib && \
cc -std=c11 -Wall -Wextra -Werror -Isource/qspnlib/include source/qspnlib/examples/ghz.c target/release/libqspnlib.a -ldl -lpthread -lm -o target/release/ghz && \
./target/release/ghz
```

### Fortran

Use [include/qspnlib.fi](include/qspnlib.fi) for Fortran interfaces to
all functions in the C header. These interfaces use `bind(C)` and
`iso_c_binding`; no additional Rust library or wrapper is required.

1. Install a Fortran compiler supporting Fortran 2008, such as `gfortran`, plus
   the Rust prerequisites listed in the C/C++ section.
2. Include `qspnlib.fi` in the specification part of your free-form source,
   after `implicit none` and before executable statements. Add this package's
   `include` directory to the compiler's include path.
3. Use `integer(c_size_t)` for qubit IDs, result IDs, and measurement results,
   and `real(c_double)` for rotation angles in radians. Pass nonnegative IDs
   representable by `integer(c_size_t)`. The interfaces use `value` for numeric
   arguments to match the Rust C ABI; use kind-qualified literals such as
   `0_c_size_t` and `0.5_c_double`.
4. Pass NUL-terminated UTF-8 strings to `qdk_init` and `qdk_save_profile`, for example
   `c_char_"sparse_sim" // c_null_char`. Fortran does not append the terminator
   automatically.
5. Use `call` for instructions without results. Assign the return value of
   `qdk_mz`, `qdk_mz_reset`, or `qdk_read_result` to an `integer(c_size_t)` variable.
   The result codes are the same as in C: 0, 1, or 2 for a backend error.
6. From the repository root, build the Rust static library with
   `cargo build --release -p qspnlib`. This produces
   `target/release/libqspnlib.a`.
7. Compile and link your Fortran program against that archive using your
   Fortran compiler as the linker driver.

For example:

```fortran
program example
    use, intrinsic :: iso_c_binding, only: c_char, c_null_char, c_size_t
    implicit none
    include 'qspnlib.fi'
    integer(c_size_t) :: outcome

    call qdk_init(c_char_"sparse_sim" // c_null_char)
    call qdk_x(0_c_size_t)
    outcome = qdk_mz(0_c_size_t)
    print *, 'Measurement:', outcome
end program example
```

The example in [examples/ghz.f90](examples/ghz.f90) is equivalent to the C example above.
Build and run from the repository root on Linux:

```sh
cargo build --release -p qspnlib && \
gfortran -std=f2008 -Wall -Wextra -Werror -Isource/qspnlib/include source/qspnlib/examples/ghz.f90 target/release/libqspnlib.a -ldl -lpthread -lm -o target/release/ghz_fortran && \
./target/release/ghz_fortran
```

## More examples

### Shor's algorithm (C and Fortran)

[examples/shor.c](examples/shor.c) and [examples/shor.f90](examples/shor.f90)
port [the Q# Shor sample](../../samples/algorithms/Shor.qs) to the interop API.
The C example provides explicit self-test, factorization, profiling, and random-shot modes.
Profiling uses `counting_profiler`; its measurement results are bogus, so the
factorization result is ignored rather than retried. The Fortran example
uses `sparse_sim` and defaults to factoring **187 = 11 \* 17**.

The C example requires the GMP development headers and library (on Ubuntu/Debian,
install `libgmp-dev`). Its classical arithmetic supports numbers up to 2048 bits;
the Fortran example retains its existing fixed-width implementation.

Build both examples from the repository root on Linux:

```sh
cargo build --release -p qspnlib
cc -std=c11 -O2 -Wall -Wextra -Werror \
    -Isource/qspnlib/include source/qspnlib/examples/shor.c \
    source/qspnlib/examples/arithmetic.c \
    target/release/libqspnlib.a -lgmp -ldl -lpthread -lm \
    -o target/release/shor_c
gfortran -std=f2008 -O2 -Wall -Wextra -Werror \
    -Isource/qspnlib/include source/qspnlib/examples/shor.f90 \
    target/release/libqspnlib.a -ldl -lpthread -lm \
    -o target/release/shor_fortran
```

The C CLI accepts exactly one of these modes:

- `--self-test`: run arithmetic regression tests on `sparse_sim`, with no other arguments.
- `--factor <number>`: factor the required decimal integer in `[4, 2^2048)` on `sparse_sim`.
- `--profile <bitlen>`: profile one shot using an even bit length from 4 to 2048.
- `--run-random <bitlen>`: run the same random-semiprime shot with the same bit-length
  constraints, without calling `qdk_init()` or saving a profile. Instructions stream
  to an external backend by default; only elapsed shot time is reported locally.
  Use `QDK_INSTRUCTION_FD` and `QDK_RESULT_FD` to keep the example's stdout logs
  separate from the protocol. The shot's factorization result is not retried.

Supporting large integers does not make generic 2048-bit quantum simulation
practical. Use profiling mode for large circuits, and expect very long runtimes:
the current gate-emitting implementation scales roughly cubically with bit length.

In profiling mode, each random prime has half the requested bit length;
their product can be one bit shorter than the requested length. Output is
written to `source/profiling/profile_viewer/profiles/shor_c_<bitlen>.json`; the parent
directory must already exist. Prime generation uses GMP probable-prime testing
with 25 repetitions and a time-seeded random generator intended for benchmarking,
not cryptography. The example reads the saved operation count and
reports elapsed wall-clock time and nanoseconds per counted operation. Timing
covers only `shors_shot`, excluding prime generation, backend initialization,
and saving (including the final buffer flush).

```sh
./target/release/shor_c --profile 20
```

The [Shor native profiling notebook](../profiling/demo/ShorsNativeProfile.ipynb) uses
`QspApplication` with `arithmetic.c` as an extra source and `-lgmp` as a compiler
argument. It compares external streaming (`--run-random 100`) with an in-process
counting profile (`--profile 100`) loaded from JSON.

The Fortran example accepts the number directly and defaults to 187:

```sh
./target/release/shor_c --factor 187
./target/release/shor_fortran
./target/release/shor_c --factor 15
./target/release/shor_fortran 21
```

Both examples also provide reversible-arithmetic regression tests:

```sh
./target/release/shor_c --self-test
./target/release/shor_fortran --self-test
```

These exhaustively check controlled modular addition and coprime multiplication
on basis states for moduli 3 through 7, including disabled controls and cleanup
of all scratch qubits. They also check coherent compute/uncompute with controls
in superposition, continued fractions, and period-to-factor recovery.
The C self-test additionally checks 64-, 65-, 2048-, and 2049-bit addition,
comparison, controls, wraparound, and workspace cleanup on basis states, as well
as large modular powers, 4097-bit phase postprocessing, and 1024-bit prime generation.
