# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for Trotter-Suzuki decomposition classes and factory functions."""

from qsharp.magnets.trotter import (
    TrotterStep,
    TrotterExpansion,
    trotter_decomposition,
    strang_splitting,
    suzuki_recursion,
    yoshida_recursion,
    fourth_order_trotter_suzuki,
)


# TrotterStep base class tests


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


# trotter_decomposition factory tests


def test_trotter_decomposition_basic():
    """Test basic trotter_decomposition creation."""
    trotter = trotter_decomposition(num_terms=3, time=0.5)
    assert trotter.nterms == 3
    assert trotter.time_step == 0.5
    assert trotter.order == 1


def test_trotter_decomposition_single_term():
    """Test trotter_decomposition with a single term."""
    trotter = trotter_decomposition(num_terms=1, time=1.0)
    result = list(trotter.step())
    assert result == [(1.0, 0)]


def test_trotter_decomposition_multiple_terms():
    """Test trotter_decomposition with multiple terms."""
    trotter = trotter_decomposition(num_terms=3, time=0.5)
    result = list(trotter.step())
    assert result == [(0.5, 0), (0.5, 1), (0.5, 2)]


def test_trotter_decomposition_zero_time():
    """Test trotter_decomposition with zero time."""
    trotter = trotter_decomposition(num_terms=2, time=0.0)
    result = list(trotter.step())
    assert result == [(0.0, 0), (0.0, 1)]


def test_trotter_decomposition_returns_all_terms():
    """Test that trotter_decomposition returns all term indices."""
    num_terms = 5
    trotter = trotter_decomposition(num_terms=num_terms, time=1.0)
    result = list(trotter.step())
    assert len(result) == num_terms
    term_indices = [idx for _, idx in result]
    assert term_indices == list(range(num_terms))


def test_trotter_decomposition_uniform_time():
    """Test that all terms have the same time in trotter_decomposition."""
    time = 0.25
    trotter = trotter_decomposition(num_terms=4, time=time)
    result = list(trotter.step())
    for t, _ in result:
        assert t == time


def test_trotter_decomposition_str():
    """Test string representation of trotter_decomposition result."""
    trotter = trotter_decomposition(num_terms=3, time=0.5)
    result = str(trotter)
    assert "order" in result.lower() or "1" in result


def test_trotter_decomposition_repr():
    """Test repr representation of trotter_decomposition result."""
    trotter = trotter_decomposition(num_terms=3, time=0.5)
    assert "FirstOrderTrotter" in repr(trotter)


# strang_splitting factory tests


def test_strang_splitting_basic():
    """Test basic strang_splitting creation."""
    strang = strang_splitting(num_terms=3, time=0.5)
    assert strang.nterms == 3
    assert strang.time_step == 0.5
    assert strang.order == 2


def test_strang_splitting_single_term():
    """Test strang_splitting with a single term."""
    strang = strang_splitting(num_terms=1, time=1.0)
    result = list(strang.step())
    # Single term: just full time on term 0
    assert result == [(1.0, 0)]


def test_strang_splitting_two_terms():
    """Test strang_splitting with two terms."""
    strang = strang_splitting(num_terms=2, time=1.0)
    result = list(strang.step())
    # Forward: half on term 0, full on term 1, backward: half on term 0
    assert result == [(0.5, 0), (1.0, 1), (0.5, 0)]


def test_strang_splitting_three_terms():
    """Test strang_splitting with three terms (example from docstring)."""
    strang = strang_splitting(num_terms=3, time=0.5)
    result = list(strang.step())
    expected = [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]
    assert result == expected


def test_strang_splitting_symmetric():
    """Test that strang_splitting produces symmetric sequence."""
    strang = strang_splitting(num_terms=4, time=1.0)
    result = list(strang.step())
    # Check symmetry: term indices should be palindromic
    term_indices = [idx for _, idx in result]
    assert term_indices == term_indices[::-1]


def test_strang_splitting_time_sum():
    """Test that total time in strang_splitting equals expected value."""
    time = 1.0
    num_terms = 3
    strang = strang_splitting(num_terms=num_terms, time=time)
    result = list(strang.step())
    total_time = sum(t for t, _ in result)
    # Each term appears once with full time equivalent
    # (half + half for outer terms, full for middle)
    assert abs(total_time - time * num_terms) < 1e-10


def test_strang_splitting_middle_term_full_time():
    """Test that the middle term gets full time step."""
    strang = strang_splitting(num_terms=5, time=2.0)
    result = list(strang.step())
    # Middle term (index 4, the last term) should have full time
    middle_entries = [(t, idx) for t, idx in result if idx == 4]
    assert len(middle_entries) == 1
    assert middle_entries[0][0] == 2.0


def test_strang_splitting_outer_terms_half_time():
    """Test that outer terms get half time steps."""
    strang = strang_splitting(num_terms=4, time=2.0)
    result = list(strang.step())
    # Term 0 should appear twice with half time each
    term_0_entries = [(t, idx) for t, idx in result if idx == 0]
    assert len(term_0_entries) == 2
    for t, _ in term_0_entries:
        assert t == 1.0


