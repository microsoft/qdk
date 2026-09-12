"""Explicit additional output-sign corrections shared by action and fault analysis."""

from collections.abc import Mapping

from binar import BitVector
import qodec as qc

from ._analysis.propagation.pauli import Pauli, relabel
from ._analysis.propagation.pauli_remap import encoding_qubit_relocation


class FrameMap:
    """Resolve local correction deltas from circuit bits and literal constants.

    Readout aliases must resolve entirely to those terms. Encoding signs are
    not delta inputs; normal frame transport is determined by the circuit.
    """

    def __init__(self, gadget: qc.Gadget) -> None:
        self.equations: dict[str, tuple[str, ...]] = {}
        self.paulis: dict[str, Pauli] = {}
        for target, equation in gadget.frames.items():
            reference = qc.gadgets.Reference(target)
            if reference.boundary != "out" or reference.encoding_property not in (
                "x",
                "z",
            ):
                raise ValueError(
                    f"frames[{target!r}]: target must be one output logical X/Z sign"
                )
            entry, index = reference.entry, reference.index
            assert entry is not None
            if entry >= len(gadget.outputs):
                raise ValueError(f"frames[{target!r}]: output entry is out of bounds")
            encoding = gadget.outputs[entry]
            operators = getattr(encoding.code, reference.encoding_property)
            if index >= len(operators):
                raise ValueError(f"frames[{target!r}]: logical index is out of bounds")
            path = f"out[{entry}].{reference.encoding_property}[{index}]"
            if path in self.equations:
                raise ValueError(f"frames[{target!r}]: duplicate output sign {path}")
            terms: dict[str, None] = {}
            for term in equation:
                for resolved in self._terms(gadget, term, frozenset()):
                    if resolved in terms:
                        del terms[resolved]
                    else:
                        terms[resolved] = None
            self.equations[path] = tuple(terms)
            dual = (
                encoding.code.z
                if reference.encoding_property == "x"
                else encoding.code.x
            )
            if index >= len(dual):
                raise ValueError(
                    f"frames[{target!r}]: code has no conjugate logical operator"
                )
            self.paulis[path] = relabel(
                Pauli(str(dual[index])), encoding_qubit_relocation(encoding)
            )

    @staticmethod
    def _terms(
        gadget: qc.Gadget,
        reference: qc.gadgets.Reference | int,
        visiting: frozenset[int],
    ):
        if type(reference) is int:
            if reference not in (0, 1):
                raise ValueError("frame constants must be integer bits 0 or 1")
            if reference:
                yield "1"
            return
        if not isinstance(reference, qc.gadgets.Reference):
            raise ValueError("frame terms must be references or integer bits 0 or 1")
        for term in reference.expand():
            if term.kind == "readout":
                if term.index >= len(gadget.readouts):
                    raise ValueError(
                        f"frame reference {term}: readout is out of bounds"
                    )
                if term.index in visiting:
                    raise ValueError(
                        f"frame reference {term}: cyclic readout definition"
                    )
                for dependency in gadget.readouts[term.index].equation:
                    yield from FrameMap._terms(
                        gadget, dependency, visiting | {term.index}
                    )
            elif term.kind == "circuit_readout":
                if term.index >= len(gadget.circuit.readouts):
                    raise ValueError(
                        f"frame reference {term}: circuit readout is out of bounds"
                    )
                yield f"circuit.readouts[{term.index}]"
            elif term.boundary == "in":
                raise ValueError(
                    f"frame reference {term}: incoming signs are not allowed in frame deltas"
                )
            else:
                raise ValueError(
                    f"frame reference {term}: output signs are not available classical bits"
                )

    def evaluate(
        self, values: Mapping[str, BitVector], zero: BitVector
    ) -> dict[str, BitVector]:
        result = {}
        for target, terms in self.equations.items():
            value = zero.copy()
            for term in terms:
                if term not in values:
                    raise NotImplementedError(
                        f"frame input {term} is unavailable for this analysis"
                    )
                value = value ^ values[term]
            result[target] = value
        return result

    def for_probe(
        self, probe: Pauli, values: Mapping[str, BitVector], zero: BitVector
    ) -> BitVector:
        result = zero.copy()
        for target, value in self.evaluate(values, zero).items():
            if not probe.commutes_with(self.paulis[target]):
                result = result ^ value
        return result
