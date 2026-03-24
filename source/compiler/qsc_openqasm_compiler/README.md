# Q# QASM Compiler

This crate implements a semantic transformation from OpenQASM to Q#. At a high level it parses the OpenQASM program (and all includes) into an AST. Once this AST is parsed, it is compiled into Q#'s AST.

Once the compiler gets to the AST phase, it no longer cares that it is processing Q#. At this point all input is indistinguishable from having been given Q# as input. This allows us to leverage capability analysis, runtime targeting, residual computation, partial evaluation, and code generation to the input.

## Process Overview

The OpenQASM code is parsed with their own lexer/parser and any errors are returned immediately before we get into further compilation. The OpenQASM parsing library hard-codes the file system into the types and parsing. Additionally, the library panics instead of surfacing `Result`. Because of this, there is a custom layer built to do the parsing so that the file system is abstracted with the `SourceResolver` trait and results with detailed errors are propagated instead of crashing.

While it would be nice to use their semantic library to analyze and type check the code, they are missing many language features that are only available in the AST and the semantic library panics instead of pushing errors.

With the source lexed and parsed, we can begin compilation. The program is compiled to the Q# AST. The OpenQASM ASG would be great to use for program as a validation pass once it is more developed, but for now we try to surface OpenQASM semantic/type errors as we encounter them. It is difficult to determine if a type error is from the AST compilation or from the source program as the implicit/explicit casting and type promotion rules are very complicated.

As we process the AST we map the spans of the OpenQASM statements/expressions/literals to their corresponding Q# AST nodes. Additionally, we map the input source code (and it’s loaded externals) into a SourceMap with entry expressions. At this point we have enough information to hand off the rest of compilation to the Q# compiler.

## Semantics

The two languages have many differences, and insofar as possible, the compilation preserves the source language semantics.

- OpenQASM and Q# have different qubit management semantics.
  - Q# assumes that qubits are in the |0⟩ state when they are allocated.
  - OpenQASM does not make this assumption and qubits start in an undefined state.
  - Q# requires that qubits are reset to the |0⟩ state when released.
  - OpenQASM does not require qubits to be in a specific state at the end of execution.
- Q# does no allow for variables to be uninitialized. All initialization is explicit.
- OpenQASM allows for implicit initialization.
- Q# does not allow for implicit casting or promotion of types. All conversions must be explicit.
- OpenQASM allows for implicit casting and promotion of types following C99 and custom rules.
- Q# does not have unsigned integers or an angle type. All integers are signed.

QIR specific semantic constraints:

- OpenQASM output registers are declared with a fixed size and not all of the indexes may be populated with measurements. In QIR, `Result`s can only ever be acquired through measurement. So if all entries in an output register aren't measured into, a code generation error will occur.

Semantic details

- Gates are implemented as lambda expressions capturing const variables from the global scope.
  - There is an exception when using `@SimulatableIntrinsic`. Those are defined as full local `operation`s as they are not allowed to capture.
  - We can change this in the future by copying `const` value decls into the `gate`/`function` scope, but this would require implementing a lot of inlining and partial evaluation which we already do in the compiler.
- OpenQASM `const` is modeled as Q# immutable bindings. This isn't fully correct as Q# can build `let` bindings with both mutable and immutable values, but as the translation is one way, we can do this mapping and know that any `const` value is assigned to an immutable `let` binding. Anything else isn't guaranteed to be immutable. There are additional semantic checks as well to ensure that const declarations are not initialized to non-const values.
