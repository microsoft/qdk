# qdk_cutensornet

> [!IMPORTANT]
> This crate is work in progress, and this README is a living design and
> implementation guide. It must be reviewed and updated in every phase so that
> implemented behavior, planned layers, ownership decisions, and validation
> evidence remain clearly distinguished.

`qdk_cutensornet` is the experimental Rust boundary between QDK and NVIDIA
cuTensorNet. The work is staged so that optional GPU acceleration does not make
CUDA part of QDK's ordinary build or startup contract.

The implemented loader can explicitly discover and validate the CUDA Runtime
and cuTensorNet at runtime. It is Linux x86-64 only and introduces no
native link-time dependency; instead, it uses
[explicit runtime dynamic loading](#why-dynamic-loading). Building QDK, loading
another QDK component, and using CPU simulation require none of the following:

- an NVIDIA GPU;
- a CUDA or cuTensorNet installation;
- NVIDIA headers or shared libraries; or
- a CUDA-aware linker configuration.

The crate also contains a private, test-only qualification layer. B0 and B1
have executed on an A100: they own the device, stream, handle, state,
workspaces, retained operators, and MPS outputs; support variable widths and
metadata-only readout; and report requested/realized extents and workspace.
B2 product-term expectation support is A100-qualified at widths 12, 16, and 20
against retained exact values and one independently refreshed QDK sparse-state
oracle. Private B3 evidence qualifies the N=128 matched-bond anchor and exact
pre-cleanup Query timing boundary. Private B4 spike evidence qualifies the
N=256 cap ladder through its converged upper plateau. Private B5 gate-1 evidence
qualifies caller-selected forced-Z branch masses, normalized non-unitary
projection, MPS capture, continuation, and Query on the same GPU-resident state.
Later Adaptive execution gates, noise, public API work, and QDK integration
remain unimplemented pending their review stops. None of these private fixtures
is a public QDK API, provider integration, or product architecture precedent.

## What this crate contains

The implementation is split into narrow layers so that generated ABI facts,
runtime availability, native resources, and QDK integration do not acquire
overlapping responsibilities.

Phase 2 works with two native shared libraries. The CUDA Runtime
(`libcudart.so.12`) provides the selected CUDA version, device, memory, and
stream APIs. NVIDIA cuTensorNet (`libcutensornet.so.2`) provides the tensor
network API. Discovery loads and validates both libraries, although it invokes
only their approved version and error probes and performs no GPU work.

| Layer                     | Location                     | Status                         | Responsibility                                                                                                                                      |
| ------------------------- | ---------------------------- | ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Generated cuTensorNet ABI | `src/bindings/v2_13.rs`      | Implemented                    | Reduced declarations generated from the audited cuTensorNet 2.13 header.                                                                            |
| Audited CUDA Runtime ABI  | `src/bindings/cudart_12.rs`  | Implemented                    | Hand-audited declarations for the 12 CUDA Runtime calls required by the spike.                                                                      |
| ABI assertions            | `src/bindings/mod.rs`        | Implemented                    | Compile-time size, alignment, offset, and selected constant checks.                                                                                 |
| Version policy            | `src/version.rs`             | Implemented                    | Accepts only the audited cuTensorNet and CUDA Runtime versions.                                                                                     |
| Dynamic loader            | `src/library.rs`             | Implemented                    | Opens `libcudart.so.12` and `libcutensornet.so.2`, resolves typed function tables, probes versions, and retains library guards.                     |
| Availability API          | `src/lib.rs`, `src/error.rs` | Implemented                    | Exposes explicit discovery, a report, and structured failures without exposing raw FFI.                                                             |
| Native qualification      | `src/library/simulation*`    | Private B0-B5 gate-1 qualified | Owns thread-confined native resources and validates Base execution, terminal product-term Queries, cap convergence, and forced-branch continuation. |
| QDK provider integration  | Future phase                 | Not implemented                | Will translate QDK simulation requests without exposing native details to callers.                                                                  |

The raw bindings, function tables, library guards, resolver seam, and native
status mapping are private. The only public Phase 2 capability is
`discover() -> Result<Availability, AvailabilityError>`.

## Runtime discovery

Discovery is explicit and lazy. No global loader cache or startup hook exists.
If `discover()` is never called, this crate does not inspect the host, open a
native library, probe a version, perform GPU work, or produce an error. Phase 2
has no hidden operation that requires prior initialization.

When availability is requested, the caller handles the result of `discover()`.
The function returns an `AvailabilityError` rather than panicking when the
platform, configuration, native libraries, symbols, or versions are unsuitable.
A future QDK provider should call `discover()` inside its own preparation path
and propagate that structured error, rather than require application code to
remember a separate initialization call.

The discovery transaction is:

1. Reject targets other than Linux x86-64 with `UnsupportedPlatform`.
2. Validate any explicit library overrides before loading either library.
3. Open the CUDA Runtime with `RTLD_NOW | RTLD_LOCAL`.
4. Resolve its complete 12-function table.
5. Open cuTensorNet with `RTLD_NOW | RTLD_LOCAL`.
6. Resolve all 24 required cuTensorNet functions and the optional
   `cutensornetGetLastError` diagnostic.
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

There is no "call `discover()`" error when discovery is omitted because code
that is never invoked cannot return an error. Once an acceleration path is
requested, its owning provider is responsible for calling `discover()` and
surfacing the returned message or selecting an allowed fallback.

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
  cuTensorNet 2.13 header. It contains 25 selected functions: 24 required by
  discovery, state replay, and product-term expectations, plus the optional
  `cutensornetGetLastError` diagnostic and their required type/constant closure.
- `src/bindings/cudart_12.rs` contains hand-audited declarations for the 12
  CUDA Runtime functions needed by discovery and the planned resource layer.
- Function pointers use exact private `unsafe extern "C"` types. Unsafe loading,
  symbol resolution, and native probe calls remain inside the crate; the public
  `discover()` API is safe Rust.
- The owning `libloading::Library` guards outlive all copied function pointers.
  Borrowed native C strings are copied into Rust-owned `String` values before
  another native call can invalidate them.
- Discovery resolves the full approved tables atomically but invokes only
  version and error probes. Resolving future state, workspace, memory, and
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

The selected cuTensorNet calls pass complex tensor storage through device
`void *` buffers rather than passing a complex struct by value. The future safe
layer must still use the asserted private storage representation and explicit
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

## Key data structures and ownership

### Implemented availability layer

- `AvailabilityReport` is cloneable data containing the selected paths and
  observed versions. It owns no native resource.
- `Availability` owns the report and the private loader owner, ensuring that
  loaded libraries outlive their copied function pointers.
- `LoadedLibrary` owns one path and one `libloading::Library` guard.
- `CudaFunctions` and `CuTensorNetFunctions` are immutable typed function
  tables. Required tables are constructed atomically.
- `NativeApi` owns immutable shared-library guards plus immutable function
  tables, with no CUDA device or cuTensorNet execution handle.

The initial Phase 1 implementation conservatively made the whole availability
owner `!Send` and `!Sync`. The subsequent NVIDIA cuTensorNet API and NVIDIA
CUDA-Q implementation review showed that this was too broad. The selected
ownership model is:

```text
Provider root
`- Arc<NativeApi>                 Send + Sync
   |- CUDA Runtime Library + immutable function table
   `- cuTensorNet Library + immutable function table

Worker thread
`- Session                       !Send + !Sync
   |- selected CUDA device and stream
   |- cuTensorNet handle
   |- state and workspace
   `- retained gate, scratch, and output buffers
```

Only the loader/function-table owner is shared. This is not a claim that a
cuTensorNet handle, state, workspace, stream, or buffer graph is safe to share.
The initial broad thread-confinement marker has been removed from this
immutable owner; the future mutable `Session` remains the confinement boundary.

### Private qualification session

A test-only `Session` is the unit of mutable native ownership and execution. It
remains structurally `!Send` and `!Sync`. Methods that alter or compute native
state require `&mut self`, and safe callers receive no raw native pointers.

This boundary follows the evidence:

- NVIDIA documents `cutensornetCreate` as thread-safe, but state, workspace,
  contraction, and sampling calls operate on mutable `inout` objects, retained
  buffers, internal metadata, or per-instance PRNG state. A handle is fixed to
  the CUDA device active when it is created.
- CUDA-Q creates one thread-local tensor-network simulator. That simulator owns
  its handle, state, scratch memory, cache, and PRNG, and deletes copy and move
  construction.

## Threading and process model

The initial execution model is parallelism through independent ownership, not
concurrent access to one native graph.

### Recommendation

Use one process-scoped `Arc<NativeApi>` for immutable library guards and
function tables, and create one independent `!Send + !Sync` `Session` inside
each owning worker thread. Each process performs its own discovery and creates
its own sessions. The initial implementation neither transfers native resources
between threads/processes nor enables cuTensorNet distributed execution.

This is the recommended model because:

- `NativeApi` is immutable and contains no device or simulation state, so it
  can be shared without sharing an execution graph.
- cuTensorNet handles are device-associated, and session operations mutate
  native objects or retain caller-owned buffers.
- CUDA's active device is a calling-thread concern, so worker ownership is a
  clearer invariant than implicit rebinding.
- NVIDIA CUDA-Q independently uses a non-copyable, non-movable, thread-local
  simulator containing its handle, state, scratch storage, cache, and PRNG.

The tradeoff is one native session and its device resources per active worker.
The provider controls that cost through its worker count, GPU assignment, and
admission policy.

### Multithreading

- Perform discovery once in provider setup and share only `Arc<NativeApi>` with
  workers.
- Create, use, and destroy each `Session` on one worker. Expose native state
  transitions through `&mut self`; a lock does not establish `Sync` or correct
  CUDA device affinity.
- Bind the intended CUDA device before handle creation and before any operation
  or cleanup that relies on the calling thread's current device.
- Synchronize the owned stream before host reads, buffer reuse, or destruction
  of queued resources. Do not share session handles, states, workspaces,
  streams, or buffers between workers.

### Multiple processes

- Treat libraries, function pointers, CUDA state, handles, streams, workspaces,
  and device pointers as process-local. Each process discovers and initializes
  independently; `Arc` and raw native values are not IPC mechanisms.
- Configure library overrides and GPU visibility before discovery. Assign GPUs
  explicitly per process or rank; discovery does not reserve capacity or
  coordinate memory budgets.
- Fork safety has not been audited. Do not call `fork()` after discovery or
  session initialization and use inherited backend state in the child. Use a
  fresh process image and initialize independently.
- Phase 2 and the planned Phase 3 surface do not include cuTensorNet's
  distributed/MPI APIs. MPI ranks may run independent sessions, but native
  distributed contraction requires a separate design and validation phase.

## Developer model

Code using or extending this crate should follow these rules:

1. Treat discovery as an explicit capability check, not process
   initialization.
2. Keep the public API in QDK terms. Native library paths, symbols, pointers,
   and handles remain implementation details.
3. Share immutable `NativeApi`; never share or globally cache a mutable
   session.
4. Create and destroy each session on its owning worker. Bind the intended CUDA
   device before handle creation and before any future operation or cleanup
   whose contract depends on the calling thread's active device.
5. Encode retained native pointers as Rust ownership. Gate, MPS output, and
   workspace buffers must outlive every native object that can reference them.
6. Synchronize asynchronous work before host reads, dependent resource
   destruction, or buffer reuse.
7. Make partial construction locally safe. Each successful native creation
   immediately gains one non-panicking RAII owner.
8. Add an ABI or runtime version only with retained header, symbol, layout,
   dependency, and hardware evidence. Do not broaden the allowlist merely
   because a newer library loads.
9. Preserve optionality. Ordinary tests and builds must never read SDK headers,
   call the binding generator, or require NVIDIA libraries.

Typical Phase 2 use is intentionally small:

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

## Validation strategy

Phase 2 is designed to be validated without CUDA first, then corroborated in a
frozen native environment.

### CPU-only and no-SDK validation

```bash
cargo check -p qdk_cutensornet --all-targets
cargo test -p qdk_cutensornet
cargo tree -p qdk_cutensornet
```

The focused tests verify:

- invalid relative, missing, and directory overrides fail before loading;
- every missing required cuTensorNet and CUDA symbol is reported by name;
- absence of optional `cutensornetGetLastError` does not reject discovery;
- malformed shared objects and exhausted candidate lists retain actionable
  context;
- CUDA version-probe failures preserve both status and copied native text;
- only the audited runtime values are accepted;
- known and unknown native status values remain distinguishable; and
- the required-symbol inventories remain frozen at 24 cuTensorNet and 12 CUDA
  Runtime functions.

Compile-time assertions separately verify the selected opaque pointer sizes,
represented enum sizes/alignments, complex element layout, and constants used
by the future replay.

### Real-library validation

`tests/availability.rs` contains an ignored discovery test for the audited CUDA
Runtime 12.9 and cuTensorNet 2.13 installation. It opens and reports the real
libraries but deliberately performs no device selection, allocation, handle
creation, or GPU work:

```bash
cargo test -p qdk_cutensornet --test availability -- --ignored --nocapture
```

This test is native-environment evidence, not a replacement for CPU-only
tests. The private Bell, ordering, and width tests have separate retained A100
evidence. The B2 expectation test has separate retained A100 evidence at widths
12, 16, and 20.

### Deterministic binding validation

Binding regeneration is maintainer-only and never runs under Cargo. The script
checks the archive, header, bindgen, clang, and full-reference hashes; runs the
reduced generation twice; compares both outputs byte-for-byte; validates the
selected declaration set; and reports the resulting hash and line count.

The complete decision history, native object graph, validation gates, and
phase boundaries are retained in
[`CUTENSORNET-RUST-FFI-WORKING-DOC.md`](../../CUTENSORNET-RUST-FFI-WORKING-DOC.md).

## Binding provenance

The checked-in cuTensorNet declarations are generated from NVIDIA cuQuantum
26.06.0 for CUDA 12:

- artifact: `cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz`
- artifact SHA-256: `4c37aa346fab9023d985e79667b047e13a0c0f9b9fea7dfca453979b331c8f77`
- `cutensornet.h` SHA-256: `f70f31595c3c7b44682a7e4bdcd468504615983a4ec628f519cf18f0036a4687`
- bindgen CLI: 0.72.1
- clang: Ubuntu 14.0.0-1ubuntu1.1
- full reference output SHA-256: `8921d1acf0ff6d384a793893e92e10cadc850dfb29a0312726c31c4d692c3d7a`
- reduced output SHA-256: `074f43ebc97494b8311d0deb9b2cc92fb86d2782af527e5121b867962f8e7eb8`
- reduced output line count: 449

The source artifact is identified by NVIDIA's `redistrib_26.06.0.json`.
NVIDIA headers, archives, and binaries are not stored in this repository. See
the NVIDIA cuQuantum SDK license distributed with that artifact for the source
material's terms.

## Regeneration

Regeneration is a maintainer operation and is never invoked by Cargo. In the
pinned Phase 2 environment, run:

```bash
source/cutensornet/scripts/generate-bindings.sh \
  /path/to/cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz \
  source/cutensornet/src/bindings/v2_13.rs
```

The script validates every pinned input, verifies the known full bindgen
output, generates the approved reduced declarations twice, requires the two
outputs to be byte-identical, and reports the final SHA-256. The full reference
hash verifies the complete input closure; the reduced output hash identifies
the exact declarations checked into `src/bindings/v2_13.rs`.
