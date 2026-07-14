# `qdk_openqasm`

A standalone lexer, parser, and semantic analyzer for [OpenQASM 3](https://openqasm.com/).

## Overview

This crate provides a complete front-end for processing OpenQASM 3 source code. It operates entirely in the OpenQASM domain and produces a typed semantic AST suitable for further compilation or analysis.

The processing pipeline has two stages:

1. Lexing and parsing tokenizes and parses OpenQASM 3 source into a syntax tree (`ParseResult`).
2. Semantic analysis lowers the syntax tree into a semantic AST with type checking, symbol resolution, and const evaluation (`AnalysisResult`).

### Source Resolution

OpenQASM programs can include other files via `include` statements. This crate abstracts filesystem access behind the `SourceResolver` trait, enabling:

- In-memory source resolution for editors and notebooks
- Custom filesystem backends
- Include cycle detection

### OpenQASM Standard Library Types

The crate includes implementations of OpenQASM-native types:

- `Angle` for fixed-point angle representation
- `Complex` for complex numbers
- `Duration` for timing durations

## Installation

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
qdk_openqasm = "0.0.0"
```

The crate is developed in the [Microsoft Quantum Development Kit repository](https://github.com/microsoft/qdk).

## Usage

### Syntactic parsing

Use `parse_source` for a fast, syntax-only parse. It takes the source text, a
logical path, and an optional `SourceResolver` for `include` directives. Pass
`None` when the program is self-contained:

```rust
use qdk_openqasm::{io::InMemorySourceResolver, parse_source};

let source = "OPENQASM 3.0; qubit q; h q;";
let result = parse_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
assert!(!result.has_errors());
```

Provide an in-memory resolver so `include` statements can be resolved:

```rust
use qdk_openqasm::{io::InMemorySourceResolver, parse_source};

let mut resolver = InMemorySourceResolver::from_iter([(
    "gates.inc".into(),
    "gate my_h q { h q; }".into(),
)]);
let source = "OPENQASM 3.0; include \"gates.inc\"; qubit q; my_h q;";
let result = parse_source(source, "main.qasm", Some(&mut resolver));
assert!(!result.has_errors());
```

### Semantic analysis

`analyze_source` performs the full lowering pipeline (type checking, symbol
resolution, and const evaluation). It shares the same `Option<&mut R>` resolver
argument as `parse_source`. The `stdgates.inc` standard library is resolved
internally, so a self-contained program can pass `None`:

```rust
use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};

let source = "OPENQASM 3.0; include \"stdgates.inc\"; qubit q; h q;";
let result = analyze_source(source, "main.qasm", None::<&mut InMemorySourceResolver>);
assert!(!result.has_errors());
```

Provide an in-memory resolver so custom `include` statements can be resolved. The
built-in `InMemorySourceResolver` maps include paths to their contents, which is
handy for editors, notebooks, and tests:

```rust
use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};

let mut resolver = InMemorySourceResolver::from_iter([(
    "gates.inc".into(),
    "gate my_h q { h q; }".into(),
)]);
let source = concat!(
    "OPENQASM 3.0; ",
    "include \"stdgates.inc\"; ",
    "include \"gates.inc\"; ",
    "qubit q; my_h q;",
);
let result = analyze_source(source, "main.qasm", Some(&mut resolver));
assert!(!result.has_errors());
```

### Walking the semantic program

A successful `analyze_source` exposes the lowered program at `result.program`:

```rust
use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};

let result = analyze_source(
    "OPENQASM 3.0; qubit[2] q; U(0, 0, 0) q[0];",
    "main.qasm",
    None::<&mut InMemorySourceResolver>,
);

for statement in &result.program.statements {
    println!("statement at {:?}", statement.span);
}
```

### Rendering structured diagnostics

Every error is a [`miette`](https://docs.rs/miette) diagnostic
(`WithSource<Error>`). Collect them with `result.all_errors()` and wrap each in a
`miette::Report` to render it. Enable the default-off `fancy` feature to get the
graphical report (source snippets, labels, and help text):

```toml
[dependencies]
qdk_openqasm = { version = "0.0.0", features = ["fancy"] }
```

```rust
use qdk_openqasm::{analyze_source, io::InMemorySourceResolver};

// `h` requires `include "stdgates.inc";`, so this reports a diagnostic.
let result = analyze_source(
    "OPENQASM 3.0; qubit q; h q;",
    "main.qasm",
    None::<&mut InMemorySourceResolver>,
);

for error in result.all_errors() {
    eprintln!("{:?}", miette::Report::new(error));
}
```

## License

MIT
