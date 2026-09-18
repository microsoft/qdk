# cuTensorNet FFI generation

There are two maintainer-run generators: one translates selected NVIDIA C API
declarations into Rust, and the other generates typed dynamic-loading code from
those declarations. Neither generator loads NVIDIA libraries or performs GPU
work. Ordinary Cargo builds consume the checked-in output; they do not regenerate
it.

```text
NVIDIA SDK headers + CUDA headers + function/type selection
                            |
               scripts/generate-bindings.sh
               (pinned Linux x86-64 host; CPU work)
                            |
                 src/bindings/v2_13.rs
                            |
                 + function manifest
                            |
       cargo run -p qdk_cutensornet --bin generate-loader
       (any supported Rust development host; CPU work)
                            |
         src/library/symbols.rs and symbols/*.rs
```

Only `src/bindings/v2_13.rs` and `src/library/symbols{,/*}.rs` are generated
by this workflow. Never hand-edit them: regeneration replaces manual changes,
and the loader's `--check` mode and test cross-checks detect stale output.
`src/bindings/cudart_12.rs` is hand-audited, and the ABI assertions in
`src/bindings/mod.rs` are handwritten. Safe wrappers and numerical execution
logic are not generated.

Unless stated otherwise, paths and shell commands below are relative to
`source/cutensornet/`.

## What lives here

| File / target              | Role                                                                                                                         |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `cutensornet-symbols.txt`  | The symbol manifest. Single source of truth for **which functions** the crate binds.                                         |
| `generate-bindings.sh`     | Headers and function/type selection &rarr; `src/bindings/v2_13.rs`. Requires the pinned x86-64 tools and headers, not a GPU. |
| `src/generator.rs`         | Manifest + bindings &rarr; the loader model &rarr; source text. Pure, unit-tested.                                           |
| `--bin generate-loader`    | Reads the manifest and Rust bindings; writes/formats the loader files. Runs on any supported Rust development host.          |
| `validate-on-cuda-host.sh` | Checks the FFI surface on Linux x86-64, with native-library and optional GPU qualification steps.                            |

Both generators consume the same manifest, so the bindgen allowlist and the
dynamic loader cannot disagree about which symbols exist.

The loader generator is Rust rather than a script for two reasons: it is covered
by the workspace-wide `cargo test` that CI already runs, and its logic is a pure
function of two strings, so every failure mode is a unit test. It is split into
a **model** (`Loader`, built by `Loader::build`, which performs all validation)
and a **serializer** (`serialize`, which is infallible text assembly). Tests
assert against the model wherever possible, so they describe what the loader
binds rather than how the file happens to be laid out.

## The manifest

Four whitespace-separated columns; `#` starts a comment.

```
# <c_symbol>                     <rust_field>            <family>     <required|optional>
cutensornetNetworkAppendTensor   network_append_tensor   contraction  required
cutensornetGetLastError          get_last_error          context      optional
```

- **`c_symbol`** — the exported symbol, resolved via `dlsym`.
- **`rust_field`** — the field on `CuTensorNetFunctions`. Also determines the
  function-pointer alias (`network_append_tensor` &rarr; `NetworkAppendTensorFn`).
  Names are _not_ derived from the C symbol, because several established ones
  are irregular (`finalize_mps`, `append_product`).
- **`family`** — selects the generated file. One of `context`, `state`,
  `workspace`, `operator`, `expectation`, `sampler`, `contraction`. Assign it by
  symbol prefix (check `NetworkOperator*` before `Network*`) so grouping stays
  mechanical. `context` rather than `core`, so the module never shadows the
  `core` crate.
- **`required`** — absence aborts discovery. **`optional`** — absence degrades
  the field to `None` and must never reject a library.

## Adding a function

1. Add a manifest row, in its family's block so the generated diff stays local.
2. Regenerate the bindings **in the pinned Linux x86-64 environment** (see below).
   No GPU is needed. Regeneration is required even though the header is unchanged:
   the new symbol has no declaration until the
   allowlist widens, and `cargo test` fails until it does.
3. Run `cargo run -p qdk_cutensornet --bin generate-loader`.
4. Review and validate the result, then commit the manifest and both generated
   outputs together. A manifest-only change leaves the generation checks failing.

Write any safe wrapper by hand — the generator stops at the raw pointer.

That row is the only place the symbol is named. The function-pointer alias, the
`CuTensorNetFunctions` field, the `resolve_*` call, the bindgen allowlist and the
required-symbol inventory the tests assert against are all derived from it, so
there is no second list to update and no way for two of them to disagree.

## Adding a type or a constant

