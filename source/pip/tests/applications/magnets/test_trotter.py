# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for Trotter-Suzuki decomposition classes and factory functions."""

import pytest

cirq = pytest.importorskip("cirq")

from qdk.applications.magnets import (
    Hyperedge,
    Hypergraph,
    Model,
    PauliString,
    TrotterExpansion,
    TrotterStep,
    fourth_order_trotter_suzuki,
    strang_splitting,
    suzuki_recursion,
    yoshida_recursion,
)


def make_two_term_model() -> Model:
    edge = Hyperedge([0, 1])
    model = Model(Hypergraph([edge]))
    model.add_interaction(edge, "ZZ", -2.0, term=0, color=0)
    model.add_interaction(edge, "XX", -0.5, term=1, color=0)
    return model


# TrotterStep tests


def test_trotter_step_empty_init():
    """Test that TrotterStep initializes as empty."""
    trotter = TrotterStep()
    assert trotter.nterms == 0
    assert trotter.time_step == 0.0
    assert trotter.order == 0
    assert list(trotter.step()) == []


def test_trotter_step_reduce_combines_consecutive():
    """Test that reduce combines consecutive same-term entries."""
    trotter = TrotterStep()
    trotter.terms = [(0.5, 0), (0.5, 0), (0.5, 1)]
    trotter.reduce()
    assert list(trotter.step()) == [(1.0, 0), (0.5, 1)]


def test_trotter_step_reduce_no_change_when_different():
    """Test that reduce does not change non-consecutive same terms."""
    trotter = TrotterStep()
    trotter.terms = [(0.5, 0), (0.5, 1), (0.5, 0)]
    trotter.reduce()
    assert list(trotter.step()) == [(0.5, 0), (0.5, 1), (0.5, 0)]


def test_trotter_step_reduce_empty():
    """Test that reduce handles empty terms."""
    trotter = TrotterStep()
    trotter.reduce()
    assert list(trotter.step()) == []


# first-order TrotterStep constructor tests


def test_trotter_step_from_explicit_terms_basic():
    """Test basic TrotterStep creation from explicit term indices."""
    trotter = TrotterStep(terms=[0, 1, 2], time_step=0.5)
    assert trotter.nterms == 3
    assert trotter.time_step == 0.5
    assert trotter.order == 1


def test_trotter_step_first_order_single_term():
    """Test TrotterStep with a single explicit term."""
    trotter = TrotterStep(terms=[7], time_step=1.0)
    result = list(trotter.step())
    assert result == [(1.0, 7)]


def test_trotter_step_first_order_multiple_terms():
    """Test TrotterStep with multiple explicit terms."""
    trotter = TrotterStep(terms=[0, 1, 2], time_step=0.5)
    result = list(trotter.step())
    assert result == [(0.5, 0), (0.5, 1), (0.5, 2)]


def test_trotter_step_first_order_zero_time():
    """Test TrotterStep with zero time."""
    trotter = TrotterStep(terms=[0, 1], time_step=0.0)
    result = list(trotter.step())
    assert result == [(0.0, 0), (0.0, 1)]


def test_trotter_step_first_order_returns_all_terms():
    """Test that TrotterStep returns all provided term indices in order."""
    terms = [2, 4, 7, 11, 15]
    trotter = TrotterStep(terms=terms, time_step=1.0)
    result = list(trotter.step())
    assert len(result) == len(terms)
    term_indices = [idx for _, idx in result]
    assert term_indices == terms


def test_trotter_step_first_order_uniform_time():
    """Test that all entries have the same configured time."""
    time = 0.25
    trotter = TrotterStep(terms=[0, 1, 2, 3], time_step=time)
    result = list(trotter.step())
    for t, _ in result:
        assert t == time


def test_trotter_step_first_order_str():
    """Test string representation of TrotterStep."""
    trotter = TrotterStep(terms=[0, 1, 2], time_step=0.5)
    result = str(trotter)
    assert "order" in result.lower() or "1" in result


