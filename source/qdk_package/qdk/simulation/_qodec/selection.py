from collections.abc import Mapping, Sequence
from dataclasses import dataclass
import re

from .protocols import ExecutionRejected, ExecutionUnresolved


@dataclass(frozen=True)
class Selection:
    patterns: tuple[tuple[tuple[int, bool], ...], ...]

    def accepts(self, flags: Sequence[bool | None]) -> bool | None:
        if not self.patterns:
            return True
        unknown = False
        for pattern in self.patterns:
            incomplete = False
            for index, required in pattern:
                value = flags[index]
                if value is None:
                    incomplete = True
                elif value != required:
                    break
            else:
                if not incomplete:
                    return True
                unknown = True
        return None if unknown else False

    def require(self, flags: Sequence[bool | None]) -> None:
        accepted = self.accepts(flags)
        if accepted is False:
            raise ExecutionRejected("Call selection rejected the reported flags")
        if accepted is None:
            raise ExecutionUnresolved(
                "Call selection is unresolved from the reported flags"
            )


def prepare_selection(
    flags: Sequence[str], patterns: Sequence[Mapping[str, int]]
) -> Selection:
    names = {name: index for index, name in enumerate(flags)}
    compiled = []
    for pattern in patterns:
        required_bits = {}
        for name, required in pattern.items():
            position = re.fullmatch(r"flags\[(\d+)\]", name)
            index = int(position.group(1)) if position is not None else names.get(name)
            if index is None or not 0 <= index < len(flags):
                raise ValueError(f"Unknown selection flag {name!r}")
            if type(required) is not int or required not in (0, 1):
                raise ValueError("Selection flag values must be integer bits")
            if index in required_bits and required_bits[index] != bool(required):
                raise ValueError("Conflicting selection values for the same flag")
            required_bits[index] = bool(required)
        compiled.append(tuple(sorted(required_bits.items())))
    return Selection(tuple(compiled))