The manifest covers functions only. Types are hand-maintained in
`generate-bindings.sh`:

- `TYPE_PATTERN` — bindgen `--allowlist-type`.
- `REQUIRED_DECLARATIONS` — names asserted to be present in the output.

bindgen pulls in types reachable from an allowlisted function signature, so a
type only needs listing when nothing in the surface references it. A type
reached only through a `void *` attribute buffer — `cutensornetComputeType_t`,
for instance — is _not_ pulled in transitively and must be named explicitly.
Adding to these lists requires regenerating the bindings.
Add or update the handwritten size/alignment/offset assertions in
`src/bindings/mod.rs` when introducing a new ABI payload.

The optimizer metadata payloads follow the same rule:

| Attribute                                               | Allowlisted payload            | Transitively included element type |
| ------------------------------------------------------- | ------------------------------ | ---------------------------------- |
| `CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_PATH`           | `cutensornetContractionPath_t` | `cutensornetNodePair_t`            |
| `CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_SLICING_CONFIG` | `cutensornetSlicingConfig_t`   | `cutensornetSliceInfoPair_t`       |

Both payloads are passed through the `void *` attribute buffers of
`cutensornetContractionOptimizerInfoGetAttribute` and
`cutensornetContractionOptimizerInfoSetAttribute`. Allowlist the payload types
and assert all four names in `REQUIRED_DECLARATIONS`; bindgen includes the
element types transitively.
These payload types do not require additional function-manifest entries.

The manifest also selects `cutensornetContractionOptimizerInfoSetAttribute`
and `cutensornetNetworkSetOptimizerInfo` for importing path/slicing metadata
and attaching it to a network. These declarations and their loader entries
provide the native ABI surface; they do not implement safe plan import or
numerical execution.

**There is no constant allowlist, and adding one would not help.** Every
constant this crate uses is an enumerator, and bindgen emits enumerators as
`<type>_<VARIANT>` once the enclosing _type_ is reachable — including
transitively, which is how all 22 `cutensornetStatus_t_CUTENSORNET_STATUS_*`
values arrive without `cutensornetStatus_t` appearing in `TYPE_PATTERN` at all.
So allowlist the **type**, then name the mangled variant in
`REQUIRED_DECLARATIONS` if you want its presence enforced. That assertion is the
part with teeth.

The reduced pass therefore passes no `--allowlist-var`. The full reference pass
still does, because its output is checked against `REFERENCE_SHA256` as the
pinned-toolchain test and must not change.

## Adding a family

`family` selects which generated file a symbol's declarations land in. To add a
new one, add it to `FAMILY_ORDER` in `src/generator.rs` **and** add at least one
manifest row naming it, in the same change: a family with no rows fails with
`EmptyFamily`, and a row naming an unlisted family fails with `UnknownFamily`,
so the two cannot drift apart.

Append new families rather than inserting them. Generated output follows
`FAMILY_ORDER`, so appending keeps the diff to additions.

## What this does not cover: cudart

The manifest describes **cuTensorNet only**. The twelve cudart symbols the crate
also resolves are still hand-maintained:

|                                | cuTensorNet                              | cudart                                                                      |
| ------------------------------ | ---------------------------------------- | --------------------------------------------------------------------------- |
| Header &rarr; declarations     | `bindgen` &rarr; `src/bindings/v2_13.rs` | none                                                                        |
| What is checked in             | `pub fn cutensornetX(..) -> ..`          | `src/bindings/cudart_12.rs` &mdash; the function-pointer aliases themselves |
| Where the signature comes from | read from `cutensornet.h`                | transcribed by hand                                                         |
| Struct and `resolve_*` calls   | generated                                | hand-written in `library.rs`                                                |

This is a **bindings** gap, not a generator gap. The generator earns its keep by
deriving each signature from a machine-read header; for cudart there is no
`pub fn cudaMalloc(..)` declaration anywhere to derive from or check against, so
pointing the generator at it today would give it nothing to read.

Closing the gap is deliberately not done, because twelve symbols have been
stable across CUDA 12.x and nothing planned adds to them. It is worth doing the
moment a new cudart symbol is needed or the crate moves off CUDA 12. It would
take:

1. A bindgen pass over `cuda_runtime_api.h` producing real declarations, with a
   tight allowlist &mdash; that header is far larger than `cutensornet.h`.
2. Manifest rows for the twelve symbols, plus a column naming the library.
3. Replacing the hand-written `CudaError` and `CudaMemcpyKind` aliases with the
   bindgen types, which touches the call sites that use them.