def test_trotter_step_first_order_repr():
    """Test repr representation of TrotterStep."""
    trotter = TrotterStep(terms=[0, 1, 2], time_step=0.5)
    assert "TrotterStep" in repr(trotter)


# strang_splitting factory tests


def test_strang_splitting_basic():
    """Test basic strang_splitting creation."""
    strang = strang_splitting(terms=[0, 1, 2], time=0.5)
    assert strang.nterms == 3
    assert strang.time_step == 0.5
    assert strang.order == 2


def test_strang_splitting_single_term():
    """Test strang_splitting with a single term."""
    strang = strang_splitting(terms=[0], time=1.0)
    result = list(strang.step())
    # Single term: just full time on term 0
    assert result == [(1.0, 0)]


def test_strang_splitting_two_terms():
    """Test strang_splitting with two terms."""
    strang = strang_splitting(terms=[0, 1], time=1.0)
    result = list(strang.step())
    # Forward: half on term 0, full on term 1, backward: half on term 0
    assert result == [(0.5, 0), (1.0, 1), (0.5, 0)]


def test_strang_splitting_three_terms():
    """Test strang_splitting with three terms (example from docstring)."""
    strang = strang_splitting(terms=[0, 1, 2], time=0.5)
    result = list(strang.step())
    expected = [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]
    assert result == expected


def test_strang_splitting_symmetric():
    """Test that strang_splitting produces symmetric sequence."""
    strang = strang_splitting(terms=[0, 1, 2, 3], time=1.0)
    result = list(strang.step())
    # Check symmetry: term indices should be palindromic
    term_indices = [idx for _, idx in result]
    assert term_indices == term_indices[::-1]


def test_strang_splitting_time_sum():
    """Test that total time in strang_splitting equals expected value."""
    time = 1.0
    terms = [0, 1, 2]
    strang = strang_splitting(terms=terms, time=time)
    result = list(strang.step())
    total_time = sum(t for t, _ in result)
    # Each term appears once with full time equivalent
    # (half + half for outer terms, full for middle)
    assert abs(total_time - time * len(terms)) < 1e-10


def test_strang_splitting_middle_term_full_time():
    """Test that the middle term gets full time step."""
    strang = strang_splitting(terms=[0, 1, 2, 3, 4], time=2.0)
    result = list(strang.step())
    # Middle term (index 4, the last term) should have full time
    middle_entries = [(t, idx) for t, idx in result if idx == 4]
    assert len(middle_entries) == 1
    assert middle_entries[0][0] == 2.0


def test_strang_splitting_outer_terms_half_time():
    """Test that outer terms get half time steps."""
    strang = strang_splitting(terms=[0, 1, 2, 3], time=2.0)
    result = list(strang.step())
    # Term 0 should appear twice with half time each
    term_0_entries = [(t, idx) for t, idx in result if idx == 0]
    assert len(term_0_entries) == 2
    for t, _ in term_0_entries:
        assert t == 1.0


def test_strang_splitting_repr():
    """Test repr representation of strang_splitting result."""
    strang = strang_splitting(terms=[0, 1, 2], time=0.5)
    assert "StrangSplitting" in repr(strang)


# suzuki_recursion tests


def test_suzuki_recursion_from_strang():
    """Test Suzuki recursion applied to Strang splitting produces 4th order."""
    strang = strang_splitting(terms=[0, 1], time=1.0)
    suzuki = suzuki_recursion(strang)
    assert suzuki.order == 4
    assert suzuki.nterms == 2
    assert suzuki.time_step == 1.0


def test_suzuki_recursion_from_first_order():
    """Test Suzuki recursion applied to first-order Trotter produces 3rd order."""
    trotter = TrotterStep(terms=[0, 1], time_step=1.0)
    suzuki = suzuki_recursion(trotter)
    assert suzuki.order == 3
    assert suzuki.nterms == 2


def test_suzuki_recursion_preserves_nterms():
    """Test that Suzuki recursion preserves number of terms."""
    base = strang_splitting(terms=[0, 1, 2, 3, 4], time=0.5)
    suzuki = suzuki_recursion(base)
    assert suzuki.nterms == base.nterms


