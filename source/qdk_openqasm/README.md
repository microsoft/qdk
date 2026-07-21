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

Resolver paths are logical identifiers, not proof that a file exists on disk.
Relative include paths are joined to the including source's logical parent and
`.` or `..` components are normalized before `SourceResolver::resolve` is
called. URI-like schemes are preserved, but the resolver does not otherwise
parse, decode, fetch, or canonicalize URIs. Matching in
`InMemorySourceResolver` is exact and case-sensitive.

The resolver context currently stores include-graph state for one top-level
parse. Create a fresh resolver for each call to `parse_source` or
`analyze_source`; resolver-session reuse is not yet supported. Results do not
borrow from the resolver. They retain an immutable source snapshot containing
the entry source, resolved includes, unresolved placeholders, and aliases.

`stdgates.inc` and `qelib1.inc` are recognized internally without consulting
the caller's resolver. Other includes are resolved only through the supplied
resolver. The crate does not fall back to the filesystem.

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

### Lossless tokens

`tokens::tokenize` eagerly returns an owned `Vec<RawToken>`. Each token owns its
text and metadata, so the returned tokens do not borrow the input string. Token
spans are source-local, half-open UTF-8 byte ranges because tokenization handles
one source and does not resolve includes:

```rust
use qdk_openqasm::tokens::{RawTokenKind, tokenize};

let tokens = tokenize("qubit q;");
assert_eq!(tokens[0].kind, RawTokenKind::Identifier);
assert_eq!(tokens[0].text, "qubit");
assert_eq!((tokens[0].span.lo, tokens[0].span.hi), (0, 5));
```

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

### Source snapshots and coordinates

Syntax and semantic node spans and diagnostic label spans from parse or
analysis results are global, half-open UTF-8 byte ranges. Resolve them through
the result's `source_map` or `source_snapshot`; do not slice the entry source
with a global included-file span. Files are assigned non-overlapping global
ranges in snapshot order, separated by one byte in the global coordinate
space. `source_snapshot` owns shared `Arc<str>` text, so it remains valid for
the lifetime of the result without borrowing the resolver or caller input.

Raw-token spans are the exception: `tokens::tokenize` does not build a
multi-source document, so its spans are local to the tokenized string.

### Canonical serialization

`unparse::unparse` returns canonical source as a `Result<String,
UnparseError>`. `unparse::write` streams the same format to an `io::Write`
sink. Both reject recovered syntax, unsupported syntax, invalid strings, and
non-finite floating-point spellings. `write` also returns
`UnparseError::Write` if the sink fails; it does not panic or silently truncate
output:

```rust
use qdk_openqasm::{io::InMemorySourceResolver, parse_source, unparse};

let result = parse_source(
    "OPENQASM 3.0; qubit q;",
    "main.qasm",
    None::<&mut InMemorySourceResolver>,
);
let source = unparse::unparse(result.source.program().expect("program is retained"))?;
assert_eq!(source, "OPENQASM 3.0;\nqubit q;\n");
# Ok::<(), unparse::UnparseError>(())
```

### Resource limits

The current public API does not expose configurable source-size, source-count,
diagnostic-count, expansion, include-depth, or syntax-depth limits. It also
does not install depth guards around every recursive parser, analyzer, visitor,
or serializer path. Callers that process untrusted or adversarial input must
enforce suitable input and nesting limits before calling this crate. These
constraints are current behavior, not configurable limit guarantees.

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
