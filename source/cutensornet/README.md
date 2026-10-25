# qdk_cutensornet

## What this crate does

`qdk_cutensornet` is the Rust boundary between the QDK and NVIDIA cuTensorNet.
It finds and loads cuTensorNet and the CUDA Runtime at run time, so optional GPU
acceleration does not make CUDA part of the QDK's ordinary build or startup
contract.

Native shared libraries expose functions under exported names called
_symbols_, such as `cudaMalloc` or `cutensornetCreate`. To keep NVIDIA software
optional, QDK does not link those functions into every build. When `discover()`
is called, this crate opens compatible libraries installed on the host, looks
up the approved symbols by name, and stores their addresses as typed Rust
function pointers. This lookup is called _symbol resolution_; resolving a
function does not execute it or perform GPU work.

The crate maintains two approved function surfaces:

- The CUDA Runtime functions declared in `src/bindings/cudart_12.rs` and
  assembled into the handwritten `CudaFunctions` table in `src/library.rs`.
  This surface currently contains 12 required functions. The `_12` suffix
  identifies CUDA Runtime major version 12; it is not the function count.
- The cuTensorNet functions listed in `scripts/cutensornet-symbols.txt`. This
  surface currently contains 54 required functions and the optional
  `cutensornetGetLastError`; its typed loader tables are generated from the
  audited declarations in `src/bindings/v2_13.rs`.

The handwritten ABI files serve different purposes:
`src/bindings/cudart_12.rs` contains hand-audited CUDA Runtime declarations,
while `src/bindings/mod.rs` contains hand-audited compile-time assertions that
verify selected layout and constant assumptions across both binding surfaces.

`discover()` opens both libraries, resolves these function surfaces, validates
their versions, and returns an `Availability` value that keeps the libraries
loaded. Code executing a later cuTensorNet-backed workload uses the stored
function pointers; discovery itself performs no GPU work.