def test_suzuki_recursion_preserves_time_step():
    """Test that Suzuki recursion preserves time step."""
    base = strang_splitting(terms=[0, 1, 2], time=0.75)
    suzuki = suzuki_recursion(base)
    assert suzuki.time_step == base.time_step


def test_suzuki_recursion_repr():
    """Test repr of Suzuki recursion result."""
    base = strang_splitting(terms=[0, 1], time=1.0)
    suzuki = suzuki_recursion(base)
    assert "SuzukiRecursion" in repr(suzuki)


def test_suzuki_recursion_time_weights_sum():
    """Test that time weights in Suzuki recursion sum correctly."""
    base = TrotterStep(terms=[0, 1], time_step=1.0)
    suzuki = suzuki_recursion(base)
    # The total scaled time should equal the original total time * nterms
    # because we're scaling times, not adding them
    result = list(suzuki.step())
    total_time = sum(t for t, _ in result)
    # For Suzuki: 5 copies scaled by p, p, (1-4p), p, p
    # where weights sum to 4p + (1-4p) = 1, so total = base total
    base_total = sum(t for t, _ in base.step())
    assert abs(total_time - base_total) < 1e-10


# yoshida_recursion tests


def test_yoshida_recursion_from_strang():
    """Test Yoshida recursion applied to Strang splitting produces 4th order."""
    strang = strang_splitting(terms=[0, 1], time=1.0)
    yoshida = yoshida_recursion(strang)
    assert yoshida.order == 4
    assert yoshida.nterms == 2
    assert yoshida.time_step == 1.0


def test_yoshida_recursion_from_first_order():
    """Test Yoshida recursion applied to first-order Trotter produces 3rd order."""
    trotter = TrotterStep(terms=[0, 1], time_step=1.0)
    yoshida = yoshida_recursion(trotter)
    assert yoshida.order == 3
    assert yoshida.nterms == 2


def test_yoshida_recursion_preserves_nterms():
    """Test that Yoshida recursion preserves number of terms."""
    base = strang_splitting(terms=[0, 1, 2, 3, 4], time=0.5)
    yoshida = yoshida_recursion(base)
    assert yoshida.nterms == base.nterms


def test_yoshida_recursion_preserves_time_step():
    """Test that Yoshida recursion preserves time step."""
    base = strang_splitting(terms=[0, 1, 2], time=0.75)
    yoshida = yoshida_recursion(base)
    assert yoshida.time_step == base.time_step


def test_yoshida_recursion_repr():
    """Test repr of Yoshida recursion result."""
    base = strang_splitting(terms=[0, 1], time=1.0)
    yoshida = yoshida_recursion(base)
    assert "YoshidaRecursion" in repr(yoshida)


def test_yoshida_recursion_time_weights_sum():
    """Test that time weights in Yoshida recursion sum correctly."""
    base = TrotterStep(terms=[0, 1], time_step=1.0)
    yoshida = yoshida_recursion(base)
    # The total scaled time should equal the original total time * nterms
    # because weights w1 + w0 + w1 = 2*w1 + w0 = 2*w1 + (1 - 2*w1) = 1
    result = list(yoshida.step())
    total_time = sum(t for t, _ in result)
    base_total = sum(t for t, _ in base.step())
    assert abs(total_time - base_total) < 1e-10


def test_yoshida_fewer_terms_than_suzuki():
    """Test that Yoshida produces fewer terms than Suzuki (3x vs 5x)."""
    base = strang_splitting(terms=[0, 1, 2], time=1.0)
    suzuki = suzuki_recursion(base)
    yoshida = yoshida_recursion(base)
    # Yoshida uses 3 copies, Suzuki uses 5 copies
    # After reduction, Yoshida should generally have fewer terms
    assert len(list(yoshida.step())) <= len(list(suzuki.step()))


# fourth_order_trotter_suzuki tests


