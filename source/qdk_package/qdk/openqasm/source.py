# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Immutable source documents and strict coordinates for OpenQASM syntax.

``SourceMap`` converts among source-local UTF-8 byte offsets, Unicode code
points, and UTF-16 code units. Lines and columns are zero based, ranges are
half open, and EOF is valid. Invalid boundaries, separator gaps, reversed
ranges, mixed encodings, unknown sources, and cross-source spans raise
``ValueError`` rather than being clamped. ``Position`` and ``SourceRange``
constructors raise ``OverflowError`` when an unsigned 32-bit argument is
negative or greater than ``2**32 - 1``.
"""

from .._native import (  # type: ignore
    Position,
    PositionEncoding,
    SourceDocument,
    SourceEdit,
    SourceFile,
    SourceMap,
    SourceRange,
)

__all__ = [
    "Position",
    "PositionEncoding",
    "SourceDocument",
    "SourceEdit",
    "SourceFile",
    "SourceMap",
    "SourceRange",
]