The loader explicitly discovers and validates both libraries. It is Linux
x86-64 only and introduces no native link-time dependency; instead, it uses
[explicit runtime dynamic loading](#why-dynamic-loading). Building the QDK,
loading another QDK component, and using CPU simulation require none of the
following:

- an NVIDIA GPU;
- a CUDA or cuTensorNet installation;
- NVIDIA headers or shared libraries; or
- a CUDA-aware linker configuration.

On other platforms the crate compiles, and `discover()` reports the libraries
as unavailable.

## Lifecycle at a glance

Most users and contributors do not run code generation. The cuTensorNet Rust
declarations and dynamic-loader tables are generated ahead of time from a
pinned NVIDIA SDK, reviewed, and committed to this repository. Ordinary Cargo
builds compile those checked-in files and require neither the SDK archive nor
NVIDIA headers.

Maintainers rerun code generation only when they need to update or reproduce
those files; see the [FFI generation documentation](scripts/README.md).
Separately, at run time, a QDK component calls `discover()` only when preparing
a cuTensorNet-backed workload. That operation looks for compatible cuTensorNet
and CUDA libraries installed on the host.

```text
pinned NVIDIA headers
  -> maintainer generation
  -> reviewed, checked-in Rust source
  -> ordinary Cargo build
  -> optional runtime library discovery
```

## What this crate contains

The implementation is split into narrow layers so that generated ABI facts,
runtime availability and native resources do not acquire overlapping
responsibilities.

The crate works with two native shared libraries. The CUDA Runtime
(`libcudart.so.12`) provides the selected CUDA version, device, memory, and
stream APIs. NVIDIA cuTensorNet (`libcutensornet.so.2`) provides the tensor
network API. Discovery loads and validates both libraries, although it invokes
only their approved version and error probes and performs no GPU work.

| Layer                     | Location                                         | Responsibility                                                                                                                 |
| ------------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Generated cuTensorNet ABI | `src/bindings/v2_13.rs`                          | Reduced declarations generated from the audited cuTensorNet 2.13 header.                                                       |
| Audited CUDA Runtime ABI  | `src/bindings/cudart_12.rs`                      | Hand-audited declarations for the approved CUDA Runtime functions; `_12` denotes CUDA major version 12.                        |
| ABI assertions            | `src/bindings/mod.rs`                            | Hand-audited compile-time size, alignment, offset, and selected constant checks.                                               |
| Version policy            | `src/version.rs`                                 | Accepts only the audited cuTensorNet and CUDA Runtime versions.                                                                |
| Dynamic loader            | `src/library.rs`                                 | At run time, opens both libraries, resolves approved exports into typed pointers, probes versions, and retains library guards. |
| Generated loader tables   | `src/library/symbols{,/*}.rs`                    | Generated cuTensorNet function-table fields and per-symbol runtime lookup calls.                                               |
| Loader generator          | `src/generator.rs`, `src/bin/generate-loader.rs` | Maintainer tool that reads the manifest and binding signatures and writes the generated loader tables.                         |
| Availability API          | `src/lib.rs`                                     | Exposes explicit discovery and reports availability without exposing raw FFI.                                                  |
| Error and status model    | `src/error.rs`                                   | Defines structured discovery failures and maps native status values without losing unknown codes.                              |
| MPS engine                | `src/engine.rs`, `src/engine/*.rs`               | `CuTensorNetEngine`, a `tensornet::MpsEngine` on cuTensorNet's State/MPS API, over private seams of the native calls.          |
| Native engine calls       | `src/library/engine.rs`                          | Implements the engine's seams with the loaded function tables (Linux x86-64).                                                  |

The raw bindings, function tables, library guards, resolver seam, and native
status mapping are private. The public capabilities are
`discover() -> Result<Availability, AvailabilityError>` and, on Linux x86-64,
`Availability::mps_engine() -> Result<CuTensorNetEngine, EngineError>`.

## The MPS engine

`CuTensorNetEngine` implements `tensornet::MpsEngine`: it takes matrices on
one or two sites and answers probabilities, samples, Pauli expectations and
the realized chain. It knows no simulator protocol, and
`tensornet::conformance` checks it against
`tensornet::reference::ExactReference`. Its code is block ④'s cuTensorNet
mechanics, moved below the protocol seam; the circuit, the request, the branch
qualification and the consumer are not here.

| File                                       | Responsibility                                                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `src/engine.rs`                            | `CuTensorNetEngine`, `CuTensorNetState`, the `MpsEngine` implementation, the `NativeApi` seam.        |
| `src/engine/state.rs`                      | `MpsExecution`: one native state, its operator log, compute on demand, queries, sampling and cleanup. |
| `src/engine/execution.rs`                  | `MpsExecutionApi` (State/MPS calls), `MpsTarget` (the extents asked for), realized-chain checks.      |
| `src/engine/sampler.rs`                    | `SamplerApi` and `PreparedSampler`, the sampler's ordered configuration and cleanup.                  |
| `src/engine/resources.rs`                  | `SessionApi` and `SessionResources`: the device's stream and cuTensorNet handle.                      |
| `src/engine/memory_workspace.rs`, `ffi.rs` | Device memory and workspace calls; the C-compatible complex element.                                  |
| `src/engine/policy.rs`                     | `ExecutionPolicy`: device, SVD cutoffs, algorithm and gauge, workspace limit, capture.                |
| `src/engine/error.rs`                      | `EngineError`.                                                                                        |
| `src/engine/test_double.rs`                | Host tests only: a CPU double of the native API that computes with `ExactReference`.                  |
| `src/engine/tests.rs`                      | Host tests: conformance, refusals, call order, release of every native object.                        |
| `tests/qualification/`                     | The A100 qualification (ignored tests; see [A100 qualification](#a100-qualification)).                |

What each operation calls:

| `MpsEngine` method                                                    | cuTensorNet and CUDA calls                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| every method, first                                                   | `cudaSetDevice` with the engine's device, so engines on different devices may share a thread                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `prepare`                                                             | `cutensornetCreateState`, `cutensornetCreateWorkspaceDescriptor`                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `apply`, `project`                                                    | `cudaMalloc`, `cudaMemcpy`, `cutensornetStateApplyTensorOperator` (a projection is a non-unitary diagonal scaled by 1/√p)                                                                                                                                                                                                                                                                                                                                                                                                            |
| any read after a change (compute)                                     | first time only: `cutensornetStateFinalizeMPS`, `cutensornetStateConfigure` ×5; then `cudaMalloc` (outputs), `cudaMemGetInfo`, `cutensornetStatePrepare`, `cutensornetWorkspaceGetMemorySize`, `cudaMalloc`, `cutensornetWorkspaceSetMemory`, `cutensornetStateCompute`, `cudaStreamSynchronize`, `cutensornetStateCaptureMPS`, `cudaFree` of the previous compute's outputs and scratch                                                                                                                                             |
| `probability_of_one`, `conditional_probability_of_one`, `expectation` | per query: `cutensornetCreateNetworkOperator`, `cudaMalloc` + `cudaMemcpy` per distinct factor, `cutensornetNetworkOperatorAppendProduct`, `cutensornetCreateExpectation`, `cutensornetExpectationConfigure`, `cutensornetCreateWorkspaceDescriptor`, `cudaMemGetInfo`, `cutensornetExpectationPrepare`, `cutensornetWorkspaceGetMemorySize`, `cudaMalloc` + `cutensornetWorkspaceSetMemory` if needed, `cutensornetExpectationCompute`, `cudaStreamSynchronize`, then the destroys and `cudaFree`s. A conditional runs two queries. |
| `sample`                                                              | `cudaMemGetInfo`, `cutensornetCreateSampler`, `cutensornetSamplerConfigure` (hyper-samples, path seed), `cutensornetSamplerPrepare`, `cutensornetWorkspaceGetMemorySize`, `cudaMalloc`, `cutensornetWorkspaceSetMemory`, `cutensornetSamplerConfigure` (sample seed), `cutensornetSamplerSample`, `cutensornetDestroySampler`, `cudaStreamSynchronize`, `cudaFree` (scratch)                                                                                                                                                         |
| `fork`                                                                | `prepare`, then every logged operator applied again: cuTensorNet 2.13 offers no state copy among the loaded symbols, so a fork costs a new state and a recompute                                                                                                                                                                                                                                                                                                                                                                     |
| `shape`                                                               | the compute, if needed; the extents `cutensornetStateCompute` reports                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `close`, `Drop`                                                       | `cudaSetDevice`, `cudaStreamSynchronize`, the destroys, `cudaFree` of every retained buffer                                                                                                                                                                                                                                                                                                                                                                                                                                          |

Each compute is captured (`cutensornetStateCaptureMPS`): the computed MPS
becomes the state's start and the library forgets the operators applied so
far, so the next compute applies only the operators applied since, instead of
every operator from |0…0⟩; a terminal that reads after each of a shot's `M`
measurements then pays `O(G)` for `G` gates, not `O(M·G)`. The feature
`qualification` exposes `Availability::mps_engine_without_capture`, only to
measure that cost on the A100. The engine keeps its own operator log, which a
fork replays from |0…0⟩ in one compute, so under truncation a fork may differ
from its parent, computed in steps, within the truncation error. Nothing in M1
forks: a per-shot run projects its shot's own state in place, and conditional
probabilities are expectations of projectors on the computed state, with no
copy. A strategy that
branches shots from a shared prefix would see the difference; it is the case
to revisit, measured by the qualification's `fork_cost` rows. Operator
tensors stay allocated until the state closes, as ④'s did: about 49,400 small
tensors per H₂ shot; freeing them after the capture waits on the
qualification's `device_mib` rows. A compute's outputs and scratch are freed
when the next compute succeeds and is captured, and a query's or sample's
buffers when it returns, so a state's device memory does not grow with its
reads. A native failure while a state is changed or computed leaves it
unusable (`EngineError::Poisoned`); a failed query or sample frees its buffers
and native objects and leaves the state usable. A conditional probability
whose condition has weight at most 1e-12 of the state's norm is refused, since
on a GPU an impossible condition comes back as rounding noise rather than
zero. The engine's host tests run all of this against the test double; none
of it has run on a GPU in this crate.

## Runtime discovery

Discovery is explicit and lazy. No global loader cache or startup hook exists.
Code that intends to execute a cuTensorNet-backed workload calls `discover()`
while preparing that workload, handles any `AvailabilityError` by reporting it
or selecting an allowed fallback, and retains the returned `Availability` for
as long as its native function pointers are needed. Code that does not request
cuTensorNet acceleration never triggers discovery.

If `discover()` is never called, this crate does not inspect the host, open a
native library, probe a version, perform GPU work, or produce an error. There is
no hidden operation that requires prior initialization.

In the transaction below, _resolve_ means looking up each exported function by
name and storing its address as the corresponding typed function pointer. It
does not mean calling the function.

The discovery transaction is:

1. Reject targets other than Linux x86-64 with `UnsupportedPlatform`.
2. Validate any explicit library overrides before loading either library.
3. Open the CUDA Runtime with `RTLD_NOW | RTLD_LOCAL`.
4. Resolve the currently approved CUDA Runtime functions declared in
   `src/bindings/cudart_12.rs` into the handwritten `CudaFunctions` table.
5. Open cuTensorNet with `RTLD_NOW | RTLD_LOCAL`.
6. Resolve the required and optional cuTensorNet functions listed in
   `scripts/cutensornet-symbols.txt` into the generated function tables.
7. Probe the loaded CUDA Runtime, CUDA driver API, cuTensorNet runtime, and the
   CUDA Runtime ABI used to build cuTensorNet.
8. Apply the exact version policy and return an `Availability` value that keeps
   both libraries alive longer than every copied function pointer.

Construction is all-or-nothing. Every opened library is immediately protected
by a Rust RAII guard, and no partially initialized function table or
availability value can escape on failure. This is implemented in Rust in
`src/library.rs`: `LoadedLibrary` owns a `libloading::Library`, local values are
advanced through each fallible step with `Result` and `?`, and Rust drops every
successfully constructed guard if a later step returns an error.
`libloading::Symbol` values are not retained; plain typed function pointers are
copied out while their owning libraries stay alive.

### Loader flags

On Linux, `libloading` opens each shared object through `dlopen` with
`RTLD_NOW | RTLD_LOCAL`:

- `RTLD_NOW` requires the operating-system loader to resolve undefined symbols
  before the open succeeds. Missing transitive dependencies therefore fail at
  discovery instead of appearing later during simulation.
- `RTLD_LOCAL` keeps symbols from the opened object out of the process-global
  symbol namespace. This avoids making QDK's optional backend an implicit
  dependency of subsequently loaded components and reduces symbol collisions.

Together, these flags provide an early, bounded validation point without
polluting global process state. Transitive dependencies are still resolved by
the operating-system loader according to its normal rules.

### Failure reporting

Discovery failures preserve the context needed by a provider or user to
diagnose the installation:

- unsupported target platform;
- invalid override variable, path, and reason;
- every default path or loader name attempted for a missing library;
- the selected library path and native loader message for load or transitive
  dependency failures;
- the library path and exact missing required symbol;
- the component, observed version, and accepted version for policy failures;
  and
- the native status and copied CUDA error text for failed version probes.

### Search policy

| Component    | Exclusive override        | Ordered defaults                                                                            |
| ------------ | ------------------------- | ------------------------------------------------------------------------------------------- |
| cuTensorNet  | `QDK_CUTENSORNET_LIBRARY` | `/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2`, then `libcutensornet.so.2` |
| CUDA Runtime | `QDK_CUDART_LIBRARY`      | `/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12`, then `libcudart.so.12`     |

An override must be an absolute path to an existing regular file or symlink.
When set, it is attempted exclusively, so a bad deployment configuration does
not silently fall back to another installation. The default search is finite
and ordered; this crate does not walk the filesystem or guess unreviewed
versions. Transitive native dependencies are resolved by the operating-system
loader, and their failures retain the attempted path and loader message.

### Version policy

The accepted values are deliberately exact:

- cuTensorNet runtime: `21300` (2.13.0);
- CUDA Runtime: `12090` (12.9); and
- cuTensorNet's reported CUDA Runtime ABI: `12090`.

The CUDA driver API version is reported for diagnostics. It is not substituted
for either CUDA Runtime check. Unknown versions fail before device selection or
native handle creation; a newer version is not assumed ABI-compatible.

## FFI and ABI boundary

The FFI layer describes how Rust calls the C libraries. The ABI policy defines
the exact binary layouts, calling conventions, constants, symbols, and runtime
versions for which those calls have been audited. Both are intentionally
private and narrower than the complete NVIDIA APIs.

### Scoped declarations

- `src/bindings/v2_13.rs` is generated from the checksum-identified official
  cuTensorNet 2.13 header. It contains the selected functions required for
  discovery, state replay, expectations, sampling, general contraction and
  logging, plus the optional `cutensornetGetLastError` diagnostic and their
  required type/constant closure.
- `src/bindings/cudart_12.rs` contains hand-audited declarations for the
  currently approved CUDA Runtime functions needed by discovery and by device,
  memory and stream management. The `_12` suffix denotes CUDA Runtime major
  version 12, not the number of declared functions.
- Function pointers use exact private `unsafe extern "C"` types. Unsafe loading,
  symbol resolution, and native probe calls remain inside the crate; the public
  `discover()` API is safe Rust.
- The owning `libloading::Library` guards outlive all copied function pointers.
  Borrowed native C strings are copied into Rust-owned `String` values before
  another native call can invalidate them.
- Discovery resolves the full approved tables atomically but invokes only
  version and error probes. Resolving the state, workspace, memory, and
  stream calls does not execute them.

Ordinary Cargo builds consume only the checked-in Rust declarations. They do
not run bindgen, inspect an SDK header, execute a build script, or pass a native
link directive.

### Compile-time ABI checks

On the supported Linux x86-64 target, `src/bindings/mod.rs` rejects compilation
if the selected ABI facts drift. It checks:

- 32-bit size and alignment for represented cuTensorNet/CUDA status and enum
  values;
- 64-bit size and alignment for opaque cuTensorNet handles and CUDA streams;
- a 16-byte size and alignment, with real and imaginary fields at byte offsets
  0 and 8, for the private complex-f64 storage type intended for
  `CUDA_C_64F` buffers; and
- selected constants including `CUDA_C_64F` and host-to-device and
  device-to-host copy directions.

Path and slicing attribute payloads also have size, alignment and field-offset
assertions. Their Linux x86-64 layouts, in bytes, are:

| Type                           | Size | Alignment | Field offsets                      |
| ------------------------------ | ---- | --------- | ---------------------------------- |
| `cutensornetNodePair_t`        | 8    | 4         | `first`: 0, `second`: 4            |
| `cutensornetContractionPath_t` | 16   | 8         | `numContractions`: 0, `data`: 8    |
| `cutensornetSliceInfoPair_t`   | 16   | 8         | `slicedMode`: 0, `slicedExtent`: 8 |
| `cutensornetSlicingConfig_t`   | 16   | 8         | `numSlicedModes`: 0, `data`: 8     |

The selected cuTensorNet calls pass complex tensor storage through device
`void *` buffers rather than passing a complex struct by value. The safe layer
above the bindings must use the asserted private storage representation and explicit
conversion instead of assuming that a general Rust complex type has the C ABI.

These assertions do not claim compatibility with another operating system,
architecture, CUDA major version, header ABI, or cuTensorNet runtime. Support
for any of those requires a separate retained audit. Native status conversion
also preserves unknown numeric values so a new status is not silently
misclassified.

## Why dynamic loading

Runtime loading is the best fit for this optional QDK capability because it:

- keeps ordinary QDK builds and packages independent of proprietary GPU
  installations;
- prevents a missing NVIDIA library from breaking process startup or unrelated
  simulation methods;
- turns absence, transitive dependency failures, missing symbols, and version
  mismatches into actionable Rust errors;
- allows deployments and tests to select exact shared objects without changing
  global loader configuration; and
- keeps support tied to an audited ABI policy instead of whatever SDK happened
  to be present at build time.

Static or load-time linking would move an optional deployment concern into
every QDK build and process. A process-global native singleton was also
rejected: it would hide device and lifetime ownership, complicate tests, and
couple independent simulation workers. Dynamic loading costs a small explicit
discovery step and requires a maintained symbol/version policy; those are
appropriate costs for preserving QDK's existing CPU and cross-platform paths.

## Ownership

- `AvailabilityReport` is cloneable data containing the selected paths and
  observed versions. It owns no native resource.
- `Availability` owns the report and the private loader owner, ensuring that
  loaded libraries outlive their copied function pointers.
- `LoadedLibrary` owns one path and one `libloading::Library` guard.
- `CudaFunctions` and `CuTensorNetFunctions` are immutable typed function
  tables. Required tables are constructed atomically.
- `CuTensorNetApi` owns immutable shared-library guards plus immutable function
  tables, with no CUDA device or cuTensorNet execution handle. It is
  `Send + Sync`, so one discovery can be shared across threads; it shares no
  device context, stream or state.
- `CuTensorNetEngine` owns one device's stream and cuTensorNet handle; each
  `CuTensorNetState` owns its native state, workspace and device buffers and
  keeps the engine's session alive. Both are `!Send`.

Typical use is intentionally small:

```rust
let availability = qdk_cutensornet::discover()?;
let report = availability.report();

println!(
    "cuTensorNet {} loaded from {}",
    report.cutensornet_version,
    report.cutensornet_library.display()
);
```

Holding `availability` keeps both libraries and their function tables alive.
It does not reserve a GPU or imply that a later workload will fit in device
memory.

Code extending this crate should:

1. Treat discovery as an explicit capability check, not process
   initialization.
2. Keep the public API in QDK terms. Native library paths, symbols, pointers,
   and handles remain implementation details.
3. Add an ABI or runtime version only with retained header, symbol, layout,
   dependency, and hardware evidence. Do not broaden the allowlist merely
   because a newer library loads.
4. Preserve optionality. Ordinary tests and builds must never read SDK headers,
   call the binding generator, or require NVIDIA libraries.

## Validation

### CPU-only and no-SDK validation

```bash
cargo test -p qdk_cutensornet
cargo clippy --target x86_64-unknown-linux-gnu -p qdk_cutensornet --all-targets -- -D warnings
```

The loader is gated to Linux x86-64, so on other hosts its tests do not
compile; the `src/generator.rs` unit tests and the non-x86-64 paths still run
there. The second command type-checks the gated code from any host. On Linux
x86-64, the focused tests verify:

- invalid relative, missing, and directory overrides fail before loading;
- every missing required cuTensorNet and CUDA symbol is reported by name;
- absence of optional `cutensornetGetLastError` does not reject discovery;
- malformed shared objects and exhausted candidate lists retain actionable
  context;
- CUDA version-probe failures preserve both status and copied native text;
- only the audited runtime values are accepted;
- known and unknown native status values remain distinguishable; and
- the cuTensorNet bindings and loader match the manifest's required and
  optional symbols, and the CUDA Runtime inventory remains aligned with its
  handwritten function table.

On every host, the engine's tests (`src/engine/tests.rs`) run
`tensornet::conformance::check` and the engine's own refusals, call order and
cleanup against the CPU test double of the native API; the per-file tests of
the sampler, the session resources and the policy run there too.

On every host, `checked_in_loader_matches_freshly_generated_output` checks that
the checked-in loader matches its generator. Compile-time assertions separately
verify the selected opaque pointer sizes, represented enum sizes/alignments,
complex element layout, and constants.

### Real-library validation

`tests/availability.rs` contains an ignored discovery test for the audited CUDA
Runtime 12.9 and cuTensorNet 2.13 installation. It opens and reports the real
libraries but deliberately performs no device selection, allocation, handle
creation, or GPU work:

```bash
cargo test -p qdk_cutensornet --test availability -- --ignored --nocapture
```

This test is native-environment evidence, not a replacement for CPU-only
tests.

### A100 qualification

`tests/qualification` is the engine's first check on the real libraries, and
the measurements its open GPU questions need. Every test there is ignored and
needs the feature `qualification`:

- `conformance` and `conformance_without_capture` run
  `tensornet::conformance::check` on the engine with and without MPS capture,
  and are the only tests that assert;
- `environment` records the versions and libraries found;
- `empty_compute`, `read_cost`, `fork_cost`, `free_after_compute`,
  `shape_and_tolerance`, `impossible_condition`, `rare_condition` and
  `projection_memory` print CSV rows (`case,item,value,unit`); an engine error is a row, and the sweep
  goes on. `read_cost` runs with `capture=on` and `capture=off` on the same
  circuit and χ, records `P(site 0 = 1)` and the device memory at each
  checkpoint both ways, and adds rows tagged `extrapolation=h2`: H₂'s 1,404
  reads over 48,000 gates estimated from a fit, not measured. `fork_cost`
  also forks a state that was read after every 34 operators, and records how
  far the fork and the stepped parent are from each other and from the state
  computed once. `projection_memory` records the device memory along H₂'s
  pattern (34 gates, a read, then a measurement, 200 times) with variants of
  the step's last operator (unitary, one or two projections, a full-rank
  non-unitary, three reads first) and of the width, χ and truncation, to
  tell what the memory grows with; its slope in MiB per step is also scaled
  to H₂'s 1,404 measurements, tagged `extrapolation=h2`.

`scripts/qualify-engine-on-a100.sh` runs them on an A100 host. It checks the
host, the GPU, the libraries and their versions, the commit and the disk; it
fetches the branch from `origin` (except with `--check`, which changes
nothing), then builds and runs at the given commit in a detached worktree next
to the checkout, which it leaves untouched; and it writes the log, the rows, a
summary (each test's result, `passed` or `failed` per conformance case with
and without capture, the read cost, values and memory both ways, the forks of
read states, then every measurement) and a tarball of them, also when a step
fails:

```bash
source/cutensornet/scripts/qualify-engine-on-a100.sh --check <commit>  # checks only
source/cutensornet/scripts/qualify-engine-on-a100.sh --quick <commit>  # smaller sweeps
source/cutensornet/scripts/qualify-engine-on-a100.sh <commit>          # the qualification
source/cutensornet/scripts/qualify-engine-on-a100.sh --only projection_memory <commit>
```

`--only` takes comma-separated test names, runs only those (with
`--quick` or `--check` too), and summarizes only their sections.

By hand, from a checkout at the commit:

```bash
cargo test --release -p qdk_cutensornet --features qualification --test qualification -- \
    --ignored --nocapture --test-threads=1
```

`QDK_ENGINE_QUALIFICATION_QUICK=1` makes every sweep smaller.

### Deterministic binding validation

Checked-in generated outputs are validated for reproducibility and freshness;
see [Validating the FFI surface](scripts/README.md#validating-the-ffi-surface).

## Binding provenance

The checked-in cuTensorNet declarations are generated from NVIDIA cuQuantum
26.06.0 for CUDA 12:

- artifact: `cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz`
- artifact SHA-256: `4c37aa346fab9023d985e79667b047e13a0c0f9b9fea7dfca453979b331c8f77`
- `cutensornet.h` SHA-256: `f70f31595c3c7b44682a7e4bdcd468504615983a4ec628f519cf18f0036a4687`
- bindgen CLI: 0.72.1
- clang: Ubuntu 14.0.0-1ubuntu1.1
- full reference output SHA-256: `8921d1acf0ff6d384a793893e92e10cadc850dfb29a0312726c31c4d692c3d7a`
- reduced output SHA-256: `2252b5ea29af1bf8ed5eb2d8268530ed289647791b9fa04bdc53c8599ee956da`
- reduced output line count: 882

Changing the selected functions or types changes the reduced output without
changing the SDK inputs. The full-reference hash depends on the headers and
pinned generation tools/options, not the reduced selection; its check remains
unchanged when only that selection changes.

The source artifact is identified by NVIDIA's `redistrib_26.06.0.json`.
NVIDIA headers, archives, and binaries are not stored in this repository. See
the NVIDIA cuQuantum SDK license distributed with that artifact for the source
material's terms.

## Regeneration

Generation is an explicit maintainer operation, not an ordinary Cargo build
step. The two maintainer-run generators are:

1. `scripts/generate-bindings.sh`, which generates
   `src/bindings/v2_13.rs`.
2. `cargo run -p qdk_cutensornet --bin generate-loader`, which generates
   `src/library/symbols.rs` and `src/library/symbols/*.rs`.

See [cuTensorNet FFI generation](scripts/README.md) for the SDK archive,
prerequisites, selection rules, host requirements, generated versus handwritten
files, and validation details. After updating the selection inputs, run from
the repository root:

```bash
source/cutensornet/scripts/generate-bindings.sh \
  /path/to/cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz \
  source/cutensornet/src/bindings/v2_13.rs &&
cargo run -p qdk_cutensornet --bin generate-loader
```
