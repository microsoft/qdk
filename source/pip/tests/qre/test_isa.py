# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qsharp.qre import (
    LOGICAL,
    ISARequirements,
    constraint,
    generic_function,
    property_name,
    property_name_to_key,
)
from qsharp.qre._qre import _ProvenanceGraph
from qsharp.qre.models import SurfaceCode, GateBased
from qsharp.qre._architecture import _make_instruction
from qsharp.qre.instruction_ids import CCX, CCZ, LATTICE_SURGERY, T
from qsharp.qre.property_keys import DISTANCE


def test_isa():
    """Test ISA creation, instruction lookup, and dynamic node addition."""
    graph = _ProvenanceGraph()
    isa = graph.make_isa(
        [
            graph.add_instruction(
                T, encoding=LOGICAL, time=1000, space=400, error_rate=1e-8
            ),
            graph.add_instruction(
                CCX, encoding=LOGICAL, arity=3, time=2000, space=800, error_rate=1e-10
            ),
        ]
    )

    assert T in isa
    assert CCX in isa
    assert LATTICE_SURGERY not in isa

    t_instr = isa[T]
    assert t_instr.time() == 1000
    assert t_instr.error_rate() == 1e-8
    assert t_instr.space() == 400

    assert len(isa) == 2
    ccz_instr = isa[CCX].with_id(CCZ)
    assert ccz_instr.arity == 3
    assert ccz_instr.time() == 2000
    assert ccz_instr.error_rate() == 1e-10
    assert ccz_instr.space() == 800

    # Add another instruction to the graph and register it in the ISA
    ccz_node = graph.add_instruction(ccz_instr)
    isa.add_node(CCZ, ccz_node)
    assert CCZ in isa
    assert len(isa) == 3

    # Adding the same instruction ID should not increase the count
    isa.add_node(CCZ, ccz_node)
    assert len(isa) == 3


def test_instruction_properties():
    """Test getting and setting instruction properties."""
    # Test instruction with no properties
    instr_no_props = _make_instruction(T, 1, 1, 1000, None, None, 1e-8, {})
    assert instr_no_props.get_property(DISTANCE) is None
    assert instr_no_props.has_property(DISTANCE) is False
    assert instr_no_props.get_property_or(DISTANCE, 5) == 5

    # Test instruction with valid property (distance)
    instr_with_distance = _make_instruction(
        T, 1, 1, 1000, None, None, 1e-8, {"distance": 9}
    )
    assert instr_with_distance.get_property(DISTANCE) == 9
    assert instr_with_distance.has_property(DISTANCE) is True
    assert instr_with_distance.get_property_or(DISTANCE, 5) == 9

    # Test instruction with invalid property name
    with pytest.raises(ValueError, match="Unknown property 'invalid_prop'"):
        _make_instruction(T, 1, 1, 1000, None, None, 1e-8, {"invalid_prop": 42})


def test_instruction_constraints():
    """Test constraint property filtering and ISA.satisfies behavior."""
    # Test constraint without properties
    c_no_props = constraint(T, encoding=LOGICAL)
    assert c_no_props.has_property(DISTANCE) is False

    # Test constraint with valid property (distance=True)
    c_with_distance = constraint(T, encoding=LOGICAL, distance=True)
    assert c_with_distance.has_property(DISTANCE) is True

    # Test constraint with distance=False (should not add the property)
    c_distance_false = constraint(T, encoding=LOGICAL, distance=False)
    assert c_distance_false.has_property(DISTANCE) is False

    # Test constraint with invalid property name
    with pytest.raises(ValueError, match="Unknown property 'invalid_prop'"):
        constraint(T, encoding=LOGICAL, invalid_prop=True)

    # Test ISA.satisfies with property constraints
    graph = _ProvenanceGraph()
    isa_no_dist = graph.make_isa(
        [
            graph.add_instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-8),
        ]
    )
    isa_with_dist = graph.make_isa(
        [
            graph.add_instruction(
                T, encoding=LOGICAL, time=1000, error_rate=1e-8, distance=9
            ),
        ]
    )

    reqs_no_prop = ISARequirements(constraint(T, encoding=LOGICAL))
    reqs_with_prop = ISARequirements(constraint(T, encoding=LOGICAL, distance=True))

    # ISA without distance property
    assert isa_no_dist.satisfies(reqs_no_prop) is True
    assert isa_no_dist.satisfies(reqs_with_prop) is False

    # ISA with distance property
    assert isa_with_dist.satisfies(reqs_no_prop) is True
    assert isa_with_dist.satisfies(reqs_with_prop) is True


def test_property_names():
    """Test property name lookup and case-insensitive key resolution."""
    assert property_name(DISTANCE) == "DISTANCE"

    # An unregistered property
    UNKNOWN = 10_000
    assert property_name(UNKNOWN) is None

    # But using an existing property key with a different variable name will
    # still return something
    UNKNOWN = 0
    assert property_name(UNKNOWN) == "DISTANCE"

    assert property_name_to_key("DISTANCE") == DISTANCE

    # But we also allow case-insensitive lookup
    assert property_name_to_key("distance") == DISTANCE


def test_generic_function():
    """Test generic_function wrapping for int and float return types."""
    from qsharp.qre._qre import _IntFunction, _FloatFunction

    def time(x: int) -> int:
        return x * x

    time_fn = generic_function(time)
    assert isinstance(time_fn, _IntFunction)

    def error_rate(x: int) -> float:
        return x / 2.0

    error_rate_fn = generic_function(error_rate)
    assert isinstance(error_rate_fn, _FloatFunction)

    # Without annotations, defaults to FloatFunction
    space_fn = generic_function(lambda x: 12)
    assert isinstance(space_fn, _FloatFunction)

    i = _make_instruction(42, 0, None, time_fn, 12, None, error_rate_fn, {})
    assert i.space(5) == 12
    assert i.time(5) == 25
    assert i.error_rate(5) == 2.5


def test_isa_from_architecture():
    """Test generating logical ISAs from an architecture and QEC code."""
    arch = GateBased(gate_time=50, measurement_time=100)
    code = SurfaceCode()
    ctx = arch.context()

    # Verify that the architecture satisfies the code requirements
    assert ctx.isa.satisfies(SurfaceCode.required_isa())

    # Generate logical ISAs
    isas = list(code.provided_isa(ctx.isa, ctx))

    # There is one ISA with one instructions
    assert len(isas) == 1
    assert len(isas[0]) == 1
