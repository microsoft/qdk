"""Mathematical consistency of declared code operators."""

from collections.abc import Iterator
from dataclasses import dataclass
from itertools import combinations

import qodec as qc

from ..._analysis.code_algebra import SubsystemCode
from ..._analysis.propagation.pauli import Pauli
from .._diagnostic import Diagnostic, Phase, Severity

from .._rule import Rule


@dataclass(frozen=True)
class CodeAlgebraRule:
    name: str = "code/invalid-algebra"
    severity: Severity = Severity.ERROR
    phase: Phase = Phase.SEMANTIC
    target: type = qc.Code

    def __call__(self, target: object, *, qodec: qc.Qodec) -> Iterator[Diagnostic]:
        if not isinstance(target, qc.Code):
            raise TypeError(f"expected qodec.Code, got {type(target).__name__}")
        try:
            SubsystemCode.of(target)
        except ValueError as error:
            failure = str(error)
        else:
            return
        operators = [
            (kind, index, text, Pauli(text))
            for kind, values in (
                ("stabilizers", target.stabilizers),
                ("x", target.x),
                ("z", target.z),
            )
            for index, text in enumerate(values)
        ]
        reported = False
        for first, second in combinations(operators, 2):
            first_kind, first_index, first_text, first_pauli = first
            second_kind, second_index, second_text, second_pauli = second
            paired = {first_kind, second_kind} == {
                "x",
                "z",
            } and first_index == second_index
            if first_pauli.commutes_with(second_pauli) != (not paired):
                expected = "anticommute" if paired else "commute"
                yield Diagnostic(
                    self.name,
                    self.severity,
                    f"{first_kind}[{first_index}] and {second_kind}[{second_index}] must {expected}",
                    f"code[{target.name!r}]",
                    f"{first_kind}[{first_index}]: {first_text}\n{second_kind}[{second_index}]: {second_text}",
                )
                reported = True
        if not reported:
            yield Diagnostic(
                self.name,
                self.severity,
                "Code operators are algebraically inconsistent",
                f"code[{target.name!r}]",
                failure,
            )


RULES: tuple[Rule, ...] = (CodeAlgebraRule(),)

__all__ = ["CodeAlgebraRule", "RULES"]
