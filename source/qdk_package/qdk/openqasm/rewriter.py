# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Transactional entry-source rewriting for immutable OpenQASM syntax trees.

``QASMRewriter`` walks one syntactic ``ParseResult`` and collects explicit
``SourceEdit`` values returned by callbacks. Edits are validated together and
applied to the immutable entry source before a new independent parse snapshot
is published. Punctuation and list splicing are not inferred; callers deleting
or replacing list elements must include separators in an explicit range when
needed.

Rewriting accepts only a syntactic ``ParseResult`` and edits only source ID 0,
the entry source. Include contents are immutable snapshots used for reparsing,
not rewrite targets. Semantic results are rejected. Invalid edit sets raise
``QASMRewriteError`` with ``code``, ``edit_index``, and ``range`` payloads;
callback and iterator exceptions propagate unchanged. Rewriting malformed text
returns a diagnostic-bearing parse result rather than raising a parse error.
"""

from __future__ import annotations

from collections.abc import Iterable
from threading import Lock
from typing import Any, final

from .._native import (  # type: ignore
    ParseResult,
    PositionEncoding,
    QASMNode,
    SourceDocument,
    SourceEdit,
    SourceRange,
    _QASMRewriteError as _NativeQASMRewriteError,
    qasm_apply_edits as _qasm_apply_edits,
)
from ._visitor import QASMVisitor

RewriteReturn = SourceEdit | Iterable[SourceEdit] | None

__all__ = ["QASMRewriteError", "QASMRewriter", "RewriteReturn"]


class QASMRewriteError(ValueError):
    """Raised when a transactional source edit set is invalid.

    Attributes:
        code: Stable machine-readable error code.
        edit_index: Callback collection index of the invalid edit, if known.
        range: Source range associated with the invalid edit, if known.
    """

    __slots__ = ("_code", "_edit_index", "_range")

    def __init__(
        self,
        message: str,
        *,
        code: str,
        edit_index: int | None,
        range: SourceRange | None,
    ) -> None:
        super().__init__(message)
        self._code = code
        self._edit_index = edit_index
        self._range = range

    @property
    def code(self) -> str:
        """Stable machine-readable error code."""
        return self._code

    @property
    def edit_index(self) -> int | None:
        """Collection index of the invalid edit, if known."""
        return self._edit_index

    @property
    def range(self) -> SourceRange | None:
        """Source range associated with the invalid edit, if known."""
        return self._range


class QASMRewriter(QASMVisitor):
    """Collect explicit source edits while visiting one syntax parse result.

    Specialized ``visit_<ConcreteType>`` callbacks may return one
    :class:`SourceEdit`, an iterable of edits, or ``None``. Return iterables are
    consumed eagerly. A specialized callback must call ``generic_visit`` when
    it wants traversal to continue into that node's annotations and children.

    Instances reject nested and concurrent ``rewrite`` calls. They retain no
    parse result, document, node, or edit after a transaction exits.
    """

    __slots__ = (
        "_rewrite_lock",
        "_active_result",
        "_active_document",
        "_active_edits",
    )

    def __new__(cls) -> QASMRewriter:
        instance = super().__new__(cls)
        instance._rewrite_lock = Lock()
        instance._active_result = None
        instance._active_document = None
        instance._active_edits = None
        return instance

    def __init_subclass__(cls, **kwargs: Any) -> None:
        super().__init_subclass__(**kwargs)
        if "visit" in cls.__dict__:
            raise TypeError("QASMRewriter.visit is final and cannot be overridden")

    def rewrite(self, result: ParseResult, /) -> ParseResult:
        """Apply callback edits transactionally and return a new parse snapshot.

        Valid edits always produce a new recovery-oriented ``ParseResult``,
        including edits that introduce malformed syntax or unresolved includes.
        Invalid edit sets raise :class:`QASMRewriteError` before publication.
        Callback and iterator exceptions propagate unchanged.
        """
        if not isinstance(result, ParseResult):
            raise TypeError("rewrite() requires a syntactic ParseResult")
        if not self._rewrite_lock.acquire(blocking=False):
            raise RuntimeError("QASMRewriter is already rewriting")

        try:
            self._active_result = result
            self._active_document = result.document
            self._active_edits = []
            self.visit(result.program)
            try:
                return _qasm_apply_edits(result, self._active_edits)
            except _NativeQASMRewriteError as error:
                raise QASMRewriteError(
                    str(error),
                    code=error.code,
                    edit_index=error.edit_index,
                    range=error.range,
                ) from None
        finally:
            if self._active_edits is not None:
                self._active_edits.clear()
            self._active_result = None
            self._active_document = None
            self._active_edits = None
            self._rewrite_lock.release()

    @final
    def visit(self, node: Any) -> None:
        """Dispatch one callback and eagerly collect its returned source edits."""
        edits = self._active_edits
        if edits is None:
            raise RuntimeError("visit() is only valid during rewrite()")

        method = getattr(self, f"visit_{type(node).__name__}", self.generic_visit)
        returned = method(node)
        edits.extend(_returned_edits(returned))
        return None

    def replace(
        self,
        target: QASMNode | SourceRange,
        replacement: str,
        /,
    ) -> SourceEdit:
        """Create an edit replacing a node or explicit range.

        Node spans include only the node itself. Surrounding punctuation and
        separators are never inferred.
        """
        document = self._active_document
        if document is None:
            raise RuntimeError("replace() is only valid during rewrite()")

        if isinstance(target, QASMNode):
            source_range = document.source_map.range_from_span(
                target.span,
                encoding=PositionEncoding.UTF8,
            )
        elif isinstance(target, SourceRange):
            source_range = target
        else:
            raise TypeError("target must be a QASMNode or SourceRange")
        return SourceEdit(source_range, replacement)

    def delete(self, target: QASMNode | SourceRange, /) -> SourceEdit:
        """Create an edit deleting a node or explicit range."""
        return self.replace(target, "")


def _returned_edits(returned: RewriteReturn | Any) -> list[SourceEdit]:
    if returned is None:
        return []
    if isinstance(returned, SourceEdit):
        return [returned]

    try:
        iterator = iter(returned)
    except TypeError:
        raise TypeError(
            "rewriter callbacks must return SourceEdit, an iterable of "
            "SourceEdit, or None"
        ) from None

    edits = []
    for item in iterator:
        if not isinstance(item, SourceEdit):
            raise TypeError("rewriter callback iterable contains a non-SourceEdit value")
        edits.append(item)
    return edits


