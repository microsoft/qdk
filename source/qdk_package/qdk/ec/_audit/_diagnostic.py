"""Audit diagnostic values and phases."""

from dataclasses import dataclass, field
from enum import Enum

from qodec import SourceLocation


class Phase(Enum):
    STRUCTURAL = "structural"
    SEMANTIC = "semantic"
    INFORMATIONAL = "informational"


@dataclass(frozen=True)
class Diagnostic:
    class Severity(Enum):
        INFO = "info"
        WARNING = "warning"
        ERROR = "error"

    rule: str
    severity: Severity
    summary: str
    where: str
    detail: str = ""
    source_location: SourceLocation | None = field(
        default=None, kw_only=True, compare=False
    )
    _path: str = field(default="", kw_only=True, repr=False, compare=False)


Severity = Diagnostic.Severity


__all__ = ["Diagnostic", "Phase"]
