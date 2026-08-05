# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import dataclass
from typing import Any, Callable

from ._architecture import Architecture
from ._isa_enumeration import ISAQuery
from ._results import EstimationTableEntry


@dataclass(frozen=True)
class QECStrategy:
    """Encapsulates QEC-specific behaviour for a target.

    A ``QECStrategy`` bundles together everything that varies between error
    correction schemes: how to enumerate the corresponding ISAs, which
    lattice-surgery and magic-state transforms are required, and the extra
    result columns and assumptions to surface. Instances are defined once as
    module-level constants and combined with an :class:`Architecture` to form a
    :class:`Target`.

    Instances are immutable (``frozen=True``) so they can be safely shared as
    module-level singletons.

    Attributes:
        suffix: Short, machine-friendly identifier for the strategy (e.g.
            ``"sc"``, ``"3aux"``).
        name: Human-readable label for the strategy (e.g.
            ``"Surface Code"``, ``"Lattice Surgery (3aux)"``) for display.
        build_isa_query: Zero-argument factory that constructs the
            :class:`ISAQuery` enumeration tree for this strategy. Stored as a
            callable (rather than a prebuilt query) so the tree is only built
            lazily when :attr:`isa_query` is accessed, and so each access yields
            a fresh, independently consumable tree. Prefer the :attr:`isa_query`
            property over calling this directly.
        needs_lattice_surgery_transform: Whether estimation must insert a
            lattice-surgery transform when lowering the application to this
            strategy's ISA.
        supports_ccx_states: Whether the strategy can consume CCX (Toffoli)
            magic states directly. When ``True``, estimation requests CCX magic
            states from the PSSPC layer and reports a ``num_ccx_states`` column.
        columns: Extra result columns to append to the estimation table,
            each a ``(column_name, extractor)`` pair where ``extractor`` maps an
            :class:`EstimationTableEntry` to the cell value (e.g. code
            distances, cycle times). Stored as a tuple so instances remain
            fully immutable.
        assumptions: Human-readable assumptions specific to this strategy,
            merged with the architecture's assumptions and rendered in report
            output.
    """

    suffix: str
    name: str
    build_isa_query: Callable[[], ISAQuery]
    needs_lattice_surgery_transform: bool = True
    supports_ccx_states: bool = False
    columns: tuple[tuple[str, Callable[[EstimationTableEntry], Any]], ...] = ()
    assumptions: tuple[str, ...] = ()

    @property
    def isa_query(self) -> ISAQuery:
        """Build and return a fresh :class:`ISAQuery` for this strategy.

        Invokes :attr:`build_isa_query` on every access, so each call returns
        a new enumeration tree that can be consumed independently by an
        estimation run.
        """
        return self.build_isa_query()


@dataclass(frozen=True)
class Target:
    """A target is an architecture paired with a QEC strategy.

    It describes a specific physical machine together with the error correction
    and magic state factories needed to create logical instructions. A target
    is the unit passed to estimation: it supplies the architecture, the ISA
    enumeration to search over, and the strategy-specific transforms, columns,
    and assumptions.

    Targets are constructed by pairing a qubit model / architecture with a
    :class:`QECStrategy`. Instances are immutable (``frozen=True``); the
    remaining public surface consists of read-only properties that delegate to
    the wrapped :class:`QECStrategy`.

    Attributes:
        architecture (Architecture): The physical qubit model / architecture
            (e.g. a neutral-atom or superconducting model) this target runs on.
        qec (QECStrategy): The :class:`QECStrategy` describing how logical
            instructions are realised on that architecture.
    """

    architecture: Architecture
    qec: QECStrategy

    @property
    def name(self) -> str:
        """Unique target name, formatted as ``"{architecture}+{suffix}"``."""
        return f"{self.architecture}+{self.qec.suffix}"

    @property
    def isa_query(self) -> ISAQuery:
        """Fresh :class:`ISAQuery` enumeration tree for this target.

        Delegates to :attr:`QECStrategy.isa_query`, so each access returns a
        newly built, independently consumable tree.
        """
        return self.qec.isa_query

    @property
    def needs_lattice_surgery_transform(self) -> bool:
        """Whether estimation must insert a lattice-surgery transform.

        Delegates to :attr:`QECStrategy.needs_lattice_surgery_transform`.
        """
        return self.qec.needs_lattice_surgery_transform

    @property
    def supports_ccx_states(self) -> bool:
        """Whether the strategy can consume CCX (Toffoli) magic states.

        Delegates to :attr:`QECStrategy.supports_ccx_states`.
        """
        return self.qec.supports_ccx_states

    @property
    def columns(self) -> tuple[tuple[str, Callable[[EstimationTableEntry], Any]], ...]:
        """Extra ``(name, extractor)`` result columns for this target.

        Delegates to :attr:`QECStrategy.columns`.
        """
        return self.qec.columns

    @property
    def assumptions(self) -> list[str]:
        """Combined assumptions to surface in report output.

        Concatenates the architecture's assumptions with the QEC strategy's
        assumptions.
        """
        return self.architecture.assumptions + list(self.qec.assumptions)
