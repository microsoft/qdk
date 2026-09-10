# cuTensorNet FFI generation

Everything under `src/bindings/` and `src/library/symbols{,/*}.rs` is generated.
Never hand-edit those files: `generate-loader --check` and the `cargo test`
cross-checks exist specifically to reject that, and a hand-edit is silently lost
the next time anyone regenerates.

## What lives here

| File / target              | Role                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------ |
| `cutensornet-symbols.txt`  | The symbol manifest. Single source of truth for **which functions** the crate binds. |
| `generate-bindings.sh`     | Header &rarr; `src/bindings/v2_13.rs`. Requires a CUDA host.                         |
| `src/generator.rs`         | Manifest + bindings &rarr; the loader model &rarr; source text. Pure, unit-tested.   |
| `--bin generate-loader`    | The I/O wrapper around `src/generator.rs`. Runs anywhere.                            |
| `validate-on-cuda-host.sh` | Runs the checks that only a CUDA x86_64 host can run. Copy to the GPU host and run.  |

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
2. Regenerate the bindings **on a CUDA host** (see below). This is required even
   though the header is unchanged: the new symbol has no declaration until the
   allowlist widens, and `cargo test` fails until it does.
3. Run `cargo run -p qdk_cutensornet --bin generate-loader` and commit the result.
4. Write the safe wrapper by hand — the generator stops at the raw pointer.

That row is the only place the symbol is named. The function-pointer alias, the
`CuTensorNetFunctions` field, the `resolve_*` call, the bindgen allowlist and the
required-symbol inventory the tests assert against are all derived from it, so
there is no second list to update and no way for two of them to disagree.

## Adding a type or a constant

The manifest covers functions only. Types, enum variants and constants are still
hand-maintained in `generate-bindings.sh`:

- `TYPE_PATTERN` — bindgen `--allowlist-type`.
- `CONSTANT_PATTERN` — bindgen `--allowlist-var`.
- `REQUIRED_DECLARATIONS` — names asserted to be present in the output.

bindgen pulls in types reachable from an allowlisted function signature, so a
type only needs listing when nothing in the surface references it. A type
reached only through a `void *` attribute buffer — `cutensornetComputeType_t`,
for instance — is _not_ pulled in transitively and must be named explicitly.
Adding to these lists requires regenerating the bindings.

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

## Regenerating the bindings (CUDA host only)

```sh
./scripts/generate-bindings.sh <path>/cuquantum-linux-x86_64-26.06.0.17_cuda12-archive.tar.xz \
    src/bindings/v2_13.rs
```

The script refuses to run unless the environment matches what the checked-in
output was produced with: `bindgen 0.72.1`, `Ubuntu clang version
14.0.0-1ubuntu1.1`, CUDA 12.9 headers under
`/usr/local/cuda-12.9/targets/x86_64-linux/include`, and the pinned archive and
`cutensornet.h` SHA-256 values. It also generates the _full_ unrestricted
surface, pins its hash, generates the reduced surface twice to prove
determinism, normalises formatting to Rust edition 2024, and verifies the
selected function set against the manifest before replacing the output.

Because the reduced surface is derived from the manifest, regenerating without a
manifest change must reproduce the committed file byte for byte.

## Regenerating the loader (any host)

```sh
cargo run -p qdk_cutensornet --bin generate-loader             # rewrite the generated files
cargo run -p qdk_cutensornet --bin generate-loader -- --check  # fail if stale or edited
```

No CUDA, bindgen or archive needed — signatures are read from the checked-in
`src/bindings/v2_13.rs`, so the function-pointer types are transcribed from the
same header the declarations came from rather than by hand.

The generator emits unformatted source and pipes it through `rustfmt`; it never
tries to predict how rustfmt will lay the file out. `checked_in_loader_matches_freshly_generated_output`
formats freshly rendered text the same way and compares it byte for byte, so
drift is caught without emulating the formatter.

## Validating on the GPU host

`mod library` is gated to linux/x86_64, so on any other machine its tests do not
fail — they silently do not exist. A green `cargo test` on a dev box therefore
says nothing about the loader. `validate-on-cuda-host.sh` closes that gap:

```sh
scripts/validate-on-cuda-host.sh                      # the FFI surface, fast
scripts/validate-on-cuda-host.sh --archive <archive>  # also regenerate and diff the bindings
scripts/validate-on-cuda-host.sh --qualification      # also run the slow A100 suite
scripts/validate-on-cuda-host.sh --skip-hardware      # CUDA host without a usable library
```

Copy the checkout (or just this crate) to the GPU host and run it from anywhere;
it validates the tree it lives in and writes nothing outside it. It refuses to
run on a non-x86_64 host rather than reporting a misleading pass, then checks:

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

The seven `#[ignore]`d A100 tests in `replay.rs` are **not** run by default. They
are numerical qualification runs — expensive, requiring a real GPU, and some are
steered by `QDK_CUTENSORNET_*` environment variables — so they answer "does the
simulation still produce the right numbers", not "is the FFI surface intact".
A manifest or loader change cannot plausibly pass step 5 and fail them for a
reason worth blocking on, and folding them in would turn a two-minute check into
a long one. Pass `--qualification` when you do want them.

Nothing about a particular transfer workflow — bundle hashes, clone URLs, commit
ranges — belongs in this script; that would go stale on the next commit.

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
