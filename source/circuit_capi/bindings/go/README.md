---
description: Experimental Go bindings distributed with the local QDK circuit native SDK.
---

# QDK circuit Go binding

Import the `circuit` package instead of calling cgo directly:

```go
import circuit "github.com/microsoft/qdk/source/circuit_capi/bindings/go"
```

The initial surface covers circuit creation and cloning, H/X/CX and measurement,
scoped `IfOne` bodies, binary and S-expression snapshots, and QIR compilation.
It is not a complete wrapper for the C API. Graph passes, parameter expressions,
other gates, and nested or else bodies are not exposed by this package yet.
Simulation remains a separate consumer of exported QIR, as shown in the notebook
samples.

## Status and prerequisites

This package and the native ABI are experimental and versioned together. Always
use the Go sources, generated header, and native library from the same QDK build.
Local SDK staging is supported; no release publishing, stable ABI, or
`go get` installation contract is provided yet.

Use Go 1.20+, a GCC/Clang-compatible C compiler, and cgo. Runtime validation
currently covers macOS arm64. Linux uses the same library layout; Windows users
should use WSL2 for this sample workflow until native Windows packaging is added.
There are no third-party Go module dependencies.

## Build and consume a local SDK

From the QDK checkout, run:

```sh
python source/circuit_capi/build.py --test
```

The printed SDK location is under Cargo's resolved target directory:

```text
circuit-capi/debug/
  LICENSE.txt
  include/qdk_circuit.h
  lib/libqdk_circuit_capi.dylib    (or .so / .dll for the build platform)
  bindings/go/
    go.mod
    shim.go
    shim_test.go
    README.md
    LICENSE.txt
```

The native library is copied into the SDK, not referenced back into Cargo's
build tree. Copy this layout together when moving the SDK.

In your application's Go module, set `QDK_CIRCUIT_SDK` to that absolute SDK path.
Configure cgo to find its header and library, then add a local module replacement:

```sh
export QDK_CIRCUIT_SDK="/absolute/path/to/circuit-capi/debug"
export CGO_ENABLED=1
export CGO_CFLAGS="-I\"$QDK_CIRCUIT_SDK/include\""
export CGO_LDFLAGS="-L\"$QDK_CIRCUIT_SDK/lib\""
go mod edit -require=github.com/microsoft/qdk/source/circuit_capi/bindings/go@v0.0.0
go mod edit "-replace=github.com/microsoft/qdk/source/circuit_capi/bindings/go=$QDK_CIRCUIT_SDK/bindings/go"
```

The `v0.0.0` requirement is a local placeholder resolved by `replace`, not a
published release. No network download of the binding is needed.

Set the runtime loader path for the platform before `go run`, `go test`, or
running a built executable:

```sh
# macOS
export DYLD_LIBRARY_PATH="$QDK_CIRCUIT_SDK/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"

# Linux
export LD_LIBRARY_PATH="$QDK_CIRCUIT_SDK/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

These are separate build-time and runtime settings. Production packaging may
instead set an appropriate loader-relative rpath; this local SDK does not
configure a system-wide library installation.

## Ownership and errors

- `New`, `Clone`, `FromBinary`, and `FromText` return independently owned circuits.
  Close each one explicitly; no finalizer releases native resources.
- A Go value copy shares its circuit's lifetime. Use `Clone`, not a struct copy,
  for independent ownership. The zero value and released circuits reject
  operations with `ErrClosed`.
- `IfOne` scopes builders and conditions to its callback. Return operation errors
  from the callback. On failure it cancels the unfinished body, then releases
  dependent handles. A retained `Body` rejects calls after the scope ends.
- Methods are not thread-safe. Serialize access to a circuit and its aliases.
- Errors are ordinary Go errors. Native failures expose `*circuit.Error` with
  a status code and diagnostic, accessible through `errors.As`.
- Snapshot methods return copied Go byte slices, independent of the circuit's
  lifetime. The binding releases native output buffers and diagnostics.
- Bound untrusted snapshot input sizes before importing them.

```go
func buildFeedback() (err error) {
    c, err := circuit.New(2, 2)
    if err != nil {
        return err
    }
    defer func() { err = errors.Join(err, c.Close()) }()

    if err := c.H(0); err != nil {
        return err
    }
    if err := c.Measure(0, 0); err != nil {
        return err
    }
    if err := c.IfOne(0, func(body *circuit.Body) error {
        return body.X(1)
    }); err != nil {
        return err
    }
    return c.Measure(1, 1)
}
```

The snippet also requires the standard `errors` package. Named-return cleanup
propagates close errors without hiding an earlier construction failure.

## Validation

The native SDK build's `--test` mode runs the binding's tests from the staged
module with `GOEXPERIMENT=cgocheck2`, alongside the existing C and Go ABI tests.
With the environment above configured, the binding can also be tested directly:

```sh
cd "$QDK_CIRCUIT_SDK/bindings/go"
GOEXPERIMENT=cgocheck2 go test -count=1 ./...
```

ABI fault injection and retained native callback tests remain in the QDK test
harness; they are not exported by this package.