4. Four cuTensorNet assumptions in `src/generator.rs` becoming parameters: the
   family list, the `v2_13::` prefix applied by `qualify`, the output paths, and
   the `CuTensorNetFunctions` / `CUTENSORNET_NAME` /
   `resolve_cutensornet_functions` names emitted by `render_root`. The model and
   serializer are already separate, so this is contained.

Only step 4 touches the generator; the rest is bindings work.

## Regenerating the bindings (pinned x86-64 host)

The input archive is NVIDIA's compressed cuQuantum SDK distribution for
Linux x86-64/CUDA 12. Its C headers describe the API; native library binaries
contain NVIDIA's implementation. Generation uses only the headers, not those
binaries: the script extracts `*/include/cutensornet.h` and its companion
`*/include/cutensornet/*` files into a temporary directory. It reads dependent
CUDA headers from the separately installed CUDA include directory.

```sh
./scripts/generate-bindings.sh /path/to/cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz \
    src/bindings/v2_13.rs
```

The script invokes bindgen, which uses Clang to parse C declarations and produce
the selected Rust types, constants and `extern "C"` function declarations.
It does not translate NVIDIA's implementation into Rust, compile cuTensorNet,
resolve symbols from a `.so`, or run a contraction.

The script refuses to run unless the environment matches what the checked-in
output was produced with: `bindgen 0.72.1`, `Ubuntu clang version
14.0.0-1ubuntu1.1`, CUDA 12.9 headers under
`/usr/local/cuda-12.9/targets/x86_64-linux/include`, and the pinned archive and
`cutensornet.h` SHA-256 values. It also generates the _full_ unrestricted
surface, pins its hash, generates the reduced surface twice to prove
determinism, normalises formatting to Rust edition 2024, and verifies the
selected function set against the manifest before replacing the output.

The host matters because the script passes no `--target` triple to Clang:
bindgen inherits its host ABI. Use the pinned Linux x86-64 environment, which may
be a build machine or VM without a GPU. A machine merely capable of running
NVIDIA software does not necessarily have the matching generation prerequisites.

**Byte-for-byte reproducibility remains required.** Once generated output is
committed, regenerating with the same manifest, type allowlist and pinned
generation inputs must reproduce that file byte for byte. An intentional
type-allowlist change produces a new output to review and commit, even when the
function manifest is unchanged. The two-generation byte comparison and the
pinned full-reference hash check remain mandatory.

## Regenerating the loader (any host)

```sh
cargo run -p qdk_cutensornet --bin generate-loader             # rewrite the generated files
cargo run -p qdk_cutensornet --bin generate-loader -- --check  # fail if stale or edited
```

Cargo builds and runs the `generate-loader` executable in the `qdk_cutensornet`
workspace package. This is an explicit code-generation command, not a build
hook or a request to run the simulator.

The executable reads `scripts/cutensornet-symbols.txt` and
`src/bindings/v2_13.rs`. It derives function-pointer types from those Rust
signatures, then generates the function-table fields and required/optional
symbol-resolution code. No CUDA, bindgen, SDK archive, native NVIDIA library or
GPU is needed. This step can run on a different host from header generation
once the matching manifest and bindings are available there.

Generating lookup code does not perform the lookups. Later, when QDK explicitly
loads the native library, that generated code resolves function addresses and
reports missing required symbols. Runtime library loading and numerical GPU
execution are separate from both generation steps.

The generator emits unformatted source and pipes it through `rustfmt`; it never
tries to predict how rustfmt will lay the file out. `checked_in_loader_matches_freshly_generated_output`
formats freshly rendered text the same way and compares it byte for byte, so
drift is caught without emulating the formatter.

## Validating the FFI surface and GPU behavior

`mod library` is gated to linux/x86_64, so on any other machine its tests do not
fail — they silently do not exist. Cross-platform generator tests still run,
but passing them does not establish that the native resolver tests ran.
`validate-on-cuda-host.sh` closes that gap:

```sh
scripts/validate-on-cuda-host.sh                      # the FFI surface, fast
scripts/validate-on-cuda-host.sh --archive <archive>  # also regenerate and diff the bindings
scripts/validate-on-cuda-host.sh --qualification      # also run the slow A100 suite
scripts/validate-on-cuda-host.sh --skip-hardware      # CUDA host without a usable library
```

Run it from any directory on the supported host; it locates the crate relative
to the script. It refuses to run on a non-x86_64 host rather than reporting a
misleading pass, then checks:

1. The generated loader is current and unedited.
2. `cargo fmt --check` and `cargo clippy --all-targets -D warnings`.
3. `cargo test`, **and** that `library::tests::*` actually appeared in the
   output — the property that matters is that the gated modules compiled and
   ran, not how many tests there were. Expected counts are deliberately not
   asserted; they go stale as tests move between modules and produce false
   failures.
