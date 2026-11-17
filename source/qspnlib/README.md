# QSPNLib

QSPNLib (Quantum Service Protocol Native Library) is a library for writing native
quantum programs in languages such as C, C++ and Fortran. Programs call C-style
functions, which send quantum instructions to a QuantumBackend using QSP.

All exposed functions use the `qsp_` prefix (for example, `qsp_x` applies an X gate).

To use it:

- Write a quantum application that calls the library function corresponding to
  each quantum instruction (gate or measurement).
- By default, instructions are streamed to stdout and results are read from stdin (using
  the [QSP interprocess communication protocol](../qsp/README.md#interprocess-communication-protocol)).
  Optionally call `qsp_init()` to select a simulator or local profiler instead.
- When using a profiler, call `qsp_save_profile()` at the end to save the profile
  and resource estimate to a JSON file.
- Compile your code and statically link it against this library.
- Execute the resulting binary.

See [src/lib.rs](src/lib.rs) for list of exposed functions with documentation.
See also [this C header file](include/qspnlib.h) for the same list of functions in more
compact format.

## Supported backends in qsp_init()

The following backends can be selected by name:

- "stdio" - streams instructions to stdout and reads results from stdin. This is
  also the default when `qsp_init()` has not been called. On Unix, setting both
  `QDK_INSTRUCTION_FD` and `QDK_RESULT_FD` selects dedicated protocol descriptors
  instead. Setting just one, or an invalid descriptor, is an error.
- "sparse_sim" - a sparse simulator.
- "counting_profiler" - SequentialCountingProfiler with default parameters.
- "temporal_profiler" - TemporalProfiler with default parameters, without trace
  recording.
- "temporal_profiler_with_trace" - the same profiler with trace recording enabled.

Both temporal backends include result-register support and default instruction
transpilers. Combined measurement/reset is profiled as separate measurement and
reset operations. Programs using either backend must avoid the
[unsupported gates](../profiling/src/profilers/TemporalProfiler.md#native-instructions-and-timings).

The stdio backend uses the [QSP stream protocol](../qsp/README.md#interprocess-communication-protocol)
and can connect to a `StreamDriver` wrapping a simulator or profiler. Without
dedicated descriptors, reserve stdout for instructions and stdin for results;
print application diagnostics to stderr.
Instructions are buffered until a measurement/result read or the batch capacity
is reached. `qsp_save_profile()` in stdio mode only prints an informational message
to stderr: it does not flush, inspect the path, or write a file. Profiling and
saving must be handled by the profiler receiving the instruction stream.

## Supported languages

### C/C++

1. Install Rust/Cargo and a C or C++ compiler. LLVM development libraries are not required.
2. Include `qspnlib.h` in your source file. Add this package's `include`
   directory to your compiler's include path.
3. Optionally call `qsp_init(...)` to replace the default stdio backend before
   executing quantum instructions.
4. Call the instruction functions directly. Qubit and result IDs use `size_t`;
   rotation angles use `double` and are in radians. `qsp_mz()`, `qsp_mz_reset()`, and
   `qsp_read_result()` return `size_t`: 0 for zero, 1 for one, or 2 for a
   backend-reported error. `qsp_mz_store()` and `qsp_mz_store_reset()` store their
   outcomes in a result ID for later access through `qsp_read_result()`.
5. When using a profiler, call `qsp_save_profile("profile.json")` to save the
   accumulated profile.
6. Build the Rust static library with `cargo build --release -p qspnlib`,
   then compile and link your program with `target/release/libqspnlib.a`.
   On Linux, include the system libraries shown in the command below, after
   the Rust archive in the link command.

[examples/ghz.c](examples/ghz.c) prepares and measures a GHZ state, first using a
simulator and then a temporal profiler with trace recording enabled.

Build and run it:

```
cargo build --release -p qspnlib && \
cc -std=c11 -Wall -Wextra -Werror -Isource/qspnlib/include source/qspnlib/examples/ghz.c target/release/libqspnlib.a -ldl -lpthread -lm -o target/release/ghz && \
./target/release/ghz
```

### Fortran

Use [include/qspnlib.fi](include/qspnlib.fi) for Fortran interfaces to
all functions in the C header. The interfaces use `bind(C)` and
`iso_c_binding` and need no additional Rust library or wrapper.

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
4. Pass NUL-terminated UTF-8 strings to `qsp_init` and `qsp_save_profile`, for example
   `c_char_"sparse_sim" // c_null_char`. Fortran does not append the terminator
   automatically.
5. Use `call` for instructions without results. Assign the return value of
   `qsp_mz`, `qsp_mz_reset`, or `qsp_read_result` to an `integer(c_size_t)` variable.
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

    call qsp_init(c_char_"sparse_sim" // c_null_char)
    call qsp_x(0_c_size_t)
    outcome = qsp_mz(0_c_size_t)
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

## Example: Shor's algorithm (C and Fortran)

[examples/shor.c](examples/shor.c) and [examples/shor.f90](examples/shor.f90)
port [the Q# Shor sample](../../samples/algorithms/Shor.qs) to the native API.
The C example has self-test, factorization, profiling and random-shot modes.
Profiling uses `counting_profiler`, whose measurement results do not simulate a
quantum state. The example ignores the factorization result and does not retry.
The Fortran example uses `sparse_sim` and defaults to factoring 187 = 11 \* 17.

The C example requires the GMP development headers and library (on Ubuntu/Debian,
install `libgmp-dev`). Its classical arithmetic supports numbers up to 2048 bits;
the Fortran example uses fixed-width arithmetic.

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
  constraints, without calling `qsp_init()` or saving a profile. Instructions stream
  to an external backend by default; only elapsed shot time is reported locally.
  Use `QDK_INSTRUCTION_FD` and `QDK_RESULT_FD` to keep the example's stdout logs
  separate from the protocol. The shot's factorization result is not retried.

Generic 2048-bit quantum simulation is impractical even with large-integer
support. Use profiling mode for large circuits, and expect very long runtimes:
the current gate-emitting implementation scales roughly cubically with bit length.

In profiling mode, each random prime has half the requested bit length, so their
product can be one bit shorter than requested. The example writes the profile to
`source/profiling/profile_viewer/profiles/shor_c_<bitlen>.json`; the parent
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

## Python API

`QspApplication` compiles C or C++ programs written using QSPNLib, automaticlaly linking
it with pre-built QSPNLib buinary.

_Currently for this to work, the user must manually build QSPNLib using
`cargo build --release -p qspnlib`. TODO:
bundle pre-built `libqspnlib.a` with QDK and update QspApplication to know where it is located._

Pass `extra_sources=[...]` for additional source files, `compiler_args=[...]` for
compiler/linker flags and `args=[...]` for executable arguments.

### Profiling and tracing

Pass the application to `QREProfiler` to profile it, or call `get_trace()` to
collect a trace for QREv3:

```python
from qdk.qre.application import QspApplication
from qdk.qre.profiler import QREProfiler

application = QspApplication("my_program.cpp")
result = QREProfiler.counting().profile(application)
print(result.totals.calls_per_operation)

trace = application.get_trace()
```

Both calls compile and run the program, using the QSP interprocess protocol to
exchange instructions and results. The program can still print to stdout.
Call `qsp_` instructions without `qsp_init()` or `qsp_save_profile()`, and finish
with a measurement/result read to submit buffered gates.

Tracing runs the program again. Measurement outcomes are random, so the trace
records only the path taken, without simulating a quantum state.

### Standalone execution

To run without an external backend, the native program must select a local
backend, for example with `qsp_init("sparse_sim")`. If it selects a profiler,
call `qsp_save_profile("profile.json")` before exiting.

Use `compile()` to build the executable, then run it directly:

```python
import subprocess

application = QspApplication("standalone.cpp")
executable = application.compile()
subprocess.run([str(executable), *application.args], check=True)
```
