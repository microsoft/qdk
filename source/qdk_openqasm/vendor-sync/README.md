# Vendored file provenance

`qdk_openqasm` is meant to build and publish on its own, without depending on the
compiler crates in this repository. Several files it needs already existed in
`qsc_data_structures` and `index_map`, so the crate carries its own copies rather
than a path dependency. This directory records where each copy came from, detects
when a copy and its origin fall out of step, and copies the origins back over the
copies on request.

Six of the eight are byte-for-byte copies. Keeping them exact is what makes this
tooling small: syncing is a file copy and detecting drift is a file comparison,
with no per-file rules to maintain. The crate does not need everything those files
contain, but trimming them to fit would turn every future sync into a manual port,
which costs more than carrying the remainder.

The copies are not required to stay identical forever. A boundary conversion in
`source/compiler/qsc_openqasm_compiler/src/parser_types.rs` already translates
between the two sides, so they are decoupled at runtime and divergence is a
supported outcome. What this tooling prevents is divergence that nobody noticed
and nobody chose.

## Files here

| File                   | Purpose                                                                       |
| ---------------------- | ----------------------------------------------------------------------------- |
| `manifest.json`        | The eight vendored files, their origins, and which ones are forks             |
| `check_vendor_sync.py` | Fails when a copy no longer matches its origin, or when a fork's origin moved |
| `copy_vendored.py`     | Copies origins over the copy-tier files, for a maintainer to review           |

`check_vendor_sync.py` runs from `./build.py` as part of the check step, so
`./build.py --no-check` skips it. Nothing runs `copy_vendored.py` for you.

## Drift tiers

**Copy.** `index_map.rs`, `display.rs`, `display/core.rs`, `span.rs`, `source.rs`,
and `source/tests.rs` are exact copies. The check compares bytes, so these need no
recorded hash and nothing to re-pin.

**Fork.** `error.rs` and `error/tests.rs` are not copies that drifted. They were
deliberately rewritten: span resolution is fallible here rather than panicking, and
`SourceSnapshotSourceCode` is new, so diagnostics can render against a parser
source snapshot. Copying the origin over them would throw that away, so
`copy_vendored.py` refuses to write them. Port an origin change into them by hand,
or not at all. Because there is nothing to compare against, the manifest pins the
origin hash each fork was last reviewed at.

## What the crate publishes

The vendored modules are private. `src/span.rs` and `src/source.rs` name the
individual vendored items the crate publishes, so a copy can carry items that stay
internal:

- `qdk_openqasm::span` publishes `Span` and `WithSpan`, and is nothing but those
  two re-exports. The vendored file also has `PackageSpan`, which is not
  published, because nothing here has a package to qualify a span with and the
  type would invite a meaning it does not have.
- `qdk_openqasm::source` publishes `Source`, `SourceContents`, `SourceMap`,
  `SourceName`, and `longest_common_prefix` from the vendored file. The rest of
  that module is the crate's own: the `line_column` coordinate types have no
  vendored origin and nothing here governs them.
- `qdk_openqasm::error::SourceSnapshotSourceCode` comes from the forked `error`
  module.

`display` and `index_map` are crate-private in full. Because the vendored modules
carry items with no caller here, `vendor.rs` allows `dead_code` for the whole
subtree; that allow is what lets the copies stay exact.

Adding a published item is an edit to a wrapper, not to a vendored file. An origin
change to `span.rs` or `source.rs` can still change the crate's public API, which
is why the check reports whether a drifted pair is published.

## When the check fails

A failure means a pair moved. That is common and usually harmless: the origins are
live compiler files. The report names the pair, whether it is published, and what
to do.

- **A copy no longer matches its origin, and the origin is what changed.** Run
  `python3 source/qdk_openqasm/vendor-sync/copy_vendored.py --file <name>`, read
  the diff, and run the crate's tests.
- **A copy no longer matches its origin, and the copy is what changed.** The edit
  belongs in the origin. Move it there and copy back, so both crates get it. If it
  genuinely cannot apply to the origin, promote the pair to a fork in
  `manifest.json` and say why. Do not copy, because that would revert the edit.
- **A fork's origin moved.** Nothing is copied. Read the origin's change, port it
  by hand if it applies, then re-pin with
  `python3 source/qdk_openqasm/vendor-sync/check_vendor_sync.py --update`.

Resist re-pinning a fork without reading the origin's change. The pin is only worth
having if `--update` means someone looked.

## Copying

```bash
python3 source/qdk_openqasm/vendor-sync/copy_vendored.py --dry-run
python3 source/qdk_openqasm/vendor-sync/copy_vendored.py
python3 source/qdk_openqasm/vendor-sync/copy_vendored.py --file span.rs
```

Copying is deliberately a maintainer's decision, not a build step. It rewrites
files that two of the crate's public modules re-export from, so running it
automatically, or as a side effect of a check, would let the crate's API change
without anyone reading the diff. Review the diff before you commit it.
