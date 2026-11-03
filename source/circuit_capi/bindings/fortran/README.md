---
description: Experimental Fortran circuit types and scoped builders distributed as source with the native SDK.
---

# QDK circuit Fortran binding

Use the `qdk_circuit` module instead of managing C pointers and diagnostic buffers.
Its initial surface matches the small Go binding: creation and cloning,
H/X/CX/measurement, scoped `if_one` bodies, binary and S-expression snapshots,
and QIR compilation. Graph passes, parameters, other gates, nested bodies,
and else bodies are not exposed yet.

The module and C adapter are experimental and versioned with the native ABI.
Use sources, header, and library from the same build. This adds local SDK
staging, not release publishing or a stable ABI promise.

## Build and use

From an activated build environment in the QDK checkout:

```sh
FC=flang python source/circuit_capi/build.py --test-fortran
```

The SDK under Cargo's resolved `target/circuit-capi/debug/` contains:

```text
include/qdk_circuit.h
lib/libqdk_circuit_capi.dylib    (or .so / .dll for the build platform)
bindings/fortran/
  qdk_circuit.f90
  bridge.c
  test_binding.f90
  README.md
  LICENSE.txt
```

Fortran `.mod` and `.smod` files are compiler/version-specific. Compile the module
source with the same compiler and compatible flags as your application. They are
not distributed as portable SDK artifacts; compiled objects are not shipped
either. Compile in a separate build directory, not inside `bindings/fortran/`.

For a standalone application on macOS or Linux:

```sh
export QDK_CIRCUIT_SDK="/absolute/path/to/circuit-capi/debug"
mkdir -p build
cd build
cc -std=c11 -I"$QDK_CIRCUIT_SDK/include" \
  -c "$QDK_CIRCUIT_SDK/bindings/fortran/bridge.c" -o qdk_fortran_bridge.o
flang "$QDK_CIRCUIT_SDK/bindings/fortran/qdk_circuit.f90" \
  /absolute/path/to/application.f90 qdk_fortran_bridge.o \
  -L"$QDK_CIRCUIT_SDK/lib" -lqdk_circuit_capi \
  "-Wl,-rpath,$QDK_CIRCUIT_SDK/lib" -o application
./application
```

The application needs Fortran 2008 features and `ISO_C_BINDING`; use LLVM Flang
or GNU Fortran. Runtime validation currently covers Flang on macOS arm64.
Use WSL2 for the sample workflow on Windows. The driver honors `FC` and `CC`,
and compiles the staged module and adapter with the selected toolchain.

## Types and errors

- `circuit_t` owns a circuit through a private, generation-checked handle.
- `body_t` permits operations only during an `if_one` callback.
- `error_t` returns `code`, an allocatable `message`, and `failed()`/`clear()`.
  Zero means success, positive codes are native failures, and negative codes
  identify binding validation or allocation failures.
- `qdk_char` is the character kind for snapshot and QIR buffers.
- `profile_base`, `profile_adaptive_ri`, `profile_adaptive_rif`,
  `profile_adaptive`, and `profile_unrestricted` select QIR capabilities.

Methods return errors to the application. The reusable module never calls
`stop` or `error stop`. Check the error after each call before using its output.
Export methods leave the allocatable output unallocated on failure.

Callbacks have a read-only `body_t` argument and an output `error_t`.
The body methods mutate the native circuit, not the Fortran handle value.
Return a failed `error_t` with a diagnostic to reject construction; `if_one`
cancels the unfinished body and releases its builder and condition. Cleanup
errors are retained alongside the original error.

Use module or external procedures for callbacks. Internal procedure arguments
require stack trampolines with the current macOS Flang toolchain and are not
supported by this workflow. The sample and tests use module procedures without
requiring executable-stack linker settings.

```fortran
module feedback_actions
    use qdk_circuit
    implicit none
contains
    subroutine correction(body, error)
        type(body_t), intent(in) :: body
        type(error_t), intent(out) :: error
        call body%x(1, error)
    end subroutine
end module
```

An application using this module can call:

```fortran
call circuit%if_one(0, correction, error)
if (error%failed()) then
    ! Report error%message and choose application-level recovery.
end if
```

The example application chooses to stop on unexpected errors; that policy
belongs to the application, not the reusable binding.

## Ownership and assignment

Call `init` or `load_binary`/`load_text` on a closed destination. Call `close`
for every independently created circuit. There are no finalizers that close a
circuit when a local alias leaves scope.

Intrinsic assignment aliases the same circuit:

```fortran
alias = original
```

Closing either invalidates both. Repeated close is harmless, and stale aliases
cannot access a later circuit that reuses the registry slot. The registry grows
with peak concurrent handles and reuses released slots.

Use `clone` for independent ownership:

```fortran
call original%clone(independent, error)
```

The destination must be closed. Close an owned circuit before overwriting its
last alias or letting it leave scope. The module refuses initialization/import
or cloning into an already live destination, but ordinary assignment remains
the caller's responsibility.

An active callback keeps the owner alive: closing it is refused and leaves it
usable. Retained copies of a `body_t` are rejected after callback completion or
cancellation. Serialize access to this module; the registry and native objects
are not thread-safe.

## Persistence

`marshal_binary` and `marshal_text` return owned allocatable character buffers.
Binary buffers can contain NUL bytes; use their full length, not `trim`, and
write them with unformatted stream I/O to avoid record markers.
`load_binary`/`load_text` copy their input into native circuit storage.
Bound untrusted input sizes before importing.

`qir(profile, data, error)` returns textual QIR. The notebooks write this as
`.ll` and pass it to Python for CPU or opt-in GPU simulation.

## Validation

`--test-fortran` runs both the existing low-level ABI fixture tests and the
reusable module's tests, compiled from staged sources. The binding tests cover
copy/clone lifetimes, stale handles and slot reuse, registry growth, scoped
builders, error paths, snapshots, and capability diagnostics.
