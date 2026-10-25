# Compile-time interop with other languages.

This package provides interop between various programming languages and QDK.

It alows to write code that drives QuantumBackend by calling C-style functions. 

Typical usage: 
 * Write code for quantum application in some language (e.g. C, C++, Fortran)
   where for each quantum instruction (gate or measurment) they  call corresponing 
   function exposed by the library.
 * In the beginning of the code, call `init()` to instantiate a backend (which can
   be a simulator or profiler).
 * When using profiler, in the end call `save_profile()` to save profile to JSON file
   (which will contain resource estimate based on profiling).
 * Compile their code.
 * Link it against library provided by this package.
 * Run the code.

In runtime, function calls will be turned into calls to QuantumBackend interface.

See [qep_interop.h](include/qep_interop.h) for full list of exposed functions.

## Supported backends in init()

For now, we support only 3 backends that are specified by name:
 * "sparse_sim" - sparse simulator.
 * "counting_profiler" - CountingProfiler with default parameters.
 * "temporal_profiler" - TemporalProfiler with default parameters.

Coming soon: profilers will load configuration form a file.


## Supported languages

### C/C++

1. Install Rust/Cargo and a C or C++ compiler. The `qep` dependency also requires
   LLVM 20 development libraries. If LLVM is not found automatically, set
   `LLVM_SYS_201_PREFIX` to its installation directory.
2. Include `qep_interop.h` in your source file. Add this package's `include`
   directory to your compiler's include path.
3. Call `init(...)` before executing quantum instructions.
4. Call the instruction functions directly. Qubit and result IDs use `size_t`;
   rotation angles use `double` and are in radians. `mz()`, `mz_reset()`, and
   `read_result()` return `size_t`: 0 for zero, 1 for one, or 2 for a
   backend-reported error. `mz_store()` and `mz_store_reset()` store their
   outcomes in a result ID for later access through `read_result()`.
5. When using a profiler, call `save_profile("profile.json")` to save the
   accumulated profile.
6. Build the Rust static library with `cargo build --release -p qep_interop`,
   then compile and link your program with `target/release/libqep_interop.a`.
   On Linux, include the system libraries shown in the command below, after
   the Rust archive in the link command.

For example, [examples/ghz.h](examples/ghz.c) demonstrates preparing and measuring GHZ
state, run on simulator and then profiled with a profiler. 

Build andrun it with the following command:

```
cargo build --release -p qep_interop && \
cc -std=c11 -Wall -Wextra -Werror -Isource/qep_interop/include source/qep_interop/examples/ghz.c target/release/libqep_interop.a -ldl -lpthread -lm -o target/release/ghz && \
./target/release/ghz
```
