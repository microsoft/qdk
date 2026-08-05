# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import FrozenInstanceError, dataclass
from typing import Any, cast

import pytest

from qdk.qre import (
    PHYSICAL,
    Architecture,
    QECStrategy,
    QUBIT_MODELS,
    Target,
    TechnologyFamily,
    qubit,
)
from qdk.qre._architecture import ISAContext
from qdk.qre._instruction import ISA
from qdk.qre._isa_enumeration import ISAQuery
from qdk.qre.instruction_ids import CNOT, MEAS_Z


def _dummy_query() -> ISAQuery:
    """Return a placeholder ISAQuery for tests that never consume it."""
    return cast(ISAQuery, object())


@pytest.fixture(autouse=True)
def _restore_registry():
    """Snapshot and restore the global QUBIT_MODELS registry around each test."""
    saved = dict(QUBIT_MODELS)
    try:
        yield
    finally:
        QUBIT_MODELS.clear()
        QUBIT_MODELS.update(saved)


# ---------------------------------------------------------------------------
# @qubit decorator
# ---------------------------------------------------------------------------


def test_qubit_bare_decorator():
    """A bare @qubit (no parentheses) produces an Architecture subclass."""

    @qubit
    class BareModel:
        pass

    assert issubclass(BareModel, Architecture)
    assert isinstance(BareModel(), Architecture)


def test_qubit_default_name_and_family():
    """@qubit defaults the display name to the class name and family to UNKNOWN."""

    @qubit
    class DefaultNameModel:
        pass

    assert str(DefaultNameModel()) == "DefaultNameModel"
    assert DefaultNameModel().family is TechnologyFamily.UNKNOWN


def test_qubit_family_and_name_override():
    """The family keyword and name argument are applied to the generated class."""

    @qubit(name="Fancy", family=TechnologyFamily.SUPERCONDUCTING)
    class RenamedModel:
        pass

    assert str(RenamedModel()) == "Fancy"
    assert RenamedModel().family is TechnologyFamily.SUPERCONDUCTING


def test_qubit_auto_generated_provided_isa():
    """Instruction-dict attributes are turned into an auto-generated ISA."""

    @qubit(family=TechnologyFamily.SUPERCONDUCTING)
    class AutoIsaModel:
        CNOT = {"arity": 2, "time": 100, "error_rate": 1e-3, "encoding": PHYSICAL}
        MEAS_Z = {"arity": 1, "time": 50, "error_rate": 1e-3, "encoding": PHYSICAL}

    model = AutoIsaModel()
    isa: ISA = model.provided_isa(model.context())

    assert CNOT in isa
    assert MEAS_Z in isa
    assert len(isa) == 2
    assert isa[CNOT].time() == 100
    assert isa[MEAS_Z].error_rate() == 1e-3


def test_qubit_class_level_metadata():
    """Class-level assumptions/references lists surface via the properties."""

    @qubit
    class MetadataModel:
        assumptions = ["assume-x"]
        references = ["ref-y"]

    model = MetadataModel()
    assert model.assumptions == ["assume-x"]
    assert model.references == ["ref-y"]


def test_qubit_preserves_base_metadata():
    """Inherited assumptions are preserved when the subclass does not override."""

    class Base(Architecture):
        @property
        def assumptions(self) -> list[str]:
            return ["base-assumption"]

        def provided_isa(self, ctx: ISAContext) -> ISA:
            return ctx.make_isa()

    @qubit(Base)
    class DerivedModel:
        pass

    assert DerivedModel().assumptions == ["base-assumption"]


def test_qubit_dataclass_field_override():
    """Bare attributes matching a dataclass base field become field overrides."""

    @dataclass
    class DataBase(Architecture):
        error_rate: float = 1e-3

        def provided_isa(self, ctx: ISAContext) -> ISA:
            return ctx.make_isa()

    @qubit(DataBase)
    class TunedModel:
        error_rate = 1e-6

    assert TunedModel().error_rate == 1e-6
    # The override is a proper dataclass field, so it can still be set via init.
    assert TunedModel(error_rate=1e-9).error_rate == 1e-9


