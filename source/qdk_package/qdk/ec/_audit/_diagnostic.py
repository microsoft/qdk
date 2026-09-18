"""Audit diagnostic values and phases."""

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path

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

    def __str__(self) -> str:
        lines = [f"[{self.severity.name}] {self.rule}"]
        location = self.source_location
        if location is not None:
            display_path = str(location.path)
            try:
                relative = location.path.relative_to(Path.home())
                display_path = f"~/{relative.as_posix()}"
            except (ValueError, RuntimeError):
                pass
            lines.append(f"{display_path}:{location.line}")
        lines.extend((self.where, self.summary))
        lines.extend(f"    {line}" for line in self.detail.splitlines())
        return "\n".join(lines)


Severity = Diagnostic.Severity


__all__ = ["Diagnostic", "Phase"]
