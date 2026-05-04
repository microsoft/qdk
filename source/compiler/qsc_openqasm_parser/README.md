# OpenQASM 3 Parser

A lexer, parser, and semantic analyzer for [OpenQASM 3](https://openqasm.com/) programs.

## Overview

This crate provides a complete front-end for processing OpenQASM 3 source code. It operates entirely in the OpenQASM domain and produces a typed semantic AST suitable for further compilation or analysis.

The processing pipeline has two stages:

1. **Lexing & Parsing** — Tokenizes and parses OpenQASM 3 source into a syntax tree (`QasmParseResult`)
2. **Semantic Analysis** — Lowers the syntax tree into a semantic AST with type checking, symbol resolution, and const evaluation (`QasmSemanticParseResult`)

### Source Resolution

OpenQASM programs can include other files via `include` statements. This crate abstracts filesystem access behind the `SourceResolver` trait, enabling:

- In-memory source resolution for editors and notebooks
- Custom filesystem backends
- Include cycle detection

### OpenQASM Standard Library Types

The crate includes implementations of OpenQASM-native types:

- `Angle` — Fixed-point angle representation
- `Complex` — Complex number type
- `Duration` — Timing duration type

## Usage

Parse and semantically analyze an OpenQASM 3 program:

```rust
use qsc_openqasm_parser::semantic;

let source = r#"
    OPENQASM 3.0;
    include "stdgates.inc";
    qubit[2] q;
    h q[0];
    cx q[0], q[1];
    bit[2] c;
    c = measure q;
"#;

let result = semantic::parse(source, "bell.qasm");
if result.has_errors() {
    for error in result.all_errors() {
        eprintln!("{error}");
    }
} else {
    println!("parsed {} statements", result.program.statements.len());
}
```

## License

MIT
