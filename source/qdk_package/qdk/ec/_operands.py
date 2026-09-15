"""The block labels carried by a qodec instruction-call operand.

Each positional operand names one block, as an integer or a string. Variadic
operands occupy successive entries in the same flat operand list.

A :data:`QubitLabel` is an ``int`` (an authored qubit index) or a ``str`` (a
symbolic label such as the namespaced ``"alice.0"`` that lowering emits). A
label's identity does not depend on the wire form it arrived in: the operand
``3`` and the operand ``"3"`` both name qubit ``3``.

Consumers match on the label type — ``isinstance(label, int)`` — rather than
re-parsing text.
"""

from __future__ import annotations

from typing import Union

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


def qubit_labels(value: int | str) -> list[QubitLabel]:
    """The single block label this positional operand names."""
    return [_as_label(value)]


__all__ = [
    "QubitLabel",
    "qubit_labels",
]
