"""Audit rule protocol and filtering."""

from collections.abc import Iterable, Iterator
from typing import Protocol, TYPE_CHECKING, runtime_checkable

from .diagnostic import Diagnostic, Phase
from .severity import Severity

if TYPE_CHECKING:
    import qodec


@runtime_checkable
class Rule(Protocol):
    @property
    def name(self) -> str: ...

    @property
    def severity(self) -> Severity: ...

    @property
    def phase(self) -> Phase: ...

    @property
    def target(self) -> type: ...

    def __call__(
        self, target: object, *, codec: "qodec.Qodec"
    ) -> Iterator[Diagnostic]: ...


def filter_rules(
    rules: Iterable[Rule],
    *,
    target: type | None = None,
    phase: Phase | None = None,
    disabled: Iterable[str] = (),
) -> list[Rule]:
    disabled_set = frozenset(disabled)
    return [
        rule
        for rule in rules
        if rule.name not in disabled_set
        and (target is None or rule.target is target)
        and (phase is None or rule.phase is phase)
    ]


__all__ = ["Rule", "filter_rules"]
