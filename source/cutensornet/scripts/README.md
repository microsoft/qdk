# cuTensorNet FFI generation

Everything under `src/bindings/` and `src/library/symbols{,/*}.rs` is generated.
Never hand-edit those files: `generate-loader.py --check` and the `cargo test`
cross-checks exist specifically to reject that, and a hand-edit is silently lost
the next time anyone regenerates.

## What lives here

| File                      | Role                                                                                 |
| ------------------------- | ------------------------------------------------------------------------------------ |
| `cutensornet-symbols.txt` | The symbol manifest. Single source of truth for **which functions** the crate binds. |
| `generate-bindings.sh`    | Header &rarr; `src/bindings/v2_13.rs`. Requires a CUDA host.                         |
| `generate-loader.py`      | Manifest + bindings &rarr; `src/library/symbols{,/*}.rs`. Runs anywhere.             |

The two generators consume the same manifest, so the bindgen allowlist and the
dynamic loader cannot disagree about which symbols exist.

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

1. Add a manifest row.
2. Regenerate the bindings **on a CUDA host** (see below). This is required even
   though the header is unchanged: the new symbol has no declaration until the
   allowlist widens, and `cargo test` fails until it does.
3. Run `python3 scripts/generate-loader.py` and commit the result.
4. If the symbol is `required`, add it to `CUTENSORNET_REQUIRED_SYMBOLS` in
   `src/lib.rs`; `manifest_agrees_with_the_required_symbol_inventory` enforces
   this.
5. Write the safe wrapper by hand — the generator stops at the raw pointer.

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
python3 scripts/generate-loader.py           # rewrite the generated files
python3 scripts/generate-loader.py --check   # fail if they are stale or edited
```

No CUDA, bindgen or archive needed — signatures are read from the checked-in
`src/bindings/v2_13.rs`, so the function-pointer types are transcribed from the
same header the declarations came from rather than by hand.

## What the guards catch

| Guard                                                                       | Catches                                                                                                                                                                             |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `generate-loader.py --check`                                                | A hand-edited or stale generated loader file.                                                                                                                                       |
| Manifest symbol missing from bindings (generation)                          | A manifest row added without regenerating the bindings.                                                                                                                             |
| `required_symbols_are_declared_in_bindings_and_resolved_by_the_loader`      | The same, from `cargo test` on any host, including non-x86_64 where the loader does not compile.                                                                                    |
| `manifest_agrees_with_the_required_symbol_inventory`                        | The `lib.rs` inventory drifting from the manifest.                                                                                                                                  |
| `#[ignore]`d A100 qualification tests in `src/library/simulation/replay.rs` | A required symbol absent from the real `libcutensornet.so.2` — `discover()` resolves the whole required set before any test body runs. Needs a GPU host: `cargo test -- --ignored`. |

Every guard above verifies that the surface we _asked_ for was delivered
consistently. None of them can tell you a symbol is missing from the manifest in
the first place — the upstream library exports considerably more than this crate
binds, and widening that surface is a deliberate act.