4. That `libcutensornet.so.2` and `libcudart.so.12` are present, falling back to
   the `QDK_CUTENSORNET_LIBRARY` / `QDK_CUDART_LIBRARY` overrides when they live
   somewhere other than the reference paths.
5. `cargo test --test availability -- --ignored`, which resolves every required
   symbol against the real library — the only mechanical proof the manifest's
   symbol names exist.
6. With `--archive`, that `generate-bindings.sh` reproduces `src/bindings/v2_13.rs`
   byte for byte. Self-validating, so it needs no hash pinned in this script.

The host ABI, native libraries and GPU requirements are independent:

| Step                                                  | x86-64 | The real `.so` | A GPU   |
| ----------------------------------------------------- | ------ | -------------- | ------- |
| `generate-bindings.sh` &rarr; `src/bindings/v2_13.rs` | yes    | no             | **no**  |
| `generate-loader` and its tests                       | **no** | no             | no      |
| `library::tests::*` (resolver tests, `FakeResolver`)  | yes    | no             | **no**  |
| `tests/availability.rs`                               | yes    | yes            | **no**  |
| `replay/qualification.rs` (7 `#[ignore]`d)            | yes    | yes            | **yes** |

Only the last row needs a GPU. Native-library availability checks need the
installed `.so` files; header generation and fake-resolver tests do not.
Loader generation and its tests need neither NVIDIA libraries nor an x86-64
host.

The seven `#[ignore]`d A100 tests in `replay/qualification.rs` are **not** run by
default. They are numerical qualification runs — expensive and requiring a real
GPU — so they answer "does the simulation still produce the right numbers", not
"is the FFI surface intact". Each one sweeps its parameters from a table pinned
in the test body, so running them takes no configuration.
Passing symbol resolution does not establish numerical correctness. These runs
remain a separate, opt-in gate: pass `--qualification` when validating simulation
behavior.

Nothing about a particular transfer workflow — bundle hashes, clone URLs, commit
ranges — belongs in this script; that would go stale on the next commit.
If a workflow uses a temporary delivery wrapper, it should call the existing
generators and validator rather than introduce another symbol inventory or
generation implementation. Moving inputs between hosts and collecting evidence
are surrounding workflow steps, not part of translating headers or generating
the loader.

## What the guards catch

| Guard                                                                                         | Catches                                                                                                                                                                      |
| --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `generate-loader --check`                                                                     | A hand-edited or stale generated loader file.                                                                                                                                |
| Manifest symbol missing from bindings (generation)                                            | A manifest row added without regenerating the bindings.                                                                                                                      |
| `checked_in_loader_matches_freshly_generated_output`                                          | The same, from `cargo test` on any host, including non-x86_64 where the loader does not compile. Byte-exact, so it also catches a stale or hand-edited signature.            |
| `required_symbols_are_declared_in_bindings_and_resolved_by_the_loader`                        | A manifest row added without regenerating the bindings, from `cargo test`.                                                                                                   |
| `generator::tests::*`                                                                         | Every rejected manifest or bindings shape, and the model the serializer is fed.                                                                                              |
| `only_the_last_error_helper_is_optional`                                                      | A symbol made `optional` — which weakens discovery — without that being a deliberate, reviewed change.                                                                       |
| `discovers_audited_native_libraries_without_gpu_work` (`tests/availability.rs`, `#[ignore]`d) | A required symbol absent from the real `libcutensornet.so.2` — `discover()` resolves the whole required set. Needs the native libraries: `scripts/validate-on-cuda-host.sh`. |

Every guard above verifies that the surface we _asked_ for was delivered
consistently. None of them can tell you a symbol is missing from the manifest in
the first place — the upstream library exports considerably more than this crate
binds, and widening that surface is a deliberate act.

## TODO: fetch the SDK by version instead of by path

`generate-bindings.sh` takes a path to an archive you already have, and the
hashes it checks were transcribed by hand. NVIDIA publishes a machine-readable
`redistrib_<version>.json` for both cuQuantum and CUDA, listing every archive
with its SHA-256 &mdash; that is where the pinned artifact hash came from, and it
still matches. Reading the manifest instead of transcribing it would let the
script take a version rather than a file, and would make moving to a new SDK a
version bump plus a regeneration.

This is the generation half of the guided tool sketched under **TODO: a guided
environment tool** in the crate README. Not started; nothing depends on it.

Note that this removes a manual download, not the environment requirement: the
pinned clang build and the x86-64 ABI are still needed, because the script
passes no `--target` triple and so inherits the host ABI.
