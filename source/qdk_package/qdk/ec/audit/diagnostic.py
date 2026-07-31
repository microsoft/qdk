"""Audit diagnostic values and phases."""

from dataclasses import dataclass
from enum import Enum

from .severity import Severity


class Phase(Enum):
    STRUCTURAL = "structural"
    SEMANTIC = "semantic"
    INFORMATIONAL = "informational"


@dataclass(frozen=True)
class Diagnostic:
    rule: str
    severity: Severity
    summary: str
    where: str
    detail: str = ""


__all__ = ["Diagnostic", "Phase"]
