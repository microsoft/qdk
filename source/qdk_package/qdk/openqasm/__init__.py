# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""OpenQASM tooling for the QDK.

This module provides functions for compiling, running, estimating, and
generating circuits from OpenQASM 2.0/3.0 programs.

The parser, source, semantic, and serialization APIs are in preview and may
change between QDK releases.

The trees are read-only and their nodes are not ``openqasm3.ast`` objects, so
reference-parser code that mutates nodes or depends on ``openqasm3`` types does
not work unchanged.

Key exports:

- :func:`~qdk.openqasm.import_openqasm` — import an OpenQASM program or
  fragment into the active QDK interpreter context.
- :func:`~qdk.openqasm.run` — simulate an OpenQASM program for one or more shots.
- :func:`~qdk.openqasm.compile` — compile an OpenQASM program to QIR for
  submission to a hardware target.
- :func:`~qdk.openqasm.circuit` — synthesize a circuit diagram from an
  OpenQASM program.
- :func:`~qdk.openqasm.estimate` — estimate the quantum resources required to
  run an OpenQASM program (deprecated; use :mod:`qdk.qre` instead).
- :class:`~qdk.openqasm.ProgramType` — controls how the source is interpreted
  (``Operation``, ``File``, or ``Fragments``).
- :class:`~qdk.openqasm.OutputSemantics` — controls measurement output
  semantics during compilation.
- :class:`~qdk.openqasm.QasmError` — raised when an OpenQASM source cannot
  be parsed or compiled.
- :mod:`~qdk.openqasm.parser` — the syntactic AST: :func:`parse` and the
  read-only ``openqasm3``-style node classes it produces.
- :mod:`~qdk.openqasm.semantic` — the resolved semantic AST: :func:`analyze`
  and the richly-typed, clean-named node classes it produces (for example
  :class:`~qdk.openqasm.semantic.QuantumGate` and
  :class:`~qdk.openqasm.semantic.BinaryExpression`).
- :class:`~qdk.openqasm.parser.QASMVisitor` — a read-only visitor base for
  walking either the syntactic or semantic AST.
- :class:`~qdk.openqasm.QASMNode`, :class:`~qdk.openqasm.Expression`,
  :class:`~qdk.openqasm.Statement`, and :class:`~qdk.openqasm.Annotation` —
  the classes both layers use. They live here rather than in either layer's
  module, and are re-exported from both for convenience.
- :class:`~qdk.openqasm.parser.SyntaxNode` and
  :class:`~qdk.openqasm.semantic.SemanticNode` — ask which tree a node came
  from. Most class names appear in both layers, so a value named ``Program``
  or ``IntType`` is otherwise ambiguous. The four shared classes above belong
  to neither and answer ``False`` to both.
- :func:`dumps` and :func:`dump` — canonical OpenQASM source for a whole
  syntactic program. Unlike ``openqasm3.dumps`` they do not accept an
  arbitrary node; the parameter may widen later.
"""

from . import parser, semantic, source
from .parser import (
  QASM3ParsingError,
  QASMUnparseError,
  dump,
  dumps,
  parse_program,
)
from .source import Position, PositionEncoding, SourceRange
from ._circuit import circuit
from ._compile import compile
from ._estimate import estimate
from ._import import import_openqasm
from ._run import run
from .._native import ProgramType, OutputSemantics, QasmError  # type: ignore
from .._native import (  # type: ignore
  Annotation,
  Expression,
  QASMNode,
  Statement,
)

__all__ = [
    "circuit",
    "compile",
    "estimate",
    "import_openqasm",
    "run",
    "dumps",
    "dump",
    "parse_program",
    "QASM3ParsingError",
    "QASMUnparseError",
    "parser",
    "semantic",
    "source",
    "Position",
    "PositionEncoding",
    "SourceRange",
    "QASMNode",
    "Expression",
    "Statement",
    "Annotation",
    "ProgramType",
    "OutputSemantics",
    "QasmError",
]