def test_fourth_order_trotter_suzuki_basic():
    """Test fourth_order_trotter_suzuki factory function."""
    fourth = fourth_order_trotter_suzuki(terms=[0, 1], time=1.0)
    assert fourth.order == 4
    assert fourth.nterms == 2
    assert fourth.time_step == 1.0


def test_fourth_order_trotter_suzuki_equals_suzuki_of_strang():
    """Test that fourth_order_trotter_suzuki equals suzuki_recursion(strang_splitting)."""
    fourth = fourth_order_trotter_suzuki(terms=[0, 1, 2], time=0.5)
    manual = suzuki_recursion(strang_splitting(terms=[0, 1, 2], time=0.5))
    assert list(fourth.step()) == list(manual.step())
    assert fourth.order == manual.order


# TrotterExpansion tests


def test_trotter_expansion_order_property():
    """Test TrotterExpansion order property."""
    model = make_two_term_model()
    expansion = TrotterExpansion(strang_splitting, model, time=1.0, num_steps=4)
    assert expansion.order == 2


def test_trotter_expansion_nterms_property():
    """Test TrotterExpansion nterms property."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.0, num_steps=4)
    assert expansion.nterms == 2


def test_trotter_expansion_num_steps_property():
    """Test TrotterExpansion num_steps property."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.0, num_steps=8)
    assert expansion.nsteps == 8


def test_trotter_expansion_total_time_property():
    """Test TrotterExpansion total_time property."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.0, num_steps=4)
    assert expansion.total_time == 1.0


def test_trotter_expansion_step_iterator():
    """Test TrotterExpansion.step() yields scaled PauliStrings."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.2, num_steps=3)
    result = list(expansion.step())

    # dt = 1.2 / 3 = 0.4 and model terms are 0->ZZ(-2.0), 1->XX(-0.5)
    expected = [
        ((0, 1), "ZZ", -0.8),
        ((0, 1), "XX", -0.2),
        ((0, 1), "ZZ", -0.8),
        ((0, 1), "XX", -0.2),
        ((0, 1), "ZZ", -0.8),
        ((0, 1), "XX", -0.2),
    ]
    assert len(result) == len(expected)
    for op, (qubits, paulis, coefficient) in zip(result, expected):
        assert op.qubits == qubits
        assert op.paulis == paulis
        assert op.coefficient == pytest.approx(coefficient)


def test_trotter_expansion_step_iterator_with_strang():
    """Test TrotterExpansion.step() with Strang splitting schedule."""
    model = make_two_term_model()
    expansion = TrotterExpansion(strang_splitting, model, time=2.0, num_steps=2)
    result = list(expansion.step())

    # dt = 1.0; one Strang step over terms [0,1] is:
    # (0.5,0), (1.0,1), (0.5,0)
    expected_single = [
        PauliString.from_qubits((0, 1), "ZZ", -1.0),
        PauliString.from_qubits((0, 1), "XX", -0.5),
        PauliString.from_qubits((0, 1), "ZZ", -1.0),
    ]
    expected = expected_single * 2
    assert result == expected


def test_trotter_expansion_str():
    """Test TrotterExpansion string representation."""
    model = make_two_term_model()
    expansion = TrotterExpansion(strang_splitting, model, time=1.0, num_steps=4)
    result = str(expansion)
    assert "order=2" in result
    assert "num_steps=4" in result
    assert "total_time=1.0" in result
    assert "num_terms=2" in result


def test_trotter_expansion_repr():
    """Test TrotterExpansion repr representation."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.0, num_steps=4)
    result = repr(expansion)
    assert "TrotterExpansion" in result
    assert "num_steps=4" in result


def test_trotter_expansion_cirq_repetitions():
    """Test that TrotterExpansion.cirq repeats one-step circuit num_steps times."""
    model = make_two_term_model()
    expansion = TrotterExpansion(TrotterStep, model, time=1.0, num_steps=5)

    op = expansion.cirq()
    assert op.repetitions == 5


def test_strang_splitting_rejects_empty_terms():
    """Test strang_splitting raises for empty term list."""
    with pytest.raises(IndexError):
        strang_splitting([], time=1.0)
