"""Built-in audit rules grouped by qodec artifact."""

from collections.abc import Iterator

from ...lint._rule import Rule
from .code import RULES as CODE_RULES
from .gadget import RULES as GADGET_RULES
from .instruction_set import RULES as INSTRUCTION_SET_RULES
from .qodec import RULES as QODEC_RULES


def default_rules() -> Iterator[Rule]:
    yield from INSTRUCTION_SET_RULES
    yield from CODE_RULES
    yield from GADGET_RULES
    yield from QODEC_RULES


__all__ = ["default_rules"]