def test_strang_splitting_repr():
    """Test repr representation of strang_splitting result."""
    strang = strang_splitting(num_terms=3, time=0.5)
    assert "StrangSplitting" in repr(strang)


# suzuki_recursion tests


def test_suzuki_recursion_from_strang():
    """Test Suzuki recursion applied to Strang splitting produces 4th order."""
    strang = strang_splitting(num_terms=2, time=1.0)
    suzuki = suzuki_recursion(strang)
    assert suzuki.order == 4
    assert suzuki.nterms == 2
    assert suzuki.time_step == 1.0


def test_suzuki_recursion_from_first_order():
    """Test Suzuki recursion applied to first-order Trotter produces 3rd order."""
    trotter = trotter_decomposition(num_terms=2, time=1.0)
    suzuki = suzuki_recursion(trotter)
    assert suzuki.order == 3
    assert suzuki.nterms == 2


def test_suzuki_recursion_preserves_nterms():
    """Test that Suzuki recursion preserves number of terms."""
    base = strang_splitting(num_terms=5, time=0.5)
    suzuki = suzuki_recursion(base)
    assert suzuki.nterms == base.nterms


def test_suzuki_recursion_preserves_time_step():
    """Test that Suzuki recursion preserves time step."""
    base = strang_splitting(num_terms=3, time=0.75)
    suzuki = suzuki_recursion(base)
    assert suzuki.time_step == base.time_step


def test_suzuki_recursion_repr():
    """Test repr of Suzuki recursion result."""
    base = strang_splitting(num_terms=2, time=1.0)
    suzuki = suzuki_recursion(base)
    assert "SuzukiRecursion" in repr(suzuki)


def test_suzuki_recursion_time_weights_sum():
    """Test that time weights in Suzuki recursion sum correctly."""
    base = trotter_decomposition(num_terms=2, time=1.0)
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
    strang = strang_splitting(num_terms=2, time=1.0)
    yoshida = yoshida_recursion(strang)
    assert yoshida.order == 4
    assert yoshida.nterms == 2
    assert yoshida.time_step == 1.0


def test_yoshida_recursion_from_first_order():
    """Test Yoshida recursion applied to first-order Trotter produces 3rd order."""
    trotter = trotter_decomposition(num_terms=2, time=1.0)
    yoshida = yoshida_recursion(trotter)
    assert yoshida.order == 3
    assert yoshida.nterms == 2


def test_yoshida_recursion_preserves_nterms():
    """Test that Yoshida recursion preserves number of terms."""
    base = strang_splitting(num_terms=5, time=0.5)
    yoshida = yoshida_recursion(base)
    assert yoshida.nterms == base.nterms


def test_yoshida_recursion_preserves_time_step():
    """Test that Yoshida recursion preserves time step."""
    base = strang_splitting(num_terms=3, time=0.75)
    yoshida = yoshida_recursion(base)
    assert yoshida.time_step == base.time_step


def test_yoshida_recursion_repr():
    """Test repr of Yoshida recursion result."""
    base = strang_splitting(num_terms=2, time=1.0)
    yoshida = yoshida_recursion(base)
    assert "YoshidaRecursion" in repr(yoshida)


def test_yoshida_recursion_time_weights_sum():
    """Test that time weights in Yoshida recursion sum correctly."""
    base = trotter_decomposition(num_terms=2, time=1.0)
    yoshida = yoshida_recursion(base)
    # The total scaled time should equal the original total time * nterms
    # because weights w1 + w0 + w1 = 2*w1 + w0 = 2*w1 + (1 - 2*w1) = 1
    result = list(yoshida.step())
    total_time = sum(t for t, _ in result)
    base_total = sum(t for t, _ in base.step())
    assert abs(total_time - base_total) < 1e-10


def test_yoshida_fewer_terms_than_suzuki():
    """Test that Yoshida produces fewer terms than Suzuki (3x vs 5x)."""
    base = strang_splitting(num_terms=3, time=1.0)
    suzuki = suzuki_recursion(base)
    yoshida = yoshida_recursion(base)
    # Yoshida uses 3 copies, Suzuki uses 5 copies
    # After reduction, Yoshida should generally have fewer terms
    assert len(list(yoshida.step())) <= len(list(suzuki.step()))


# fourth_order_trotter_suzuki tests


def test_fourth_order_trotter_suzuki_basic():
    """Test fourth_order_trotter_suzuki factory function."""
    fourth = fourth_order_trotter_suzuki(num_terms=2, time=1.0)
    assert fourth.order == 4
    assert fourth.nterms == 2
    assert fourth.time_step == 1.0


def test_fourth_order_trotter_suzuki_equals_suzuki_of_strang():
    """Test that fourth_order_trotter_suzuki equals suzuki_recursion(strang_splitting)."""
    fourth = fourth_order_trotter_suzuki(num_terms=3, time=0.5)
    manual = suzuki_recursion(strang_splitting(num_terms=3, time=0.5))
    assert list(fourth.step()) == list(manual.step())
    assert fourth.order == manual.order