def test_qubit_registers_model():
    """Decorated models are registered in QUBIT_MODELS under their display name."""

    @qubit(name="RegisteredModel")
    class _Registered:
        pass

    assert QUBIT_MODELS["RegisteredModel"] is _Registered


def test_qubit_duplicate_name_raises():
    """Registering two different models under the same name raises ValueError."""

    @qubit(name="CollidingModel")
    class _First:
        pass

    with pytest.raises(ValueError, match="already registered"):

        @qubit(name="CollidingModel")
        class _Second:
            pass


# ---------------------------------------------------------------------------
# QECStrategy
# ---------------------------------------------------------------------------


def test_qec_strategy_defaults():
    """QECStrategy applies documented defaults for optional fields."""

    strategy = QECStrategy(
        suffix="sc", name="Surface Code", build_isa_query=_dummy_query
    )

    assert strategy.needs_lattice_surgery_transform is True
    assert strategy.supports_ccx_states is False
    assert strategy.columns == ()
    assert strategy.assumptions == ()


def test_qec_strategy_is_frozen():
    """QECStrategy instances are immutable."""

    strategy = QECStrategy(
        suffix="sc", name="Surface Code", build_isa_query=_dummy_query
    )

    with pytest.raises(FrozenInstanceError):
        strategy.suffix = "other"  # type: ignore[misc]


def test_qec_strategy_isa_query_is_fresh_each_access():
    """isa_query invokes build_isa_query on every access, yielding fresh trees."""

    calls = {"n": 0}

    def build() -> ISAQuery:
        calls["n"] += 1
        return cast(ISAQuery, object())

    strategy = QECStrategy(suffix="sc", name="Surface Code", build_isa_query=build)

    first = strategy.isa_query
    second = strategy.isa_query

    assert calls["n"] == 2
    assert first is not second


# ---------------------------------------------------------------------------
# Target
# ---------------------------------------------------------------------------


@qubit(name="TargetTestQubit", family=TechnologyFamily.SUPERCONDUCTING)
class _TargetQubit:
    assumptions = ["arch-assumption"]


def _make_strategy(**kwargs) -> QECStrategy:
    params: dict[str, Any] = dict(
        suffix="sc", name="Surface Code", build_isa_query=_dummy_query
    )
    params.update(kwargs)
    return QECStrategy(**params)


def test_target_clean_constructor():
    """Target exposes clean, non-underscore constructor parameters."""

    arch = _TargetQubit()
    strategy = _make_strategy()

    target = Target(architecture=arch, qec=strategy)

    assert target.architecture is arch
    assert target.qec is strategy


def test_target_name_format():
    """Target name combines the architecture name and the strategy suffix."""

    target = Target(architecture=_TargetQubit(), qec=_make_strategy(suffix="3aux"))

    assert target.name == "TargetTestQubit+3aux"


def test_target_delegates_to_strategy():
    """Target read-only properties delegate to the wrapped QECStrategy."""

    strategy = _make_strategy(
        needs_lattice_surgery_transform=False,
        supports_ccx_states=True,
        columns=(("distance", lambda entry: 7),),
    )
    target = Target(architecture=_TargetQubit(), qec=strategy)

    assert target.needs_lattice_surgery_transform is False
    assert target.supports_ccx_states is True
    assert target.columns == strategy.columns


def test_target_assumptions_concatenated():
    """Target assumptions concatenate architecture and strategy assumptions."""

    strategy = _make_strategy(assumptions=("qec-assumption",))
    target = Target(architecture=_TargetQubit(), qec=strategy)

    assert target.assumptions == ["arch-assumption", "qec-assumption"]


def test_target_is_frozen():
    """Target instances are immutable."""

    target = Target(architecture=_TargetQubit(), qec=_make_strategy())

    with pytest.raises(FrozenInstanceError):
        target.qec = _make_strategy()  # type: ignore[misc]
