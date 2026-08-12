"""Tests for outcome-code profiling."""
from qdk.ec.checks import OutcomeCode, outcome_code_of
from qdk.ec._analysis.propagation import Program
from qdk.ec._qodec_compat import realization
import qodec


def test_outcome_code_of_idle_channel_is_nonempty(idle_gadget: qodec.Gadget) -> None:
    channel = realization(idle_gadget)
    program = Program(channel.instructions, channel.isa)
    code = outcome_code_of(program)
    assert isinstance(code, OutcomeCode)
    assert code.measurement_count == program.outcome_count
    assert code.check_count >= 1


def test_outcome_code_of_returns_equal_results(idle_gadget: qodec.Gadget) -> None:
    channel = realization(idle_gadget)
    program = Program(channel.instructions, channel.isa)
    assert outcome_code_of(program) == outcome_code_of(program)


def test_outcome_code_checks_are_subsets_of_measurement_indices(idle_gadget: qodec.Gadget) -> None:
    channel = realization(idle_gadget)
    program = Program(channel.instructions, channel.isa)
    code = outcome_code_of(program)
    valid_indices = set(range(code.measurement_count))
    for check in code.checks():
        assert isinstance(check, frozenset)
        assert check <= valid_indices
