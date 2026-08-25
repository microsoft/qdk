# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Compile, simulate, and inspect OpenQASM programs.

For execution workflows, use :func:`run` to simulate a program,
:func:`compile` to produce QIR, :func:`circuit` to generate a circuit, or
:func:`import_openqasm` to add declarations to the active QDK context.
For resource estimation, use :mod:`qdk.qre`; :func:`estimate` is deprecated.

For source-analysis workflows, use :func:`parse` to inspect the syntax exactly
as written, or :func:`analyze` to inspect resolved symbols, types, and constant
values. The :mod:`parser` and :mod:`semantic` modules export the corresponding
read-only node classes. :class:`QASMVisitor` walks either tree.

Use :func:`dumps` or :func:`dump` to serialize a syntax program to canonical
OpenQASM. The :mod:`source` module maps node and diagnostic spans back to files,
lines, and columns.

The parsing, semantic-analysis, source, and serialization APIs are in preview
and may change between QDK releases.
"""

from . import parser, semantic, source
from .parser import (
    QASM3ParsingError,
    QASMUnparseError,
    QASMVisitor,
    dump,
    dumps,
    parse,
    parse_program,
)
from .semantic import analyze
from .source import Position, PositionEncoding, SourceRange
from ._circuit import circuit
from ._compile import compile
from ._estimate import estimate
from ._import import import_openqasm
from ._run import run
from .._native import ProgramType, OutputSemantics, QasmError  # type: ignore
from ._native_syntax import (
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
    "analyze",
    "parse",
    "dumps",
    "dump",
    "parse_program",
    "QASM3ParsingError",
    "QASMUnparseError",
    "QASMVisitor",
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
