"""Tests for gadget action profiling and equivalence."""

import qodec as qc
from qdk.ec._analysis.channel_action import ChannelAction, realized_action_of
from qdk.ec._analysis.equivalence import gadgets_equivalent, why_not_equivalent


def test_gadget_is_equivalent_to_itself(translation: qc.Layer) -> None:
    for name in ("idle", "measure_zz", "prepare_zz"):
        g = translation.gadgets[name]
        assert gadgets_equivalent(g, g)
        assert why_not_equivalent(g, g) == ""


def test_distinct_gadgets_are_not_equivalent(
    idle_gadget: qc.Gadget, measure_xx_gadget: qc.Gadget, measure_zz_gadget: qc.Gadget
) -> None:
    assert not gadgets_equivalent(idle_gadget, measure_xx_gadget)
    assert not gadgets_equivalent(measure_xx_gadget, measure_zz_gadget)
    assert "differ" in why_not_equivalent(measure_xx_gadget, measure_zz_gadget)


def test_distinct_preparations_are_not_equivalent(
    prepare_xx_gadget: qc.Gadget,
    prepare_zz_gadget: qc.Gadget,
) -> None:
    assert not gadgets_equivalent(prepare_xx_gadget, prepare_zz_gadget)
    assert why_not_equivalent(prepare_xx_gadget, prepare_zz_gadget)


def test_gadget_equivalence_uses_canonical_channel_actions(
    idle_gadget: qc.Gadget,
) -> None:
    action = realized_action_of(idle_gadget)

    assert isinstance(action, ChannelAction)
    assert action.is_equivalent_to(realized_action_of(idle_gadget))
