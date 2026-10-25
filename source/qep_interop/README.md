# Compile-time interop with other languages.

This package provides interop between various programming languages and QDK.

It allows you to write code that drives QuantumBackend by calling C-style functions.

Typical usage:
 * Write code for a quantum application in a language (e.g., C, C++, or Fortran)
   that calls the corresponding function exposed by the library for each quantum
   instruction (gate or measurement).
 * At the beginning of the code, call `init()` to instantiate a backend (which can
   be a simulator or profiler).
 * When using a profiler, call `save_profile()` at the end to save the profile to a
   JSON file (which will contain a resource estimate based on profiling).
 * Compile your code, statically linking it against the library provided by this package.
 * Execute the resulting binary.

At runtime, function calls will be turned into calls to the QuantumBackend interface.

See [qep_interop.h](include/qep_interop.h) for the full list of exposed functions.

## Supported backends in init()

For now, we support only three backends, specified by name:
 * "sparse_sim" - a sparse simulator.
 * "counting_profiler" - CountingProfiler with default parameters.
 * "temporal_profiler" - TemporalProfiler with default parameters.

Coming soon: profilers will load configuration from a file. Meanwhile, if you need
to instantiate backend with custom parameters, define custom backend name (e.g. 
"custom_profiler") and add implementation for it in `init()` in 
[src/lib.rs](src/lib.rs).


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

For example, [examples/ghz.c](examples/ghz.c) demonstrates preparing and measuring a GHZ
state, first using a simulator and then a profiler.

Build and run it with the following command:

```
cargo build --release -p qep_interop && \
cc -std=c11 -Wall -Wextra -Werror -Isource/qep_interop/include source/qep_interop/examples/ghz.c target/release/libqep_interop.a -ldl -lpthread -lm -o target/release/ghz && \
./target/release/ghz
```

### Fortran

Use [include/qep_interop.fi](include/qep_interop.fi) for Fortran interfaces to
all functions in the C header. These interfaces use `bind(C)` and
`iso_c_binding`; no additional Rust library or wrapper is required.

1. Install a Fortran compiler supporting Fortran 2008, such as `gfortran`, plus
   the Rust and LLVM prerequisites listed in the C/C++ section.
2. Include `qep_interop.fi` in the specification part of your free-form source,
   after `implicit none` and before executable statements. Add this package's
   `include` directory to the compiler's include path.
3. Use `integer(c_size_t)` for qubit IDs, result IDs, and measurement results,
   and `real(c_double)` for rotation angles in radians. Pass nonnegative IDs
   representable by `integer(c_size_t)`. The interfaces use `value` for numeric
   arguments to match the Rust C ABI; use kind-qualified literals such as
   `0_c_size_t` and `0.5_c_double`.
4. Pass NUL-terminated UTF-8 strings to `init` and `save_profile`, for example
   `c_char_"sparse_sim" // c_null_char`. Fortran does not append the terminator
   automatically.
5. Use `call` for instructions without results. Assign the return value of
   `mz`, `mz_reset`, or `read_result` to an `integer(c_size_t)` variable.
   The result codes are the same as in C: 0, 1, or 2 for a backend error.
6. From the repository root, build the Rust static library with
   `cargo build --release -p qep_interop`. This produces
   `target/release/libqep_interop.a`.
7. Compile and link your Fortran program against that archive using your
   Fortran compiler as the linker driver.

For example:

```fortran
program example
    use, intrinsic :: iso_c_binding, only: c_char, c_null_char, c_size_t
    implicit none
    include 'qep_interop.fi'
    integer(c_size_t) :: outcome

    call init(c_char_"sparse_sim" // c_null_char)
    call x(0_c_size_t)
    outcome = mz(0_c_size_t)
    print *, 'Measurement:', outcome
end program example
```

The example in [examples/ghz.f90](examples/ghz.f90) is equivalent to the C example above.
Build and run from the repository root on Linux:

```sh
cargo build --release -p qep_interop && \
gfortran -std=f2008 -Wall -Wextra -Werror -Isource/qep_interop/include source/qep_interop/examples/ghz.f90 target/release/libqep_interop.a -ldl -lpthread -lm -o target/release/ghz_fortran && \
./target/release/ghz_fortran
```

## More examples

### Shor's algorithm (C and Fortran)

[examples/shor.c](examples/shor.c) and [examples/shor.f90](examples/shor.f90)
port [the Q# Shor sample](../../samples/algorithms/Shor.qs) to the interop API.
Both select `sparse_sim` and default to factoring **187 = 11 * 17**.

Build both examples from the repository root on Linux:

```sh
cargo build --release -p qep_interop
cc -std=c11 -O2 -Wall -Wextra -Werror \
    -Isource/qep_interop/include source/qep_interop/examples/shor.c \
    target/release/libqep_interop.a -ldl -lpthread -lm \
    -o target/release/shor_c
gfortran -std=f2008 -O2 -Wall -Wextra -Werror \
    -Isource/qep_interop/include source/qep_interop/examples/shor.f90 \
    target/release/libqep_interop.a -ldl -lpthread -lm \
    -o target/release/shor_fortran
```

Run with no argument for 187, or supply a smaller composite for a quicker run:

```sh
./target/release/shor_c
./target/release/shor_fortran
./target/release/shor_c 15
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
