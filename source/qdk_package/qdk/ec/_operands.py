"""The qubit labels carried by a qodec instruction-call operand.

A block operand names one or more qubits, and qodec's IR renders that naming as
an ``int``, a ``list[int]``, a whitespace-joined ``str``, or a ``list[str]``
depending on how the call was built. "Which qubits does this operand name?" is
therefore a question every compiler, allocator, and walker in ``qdk.ec`` has to
ask, and this module is the one place that answers it.

A :data:`QubitLabel` is an ``int`` (an authored qubit index) or a ``str`` (a
symbolic label such as the namespaced ``"alice.0"`` that lowering emits). A
label's identity does not depend on the wire form it arrived in: the operand
``3`` and the operand ``"3"`` both name qubit ``3``.

Consumers match on the label type — ``isinstance(label, int)`` — rather than
re-parsing text.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Union

import qodec as qc

if TYPE_CHECKING:
    Argument = qc.instructions.InstructionCall.Argument

#: One qubit named by a block operand: an authored index or a symbolic label.
QubitLabel = Union[int, str]


def _as_label(item: object) -> QubitLabel:
    """Normalize one operand element to a label.

    Text that renders an integer exactly becomes that integer, so ``"3"`` and
    ``3`` are the same label. Text that would not survive the round trip (an
    ``"007"``, a ``"+3"``) is kept verbatim.
    """
    if isinstance(item, int) and not isinstance(item, bool):
        return item
    text = str(item)
    try:
        number = int(text)
    except ValueError:
        return text
    return number if str(number) == text else text


def qubit_labels(value: "Argument") -> list[QubitLabel]:
    """The qubit labels ``value`` names, in order.

    An ``int`` names one qubit, a ``list`` one per element, and a ``str`` one
    per whitespace-separated token.
    """
    if isinstance(value, str):
        return [_as_label(token) for token in value.split()]
    if isinstance(value, list):
        return [_as_label(item) for item in value]
    return [_as_label(value)]


__all__ = [
    "QubitLabel",
    "qubit_labels",
]
