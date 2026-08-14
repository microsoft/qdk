# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Map OpenQASM spans to source files, lines, and columns.

Every parse or analysis result has an immutable :class:`SourceDocument` that
contains the entry source and its resolved includes. Nodes, symbols, and
diagnostics use global, half-open UTF-8 byte :class:`~qdk.openqasm.Span` values.
Use the document's :class:`SourceMap` to locate and convert them::

    from qdk.openqasm import parser

    result = parser.parse("OPENQASM 3.0; qubit q;")
    source_range = result.document.source_map.range_from_span(result.program.span)
    print(source_range.start.line, source_range.start.column)

Lines and columns are zero based, ranges are half open, and EOF is a valid
boundary. Choose :attr:`PositionEncoding.UTF8`,
:attr:`PositionEncoding.CODE_POINT`, or :attr:`PositionEncoding.UTF16` when
converting positions for another editor or protocol.

Conversions are strict: invalid boundaries, reversed ranges, unknown sources,
and ranges from another document raise ``ValueError`` instead of being clamped.
Source IDs and ranges are valid only for the document that produced them.
"""

from ._native_syntax import (
    Position,
    PositionEncoding,
    ResolutionStatus,
    SourceDocument,
    SourceFile,
    SourceMap,
    SourceRange,
)

__all__ = [
    "Position",
    "PositionEncoding",
    "ResolutionStatus",
    "SourceDocument",
    "SourceFile",
    "SourceMap",
    "SourceRange",
]
