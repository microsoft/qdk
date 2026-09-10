"""Audit diagnostic values and phases."""

from dataclasses import dataclass
from enum import Enum


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


Severity = Diagnostic.Severity


__all__ = ["Diagnostic", "Phase"]
