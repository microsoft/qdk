"""Audit diagnostic severity."""

from enum import Enum


class Severity(Enum):
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"


__all__ = ["Severity"]
