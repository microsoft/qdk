## Overview

Use two separate workflows from the repository root:

- `qir` is the real libFuzzer target. Run it through `cargo +nightly fuzz` and pass `--fuzz-dir source/fuzz`.
- `qir_matrix` is a deterministic replay binary. Run it through `cargo run -p fuzz --bin qir_matrix`; do not invoke it through `cargo fuzz`.

## Prerequisites

For libFuzzer runs:

```bash
rustup install nightly
cargo install cargo-fuzz
```

For deterministic replay on macOS:

```bash
brew install llvm@14 llvm@15 llvm@16 llvm@21
```

## List Targets

```bash
cargo +nightly fuzz list --fuzz-dir source/fuzz
```

## Run LibFuzzer

Run the real QIR fuzz target against the repo-root corpus:

```bash
cargo +nightly fuzz run --fuzz-dir source/fuzz qir --features do_fuzz -- -runs=200 -max_total_time=30
```

Inspect available libFuzzer options:

```bash
cargo +nightly fuzz run --fuzz-dir source/fuzz qir --features do_fuzz -- -help=1
```

On macOS, if the default AddressSanitizer-backed smoke stalls before the harness starts, use this local diagnostic variant to confirm the corpus and harness path independently:

```bash
cargo +nightly fuzz run --fuzz-dir source/fuzz --sanitizer none qir --features do_fuzz -- -runs=1 -max_total_time=15
```

This no-sanitizer command is a local proof aid only. The default `cargo +nightly fuzz run --fuzz-dir source/fuzz qir --features do_fuzz -- -max_total_time=15` command remains the automation gate, and the repository has not promoted the no-sanitizer variant into CI.

## Run Deterministic Replay

Replay the checked seed corpus across the fast external LLVM matrix:

```bash
cargo run -p fuzz --bin qir_matrix -- --toolchains 14,15,16,21
```

Write replay artifacts to a known directory:

```bash
cargo run -p fuzz --bin qir_matrix -- --toolchains 14,15,16,21 --output-dir /tmp/qir-matrix
```

The replay harness reads `.seed` files from `source/fuzz/corpus/qir`, exports checked text artifacts, and runs `llvm-as`, `opt -passes=verify`, `llvm-dis`, and a second `llvm-as` per lane.

## Notes

- Run all commands from the repository root.
- `qir_matrix` intentionally keeps deterministic replay separate from libFuzzer.
- The current deterministic seed bank includes BaseV1, AdaptiveV1, AdaptiveV2, and BareRoundtrip replay coverage.
- Failures from the real `qir` target are written under `source/fuzz/artifacts/qir/`.
- See [corpus/README.md](corpus/README.md) for corpus layout and seed naming.