# TrotterExpansion tests


def test_trotter_expansion_init_basic():
    """Test basic TrotterExpansion initialization."""
    step = trotter_decomposition(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    assert expansion._trotter_step is step
    assert expansion._num_steps == 4


def test_trotter_expansion_get_single_step():
    """Test TrotterExpansion with a single step."""
    step = trotter_decomposition(num_terms=2, time=1.0)
    expansion = TrotterExpansion(step, num_steps=1)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 1
    assert terms == [(1.0, 0), (1.0, 1)]


def test_trotter_expansion_get_multiple_steps():
    """Test TrotterExpansion with multiple steps."""
    step = trotter_decomposition(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 4
    assert terms == [(0.25, 0), (0.25, 1)]


def test_trotter_expansion_with_strang():
    """Test TrotterExpansion using strang_splitting."""
    step = strang_splitting(num_terms=2, time=0.5)
    expansion = TrotterExpansion(step, num_steps=2)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 2
    # strang_splitting with 2 terms: [(0.25, 0), (0.5, 1), (0.25, 0)]
    assert terms == [(0.25, 0), (0.5, 1), (0.25, 0)]


def test_trotter_expansion_total_time():
    """Test that total evolution time is correct."""
    total_time = 1.0
    num_steps = 4
    step = trotter_decomposition(num_terms=3, time=total_time / num_steps)
    expansion = TrotterExpansion(step, num_steps=num_steps)
    result = expansion.get()
    terms, count = result[0]
    # Total time = sum of times in one step * count
    step_time = sum(t for t, _ in terms)
    total = step_time * count
    # For first-order Trotter, step_time = time * num_terms
    assert abs(total - total_time * 3) < 1e-10


def test_trotter_expansion_preserves_step():
    """Test that expansion preserves the original step."""
    step = trotter_decomposition(num_terms=3, time=0.5)
    expansion = TrotterExpansion(step, num_steps=10)
    result = expansion.get()
    terms, _ = result[0]
    assert terms == list(step.step())


def test_trotter_expansion_with_fourth_order():
    """Test TrotterExpansion with fourth-order Trotter-Suzuki."""
    step = fourth_order_trotter_suzuki(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    result = expansion.get()
    terms, count = result[0]
    assert count == 4
    assert step.order == 4


def test_trotter_expansion_order_property():
    """Test TrotterExpansion order property."""
    step = strang_splitting(num_terms=3, time=0.5)
    expansion = TrotterExpansion(step, num_steps=4)
    assert expansion.order == 2


def test_trotter_expansion_nterms_property():
    """Test TrotterExpansion nterms property."""
    step = trotter_decomposition(num_terms=5, time=0.5)
    expansion = TrotterExpansion(step, num_steps=4)
    assert expansion.nterms == 5


def test_trotter_expansion_num_steps_property():
    """Test TrotterExpansion num_steps property."""
    step = trotter_decomposition(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=8)
    assert expansion.num_steps == 8


def test_trotter_expansion_total_time_property():
    """Test TrotterExpansion total_time property."""
    step = trotter_decomposition(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    assert expansion.total_time == 1.0


def test_trotter_expansion_step_iterator():
    """Test TrotterExpansion step() iterator yields full expansion."""
    step = trotter_decomposition(num_terms=2, time=0.5)
    expansion = TrotterExpansion(step, num_steps=3)
    result = list(expansion.step())
    # Should yield 3 repetitions of [(0.5, 0), (0.5, 1)]
    expected = [(0.5, 0), (0.5, 1), (0.5, 0), (0.5, 1), (0.5, 0), (0.5, 1)]
    assert result == expected


def test_trotter_expansion_step_iterator_with_strang():
    """Test TrotterExpansion step() with Strang splitting."""
    step = strang_splitting(num_terms=2, time=1.0)
    expansion = TrotterExpansion(step, num_steps=2)
    result = list(expansion.step())
    # Strang with 2 terms: [(0.5, 0), (1.0, 1), (0.5, 0)]
    # Repeated twice
    expected = [(0.5, 0), (1.0, 1), (0.5, 0), (0.5, 0), (1.0, 1), (0.5, 0)]
    assert result == expected


def test_trotter_expansion_str():
    """Test TrotterExpansion string representation."""
    step = strang_splitting(num_terms=3, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    result = str(expansion)
    assert "order=2" in result
    assert "num_steps=4" in result
    assert "total_time=1.0" in result
    assert "num_terms=3" in result


def test_trotter_expansion_repr():
    """Test TrotterExpansion repr representation."""
    step = trotter_decomposition(num_terms=2, time=0.5)
    expansion = TrotterExpansion(step, num_steps=4)
    result = repr(expansion)
    assert "TrotterExpansion" in result
    assert "num_steps=4" in result
