"""Audit reports."""

from dataclasses import dataclass, field
from pathlib import Path

from ._diagnostic import Diagnostic, Severity


@dataclass(frozen=True)
class Report:
    diagnostics: tuple[Diagnostic, ...] = field(default_factory=tuple)

    @property
    def ok(self) -> bool:
        return not self.errors

    @property
    def errors(self) -> tuple[Diagnostic, ...]:
        return tuple(
            item for item in self.diagnostics if item.severity is Severity.ERROR
        )

    @property
    def warnings(self) -> tuple[Diagnostic, ...]:
        return tuple(
            item for item in self.diagnostics if item.severity is Severity.WARNING
        )

    @property
    def informational(self) -> tuple[Diagnostic, ...]:
        return tuple(
            item for item in self.diagnostics if item.severity is Severity.INFO
        )

    def by_rule(self) -> dict[str, tuple[Diagnostic, ...]]:
        grouped: dict[str, list[Diagnostic]] = {}
        for diagnostic in self.diagnostics:
            grouped.setdefault(diagnostic.rule, []).append(diagnostic)
        return {key: tuple(items) for key, items in grouped.items()}

    def by_artifact(self) -> dict[str, tuple[Diagnostic, ...]]:
        grouped: dict[str, list[Diagnostic]] = {}
        for diagnostic in self.diagnostics:
            grouped.setdefault(diagnostic.where, []).append(diagnostic)
        return {key: tuple(items) for key, items in grouped.items()}

    def __str__(self) -> str:
        if not self.diagnostics:
            return "audit: ok (no diagnostics)"
        lines = []
        for diagnostic in (*self.errors, *self.warnings):
            lines.append(f"[{diagnostic.severity.name}] {diagnostic.rule}")
            location = diagnostic.source_location
            if location is not None:
                display_path = str(location.path)
                try:
                    relative = location.path.relative_to(Path.home())
                    display_path = f"~/{relative.as_posix()}"
                except (ValueError, RuntimeError):
                    pass
                lines.append(f"{display_path}:{location.line}")
            lines.extend((diagnostic.where, diagnostic.summary))
            lines.extend(f"    {line}" for line in diagnostic.detail.splitlines())
            lines.append("")
        lines.append(
            f"audit: {len(self.errors)} error(s), "
            f"{len(self.warnings)} warning(s), "
            f"{len(self.informational)} informational"
        )
        return "\n".join(lines)


__all__ = ["Report"]
