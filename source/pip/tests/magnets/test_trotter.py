# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Unit tests for Trotter-Suzuki decomposition classes."""

from qsharp.magnets.trotter import TrotterStep, StrangStep, TrotterExpansion


# TrotterStep tests


def test_trotter_step_init_basic():
    """Test basic TrotterStep initialization."""
    trotter = TrotterStep(num_terms=3, time=0.5)
    assert trotter._num_terms == 3
    assert trotter._time_step == 0.5


def test_trotter_step_get_single_term():
    """Test TrotterStep with a single term."""
    trotter = TrotterStep(num_terms=1, time=1.0)
    result = trotter.get()
    assert result == [(1.0, 0)]


def test_trotter_step_get_multiple_terms():
    """Test TrotterStep with multiple terms."""
    trotter = TrotterStep(num_terms=3, time=0.5)
    result = trotter.get()
    assert result == [(0.5, 0), (0.5, 1), (0.5, 2)]


def test_trotter_step_get_zero_time():
    """Test TrotterStep with zero time."""
    trotter = TrotterStep(num_terms=2, time=0.0)
    result = trotter.get()
    assert result == [(0.0, 0), (0.0, 1)]


def test_trotter_step_get_returns_all_terms():
    """Test that TrotterStep returns all term indices."""
    num_terms = 5
    trotter = TrotterStep(num_terms=num_terms, time=1.0)
    result = trotter.get()
    assert len(result) == num_terms
    term_indices = [idx for _, idx in result]
    assert term_indices == list(range(num_terms))


def test_trotter_step_get_uniform_time():
    """Test that all terms have the same time in TrotterStep."""
    time = 0.25
    trotter = TrotterStep(num_terms=4, time=time)
    result = trotter.get()
    for t, _ in result:
        assert t == time


def test_trotter_step_str():
    """Test string representation of TrotterStep."""
    trotter = TrotterStep(num_terms=3, time=0.5)
    result = str(trotter)
    assert "Trotter" in result
    assert "0.5" in result
    assert "3" in result


def test_trotter_step_repr():
    """Test repr representation of TrotterStep."""
    trotter = TrotterStep(num_terms=3, time=0.5)
    assert repr(trotter) == str(trotter)


# StrangStep tests


def test_strang_step_init_basic():
    """Test basic StrangStep initialization."""
    strang = StrangStep(num_terms=3, time=0.5)
    assert strang._num_terms == 3
    assert strang._time_step == 0.5


def test_strang_step_inherits_trotter():
    """Test that StrangStep inherits from TrotterStep."""
    strang = StrangStep(num_terms=3, time=0.5)
    assert isinstance(strang, TrotterStep)


def test_strang_step_get_single_term():
    """Test StrangStep with a single term."""
    strang = StrangStep(num_terms=1, time=1.0)
    result = strang.get()
    # Single term: just full time on term 0
    assert result == [(1.0, 0)]


def test_strang_step_get_two_terms():
    """Test StrangStep with two terms."""
    strang = StrangStep(num_terms=2, time=1.0)
    result = strang.get()
    # Forward: half on term 0, full on term 1, backward: half on term 0
    assert result == [(0.5, 0), (1.0, 1), (0.5, 0)]


def test_strang_step_get_three_terms():
    """Test StrangStep with three terms (example from docstring)."""
    strang = StrangStep(num_terms=3, time=0.5)
    result = strang.get()
    expected = [(0.25, 0), (0.25, 1), (0.5, 2), (0.25, 1), (0.25, 0)]
    assert result == expected


def test_strang_step_symmetric():
    """Test that StrangStep produces symmetric sequence."""
    strang = StrangStep(num_terms=4, time=1.0)
    result = strang.get()
    # Check symmetry: term indices should be palindromic
    term_indices = [idx for _, idx in result]
    assert term_indices == term_indices[::-1]


def test_strang_step_time_sum():
    """Test that total time in StrangStep equals expected value."""
    time = 1.0
    num_terms = 3
    strang = StrangStep(num_terms=num_terms, time=time)
    result = strang.get()
    total_time = sum(t for t, _ in result)
    # Each term appears once with full time equivalent
    # (half + half for outer terms, full for middle)
    assert abs(total_time - time * num_terms) < 1e-10


def test_strang_step_middle_term_full_time():
    """Test that the middle term gets full time step."""
    strang = StrangStep(num_terms=5, time=2.0)
    result = strang.get()
    # Middle term (index 4, the last term) should have full time
    middle_entries = [(t, idx) for t, idx in result if idx == 4]
    assert len(middle_entries) == 1
    assert middle_entries[0][0] == 2.0


def test_strang_step_outer_terms_half_time():
    """Test that outer terms get half time steps."""
    strang = StrangStep(num_terms=4, time=2.0)
    result = strang.get()
    # Term 0 should appear twice with half time each
    term_0_entries = [(t, idx) for t, idx in result if idx == 0]
    assert len(term_0_entries) == 2
    for t, _ in term_0_entries:
        assert t == 1.0


def test_strang_step_str():
    """Test string representation of StrangStep."""
    strang = StrangStep(num_terms=3, time=0.5)
    result = str(strang)
    assert "Strang" in result
    assert "0.5" in result
    assert "3" in result


# TrotterExpansion tests


def test_trotter_expansion_init_basic():
    """Test basic TrotterExpansion initialization."""
    step = TrotterStep(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    assert expansion._trotter_step is step
    assert expansion._num_steps == 4


def test_trotter_expansion_get_single_step():
    """Test TrotterExpansion with a single step."""
    step = TrotterStep(num_terms=2, time=1.0)
    expansion = TrotterExpansion(step, num_steps=1)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 1
    assert terms == [(1.0, 0), (1.0, 1)]


def test_trotter_expansion_get_multiple_steps():
    """Test TrotterExpansion with multiple steps."""
    step = TrotterStep(num_terms=2, time=0.25)
    expansion = TrotterExpansion(step, num_steps=4)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 4
    assert terms == [(0.25, 0), (0.25, 1)]


def test_trotter_expansion_with_strang_step():
    """Test TrotterExpansion using StrangStep."""
    step = StrangStep(num_terms=2, time=0.5)
    expansion = TrotterExpansion(step, num_steps=2)
    result = expansion.get()
    assert len(result) == 1
    terms, count = result[0]
    assert count == 2
    # StrangStep with 2 terms: [(0.25, 0), (0.5, 1), (0.25, 0)]
    assert terms == [(0.25, 0), (0.5, 1), (0.25, 0)]


def test_trotter_expansion_total_time():
    """Test that total evolution time is correct."""
    total_time = 1.0
    num_steps = 4
    step = TrotterStep(num_terms=3, time=total_time / num_steps)
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
    step = TrotterStep(num_terms=3, time=0.5)
    expansion = TrotterExpansion(step, num_steps=10)
    result = expansion.get()
    terms, _ = result[0]
    assert terms == step.get()


def test_trotter_expansion_docstring_example():
    """Test the example from the TrotterExpansion docstring."""
    n = 4  # Number of Trotter steps
    total_time = 1.0  # Total time
    trotter_expansion = TrotterExpansion(TrotterStep(2, total_time / n), n)
    result = trotter_expansion.get()
    expected = [([(0.25, 0), (0.25, 1)], 4)]
    assert result == expected
