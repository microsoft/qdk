"""Tests for gadget action profiling and equivalence."""
import qodec
from qdk.ec.action import LogicalAction, logical_action_of
from qdk.ec.equivalence import gadgets_equivalent, why_not_equivalent


def test_gadget_is_equivalent_to_itself(translation: qodec.Layer) -> None:
    for name in ("idle", "measure_zz", "prepare_zz"):
        g = translation.gadgets[name]
        assert gadgets_equivalent(g, g)
        assert why_not_equivalent(g, g) == ""


def test_distinct_gadgets_are_not_equivalent(idle_gadget: qodec.Gadget, measure_xx_gadget: qodec.Gadget, measure_zz_gadget: qodec.Gadget) -> None:
    assert not gadgets_equivalent(idle_gadget, measure_xx_gadget)
    assert not gadgets_equivalent(measure_xx_gadget, measure_zz_gadget)
    assert "differ" in why_not_equivalent(measure_xx_gadget, measure_zz_gadget)


def test_logical_action_of_idle_is_identity(idle_gadget: qodec.Gadget) -> None:
    action = logical_action_of(idle_gadget)
    assert isinstance(action, LogicalAction)
    assert len(action.images) == 4
    for input_idx, image in enumerate(action.images):
        assert image.observable_flips == frozenset()
        partner = input_idx ^ 1
        assert image.output_logical_flips == frozenset({partner})


def test_logical_action_of_measure_xx_flips_observables(measure_xx_gadget: qodec.Gadget) -> None:
    action = logical_action_of(measure_xx_gadget)
    assert action.encoding_out == ()
    expected = [frozenset(), frozenset({0}), frozenset(), frozenset({1})]
    assert [img.observable_flips for img in action.images] == expected
