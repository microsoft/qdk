# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from enum import Enum
from pathlib import Path
from typing import cast, Generator, Sized
import os
import pytest

import pandas as pd
import qsharp
from qsharp.estimator import LogicalCounts
from qsharp.qre import (
    Application,
    ISA,
    LOGICAL,
    PSSPC,
    EstimationResult,
    ISARequirements,
    ISATransform,
    LatticeSurgery,
    Trace,
    constraint,
    estimate,
    linear_function,
    generic_function,
    property_name,
    property_name_to_key,
)
from qsharp.qre._qre import _ProvenanceGraph
from qsharp.qre.application import QSharpApplication
from qsharp.qre.models import (
    SurfaceCode,
    AQREGateBased,
    RoundBasedFactory,
    TwoDimensionalYokedSurfaceCode,
)
from qsharp.qre.interop import trace_from_qir
from qsharp.qre._architecture import _Context, _make_instruction
from qsharp.qre._estimation import (
    EstimationTable,
    EstimationTableEntry,
)
from qsharp.qre._instruction import InstructionSource
from qsharp.qre._isa_enumeration import (
    ISARefNode,
)
from qsharp.qre.instruction_ids import CCX, CCZ, LATTICE_SURGERY, T, RZ
from qsharp.qre.property_keys import DISTANCE, NUM_TS_PER_ROTATION

# NOTE These classes will be generalized as part of the QRE API in the following
# pull requests and then moved out of the tests.


@dataclass
class ExampleFactory(ISATransform):
    _: KW_ONLY
    level: int = field(default=1, metadata={"domain": range(1, 4)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(T),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        yield ctx.make_isa(
            ctx.add_instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-8),
        )


@dataclass
class ExampleLogicalFactory(ISATransform):
    _: KW_ONLY
    level: int = field(default=1, metadata={"domain": range(1, 4)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(LATTICE_SURGERY, encoding=LOGICAL),
            constraint(T, encoding=LOGICAL),
        )

    def provided_isa(self, impl_isa: ISA, ctx: _Context) -> Generator[ISA, None, None]:
        yield ctx.make_isa(
            ctx.add_instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-10),
        )


def test_isa():
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
    arch = AQREGateBased(gate_time=50, measurement_time=100)
    code = SurfaceCode()
    ctx = arch.context()

    # Verify that the architecture satisfies the code requirements
    assert ctx.isa.satisfies(SurfaceCode.required_isa())

    # Generate logical ISAs
    isas = list(code.provided_isa(ctx.isa, ctx))

    # There is one ISA with one instructions
    assert len(isas) == 1
    assert len(isas[0]) == 1


def test_enumerate_instances():
    from qsharp.qre._enumeration import _enumerate_instances

    instances = list(_enumerate_instances(SurfaceCode))

    # There are 12 instances with distances from 3 to 25
    assert len(instances) == 12
    expected_distances = list(range(3, 26, 2))
    for instance, expected_distance in zip(instances, expected_distances):
        assert instance.distance == expected_distance

    # Test with specific distances
    instances = list(_enumerate_instances(SurfaceCode, distance=[3, 5, 7]))
    assert len(instances) == 3
    expected_distances = [3, 5, 7]
    for instance, expected_distance in zip(instances, expected_distances):
        assert instance.distance == expected_distance

    # Test with fixed distance
    instances = list(_enumerate_instances(SurfaceCode, distance=9))
    assert len(instances) == 1
    assert instances[0].distance == 9


def test_enumerate_instances_bool():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class BoolConfig:
        _: KW_ONLY
        flag: bool

    instances = list(_enumerate_instances(BoolConfig))
    assert len(instances) == 2
    assert instances[0].flag is True
    assert instances[1].flag is False


def test_enumerate_instances_enum():
    from qsharp.qre._enumeration import _enumerate_instances

    class Color(Enum):
        RED = 1
        GREEN = 2
        BLUE = 3

    @dataclass
    class EnumConfig:
        _: KW_ONLY
        color: Color

    instances = list(_enumerate_instances(EnumConfig))
    assert len(instances) == 3
    assert instances[0].color == Color.RED
    assert instances[1].color == Color.GREEN
    assert instances[2].color == Color.BLUE


def test_enumerate_instances_failure():
    from qsharp.qre._enumeration import _enumerate_instances

    import pytest

    @dataclass
    class InvalidConfig:
        _: KW_ONLY
        # This field has no domain, is not bool/enum, and has no default
        value: int

    with pytest.raises(ValueError, match="Cannot enumerate field value"):
        list(_enumerate_instances(InvalidConfig))


def test_enumerate_instances_single():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class SingleConfig:
        value: int = 42

    instances = list(_enumerate_instances(SingleConfig))
    assert len(instances) == 1
    assert instances[0].value == 42


def test_enumerate_instances_literal():
    from qsharp.qre._enumeration import _enumerate_instances

    from typing import Literal

    @dataclass
    class LiteralConfig:
        _: KW_ONLY
        mode: Literal["fast", "slow"]

    instances = list(_enumerate_instances(LiteralConfig))
    assert len(instances) == 2
    assert instances[0].mode == "fast"
    assert instances[1].mode == "slow"


def test_enumerate_instances_nested():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class InnerConfig:
        _: KW_ONLY
        option: bool

    @dataclass
    class OuterConfig:
        _: KW_ONLY
        inner: InnerConfig

    instances = list(_enumerate_instances(OuterConfig))
    assert len(instances) == 2
    assert instances[0].inner.option is True
    assert instances[1].inner.option is False


def test_enumerate_instances_union():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class OptionA:
        _: KW_ONLY
        value: bool

    @dataclass
    class OptionB:
        _: KW_ONLY
        number: int = field(default=1, metadata={"domain": [1, 2, 3]})

    @dataclass
    class UnionConfig:
        _: KW_ONLY
        option: OptionA | OptionB

    instances = list(_enumerate_instances(UnionConfig))
    assert len(instances) == 5
    assert isinstance(instances[0].option, OptionA)
    assert instances[0].option.value is True
    assert isinstance(instances[2].option, OptionB)
    assert instances[2].option.number == 1


def test_enumerate_instances_nested_with_constraints():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class InnerConfig:
        _: KW_ONLY
        option: bool

    @dataclass
    class OuterConfig:
        _: KW_ONLY
        inner: InnerConfig

    # Constrain nested field via dict
    instances = list(_enumerate_instances(OuterConfig, inner={"option": True}))
    assert len(instances) == 1
    assert instances[0].inner.option is True


def test_enumerate_instances_union_single_type():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class OptionA:
        _: KW_ONLY
        value: bool

    @dataclass
    class OptionB:
        _: KW_ONLY
        number: int = field(default=1, metadata={"domain": [1, 2, 3]})

    @dataclass
    class UnionConfig:
        _: KW_ONLY
        option: OptionA | OptionB

    # Restrict to OptionB only - uses its default domain
    instances = list(_enumerate_instances(UnionConfig, option=OptionB))
    assert len(instances) == 3
    assert all(isinstance(i.option, OptionB) for i in instances)
    assert [cast(OptionB, i.option).number for i in instances] == [1, 2, 3]

    # Restrict to OptionA only
    instances = list(_enumerate_instances(UnionConfig, option=OptionA))
    assert len(instances) == 2
    assert all(isinstance(i.option, OptionA) for i in instances)
    assert cast(OptionA, instances[0].option).value is True
    assert cast(OptionA, instances[1].option).value is False


def test_enumerate_instances_union_list_of_types():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class OptionA:
        _: KW_ONLY
        value: bool

    @dataclass
    class OptionB:
        _: KW_ONLY
        number: int = field(default=1, metadata={"domain": [1, 2, 3]})

    @dataclass
    class OptionC:
        _: KW_ONLY
        flag: bool

    @dataclass
    class UnionConfig:
        _: KW_ONLY
        option: OptionA | OptionB | OptionC

    # Select a subset: only OptionA and OptionB
    instances = list(_enumerate_instances(UnionConfig, option=[OptionA, OptionB]))
    assert len(instances) == 5  # 2 from OptionA + 3 from OptionB
    assert all(isinstance(i.option, (OptionA, OptionB)) for i in instances)


def test_enumerate_instances_union_constraint_dict():
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class OptionA:
        _: KW_ONLY
        value: bool

    @dataclass
    class OptionB:
        _: KW_ONLY
        number: int = field(default=1, metadata={"domain": [1, 2, 3]})

    @dataclass
    class UnionConfig:
        _: KW_ONLY
        option: OptionA | OptionB

    # Constrain OptionA, enumerate only that member
    instances = list(
        _enumerate_instances(UnionConfig, option={OptionA: {"value": True}})
    )
    assert len(instances) == 1
    assert isinstance(instances[0].option, OptionA)
    assert instances[0].option.value is True

    # Constrain OptionB with a domain, enumerate only that member
    instances = list(
        _enumerate_instances(UnionConfig, option={OptionB: {"number": [2, 3]}})
    )
    assert len(instances) == 2
    assert all(isinstance(i.option, OptionB) for i in instances)
    assert cast(OptionB, instances[0].option).number == 2
    assert cast(OptionB, instances[1].option).number == 3

    # Constrain one member and keep another with defaults
    instances = list(
        _enumerate_instances(
            UnionConfig,
            option={OptionA: {"value": True}, OptionB: {}},
        )
    )
    assert len(instances) == 4  # 1 from OptionA + 3 from OptionB
    assert isinstance(instances[0].option, OptionA)
    assert instances[0].option.value is True
    assert all(isinstance(i.option, OptionB) for i in instances[1:])
    assert [cast(OptionB, i.option).number for i in instances[1:]] == [1, 2, 3]


def test_enumerate_isas():
    ctx = AQREGateBased(gate_time=50, measurement_time=100).context()

    # This will enumerate the 4 ISAs for the error correction code
    count = sum(1 for _ in SurfaceCode.q().enumerate(ctx))
    assert count == 12

    # This will enumerate the 2 ISAs for the error correction code when
    # restricting the domain
    count = sum(1 for _ in SurfaceCode.q(distance=[3, 4]).enumerate(ctx))
    assert count == 2

    # This will enumerate the 3 ISAs for the factory
    count = sum(1 for _ in ExampleFactory.q().enumerate(ctx))
    assert count == 3

    # This will enumerate 36 ISAs for all products between the 12 error
    # correction code ISAs and the 3 factory ISAs
    count = sum(1 for _ in (SurfaceCode.q() * ExampleFactory.q()).enumerate(ctx))
    assert count == 36

    # When providing a list, components are chained (OR operation). This
    # enumerates ISAs from first factory instance OR second factory instance
    count = sum(
        1
        for _ in (
            SurfaceCode.q() * (ExampleFactory.q() + ExampleFactory.q())
        ).enumerate(ctx)
    )
    assert count == 72

    # When providing separate arguments, components are combined via product
    # (AND). This enumerates ISAs from first factory instance AND second
    # factory instance
    count = sum(
        1
        for _ in (SurfaceCode.q() * ExampleFactory.q() * ExampleFactory.q()).enumerate(
            ctx
        )
    )
    assert count == 108

    # Hierarchical factory using from_components: the component receives ISAs
    # from the product of other components as its source
    count = sum(
        1
        for _ in (
            SurfaceCode.q()
            * ExampleLogicalFactory.q(source=(SurfaceCode.q() * ExampleFactory.q()))
        ).enumerate(ctx)
    )
    assert count == 1296


def test_binding_node():
    """Test binding nodes with ISARefNode for component bindings"""
    ctx = AQREGateBased(gate_time=50, measurement_time=100).context()

    # Test basic binding: same code used twice
    # Without binding: 12 codes × 12 codes = 144 combinations
    count_without = sum(1 for _ in (SurfaceCode.q() * SurfaceCode.q()).enumerate(ctx))
    assert count_without == 144

    # With binding: 12 codes (same instance used twice)
    count_with = sum(
        1
        for _ in SurfaceCode.bind("c", ISARefNode("c") * ISARefNode("c")).enumerate(ctx)
    )
    assert count_with == 12

    # Verify the binding works: with binding, both should use same params
    for isa in SurfaceCode.bind("c", ISARefNode("c") * ISARefNode("c")).enumerate(ctx):
        logical_gates = [g for g in isa if g.encoding == LOGICAL]
        # Should have 1 logical gate (LATTICE_SURGERY)
        assert len(logical_gates) == 1

    # Test binding with factories (nested bindings)
    count_without = sum(
        1
        for _ in (
            SurfaceCode.q() * ExampleFactory.q() * SurfaceCode.q() * ExampleFactory.q()
        ).enumerate(ctx)
    )
    assert count_without == 1296  # 12 * 3 * 12 * 3

    count_with = sum(
        1
        for _ in SurfaceCode.bind(
            "c",
            ExampleFactory.bind(
                "f",
                ISARefNode("c") * ISARefNode("f") * ISARefNode("c") * ISARefNode("f"),
            ),
        ).enumerate(ctx)
    )
    assert count_with == 36  # 12 * 3

    # Test binding with from_components equivalent (hierarchical)
    # Without binding: 4 outer codes × (4 inner codes × 3 factories × 3 levels)
    count_without = sum(
        1
        for _ in (
            SurfaceCode.q()
            * ExampleLogicalFactory.q(
                source=(SurfaceCode.q() * ExampleFactory.q()),
            )
        ).enumerate(ctx)
    )
    assert count_without == 1296  # 12 * 12 * 3 * 3

    # With binding: 4 codes (same used twice) × 3 factories × 3 levels
    count_with = sum(
        1
        for _ in SurfaceCode.bind(
            "c",
            ISARefNode("c")
            * ExampleLogicalFactory.q(
                source=(ISARefNode("c") * ExampleFactory.q()),
            ),
        ).enumerate(ctx)
    )
    assert count_with == 108  # 12 * 3 * 3

    # Test binding with kwargs
    count_with_kwargs = sum(
        1
        for _ in SurfaceCode.q(distance=5)
        .bind("c", ISARefNode("c") * ISARefNode("c"))
        .enumerate(ctx)
    )
    assert count_with_kwargs == 1  # Only distance=5

    # Verify kwargs are applied
    for isa in (
        SurfaceCode.q(distance=5)
        .bind("c", ISARefNode("c") * ISARefNode("c"))
        .enumerate(ctx)
    ):
        logical_gates = [g for g in isa if g.encoding == LOGICAL]
        assert all(g.space(1) == 49 for g in logical_gates)

    # Test multiple independent bindings (nested)
    count = sum(
        1
        for _ in SurfaceCode.bind(
            "c1",
            ExampleFactory.bind(
                "c2",
                ISARefNode("c1")
                * ISARefNode("c1")
                * ISARefNode("c2")
                * ISARefNode("c2"),
            ),
        ).enumerate(ctx)
    )
    # 12 codes for c1 × 3 factories for c2
    assert count == 36


def test_binding_node_errors():
    """Test error handling for binding nodes"""
    ctx = AQREGateBased(gate_time=50, measurement_time=100).context()

    # Test ISARefNode enumerate with undefined binding raises ValueError
    try:
        list(ISARefNode("test").enumerate(ctx))
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "Undefined component reference: 'test'" in str(e)


def test_product_isa_enumeration_nodes():
    from qsharp.qre._isa_enumeration import _ComponentQuery, _ProductNode

    terminal = SurfaceCode.q()
    query = terminal * terminal

    # Multiplication should create ProductNode
    assert isinstance(query, _ProductNode)
    assert len(query.sources) == 2
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Multiplying again should extend the sources
    query = query * terminal
    assert isinstance(query, _ProductNode)
    assert len(query.sources) == 3
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Also from the other side
    query = terminal * query
    assert isinstance(query, _ProductNode)
    assert len(query.sources) == 4
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Also for two ProductNodes
    query = query * query
    assert isinstance(query, _ProductNode)
    assert len(query.sources) == 8
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)


def test_sum_isa_enumeration_nodes():
    from qsharp.qre._isa_enumeration import _ComponentQuery, _SumNode

    terminal = SurfaceCode.q()
    query = terminal + terminal

    # Multiplication should create SumNode
    assert isinstance(query, _SumNode)
    assert len(query.sources) == 2
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Multiplying again should extend the sources
    query = query + terminal
    assert isinstance(query, _SumNode)
    assert len(query.sources) == 3
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Also from the other side
    query = terminal + query
    assert isinstance(query, _SumNode)
    assert len(query.sources) == 4
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)

    # Also for two SumNodes
    query = query + query
    assert isinstance(query, _SumNode)
    assert len(query.sources) == 8
    for source in query.sources:
        assert isinstance(source, _ComponentQuery)


def test_trace_properties():
    trace = Trace(42)

    INT = 0
    FLOAT = 1
    BOOL = 2
    STR = 3

    trace.set_property(INT, 42)
    assert trace.get_property(INT) == 42
    assert isinstance(trace.get_property(INT), int)

    trace.set_property(FLOAT, 3.14)
    assert trace.get_property(FLOAT) == 3.14
    assert isinstance(trace.get_property(FLOAT), float)

    trace.set_property(BOOL, True)
    assert trace.get_property(BOOL) is True
    assert isinstance(trace.get_property(BOOL), bool)

    trace.set_property(STR, "hello")
    assert trace.get_property(STR) == "hello"
    assert isinstance(trace.get_property(STR), str)


def test_qsharp_application():
    from qsharp.qre._enumeration import _enumerate_instances

    code = """
    {{
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }}
    """

    app = QSharpApplication(code)
    trace = app.get_trace()

    assert trace.compute_qubits == 3
    assert trace.depth == 3
    assert trace.resource_states == {}

    graph = _ProvenanceGraph()
    isa = graph.make_isa(
        [
            graph.add_instruction(
                LATTICE_SURGERY,
                encoding=LOGICAL,
                arity=None,
                time=1000,
                space=linear_function(50),
                error_rate=linear_function(1e-6),
            ),
            graph.add_instruction(
                T, encoding=LOGICAL, time=1000, space=400, error_rate=1e-8
            ),
            graph.add_instruction(
                CCX, encoding=LOGICAL, time=2000, space=800, error_rate=1e-10
            ),
        ]
    )

    # Properties from the program
    counts = qsharp.logical_counts(code)
    num_ts = counts["tCount"]
    num_ccx = counts["cczCount"]
    num_rotations = counts["rotationCount"]
    rotation_depth = counts["rotationDepth"]

    lattice_surgery = LatticeSurgery()

    counter = 0
    for psspc in _enumerate_instances(PSSPC):
        counter += 1
        trace2 = psspc.transform(trace)
        assert trace2 is not None
        trace2 = lattice_surgery.transform(trace2)
        assert trace2 is not None
        assert trace2.compute_qubits == 12
        assert (
            trace2.depth
            == num_ts
            + num_ccx * 3
            + num_rotations
            + rotation_depth * psspc.num_ts_per_rotation
        )
        if psspc.ccx_magic_states:
            assert trace2.resource_states == {
                T: num_ts + psspc.num_ts_per_rotation * num_rotations,
                CCX: num_ccx,
            }
        else:
            assert trace2.resource_states == {
                T: num_ts + psspc.num_ts_per_rotation * num_rotations + 4 * num_ccx
            }
        result = trace2.estimate(isa, max_error=float("inf"))
        assert result is not None
        _assert_estimation_result(trace2, result, isa)
    assert counter == 32


def test_application_enumeration():
    @dataclass(kw_only=True)
    class _Params:
        size: int = field(default=1, metadata={"domain": range(1, 4)})

    class TestApp(Application[_Params]):
        def get_trace(self, parameters: _Params) -> Trace:
            return Trace(parameters.size)

    app = TestApp()
    assert sum(1 for _ in TestApp.q().enumerate(app.context())) == 3
    assert sum(1 for _ in TestApp.q(size=1).enumerate(app.context())) == 1
    assert sum(1 for _ in TestApp.q(size=[4, 5]).enumerate(app.context())) == 2


def test_trace_enumeration():
    code = """
    {{
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }}
    """

    app = QSharpApplication(code)

    ctx = app.context()
    assert sum(1 for _ in QSharpApplication.q().enumerate(ctx)) == 1

    assert sum(1 for _ in PSSPC.q().enumerate(ctx)) == 32

    assert sum(1 for _ in LatticeSurgery.q().enumerate(ctx)) == 1

    q = PSSPC.q() * LatticeSurgery.q()
    assert sum(1 for _ in q.enumerate(ctx)) == 32


def test_rotation_error_psspc():
    from qsharp.qre._enumeration import _enumerate_instances

    # This test helps to bound the variables for the number of rotations in PSSPC

    # Create a trace with a single rotation gate and ensure that the base error
    # after PSSPC transformation is less than 1.
    trace = Trace(1)
    trace.add_operation(RZ, [0])

    for psspc in _enumerate_instances(PSSPC, ccx_magic_states=False):
        transformed = psspc.transform(trace)
        assert transformed is not None
        assert (
            transformed.base_error < 1.0
        ), f"Base error too high: {transformed.base_error} for {psspc.num_ts_per_rotation} T states per rotation"


def test_estimation_max_error():
    from qsharp.estimator import LogicalCounts

    app = QSharpApplication(LogicalCounts({"numQubits": 100, "measurementCount": 100}))
    arch = AQREGateBased(gate_time=50, measurement_time=100)

    for max_error in [1e-1, 1e-2, 1e-3, 1e-4]:
        results = estimate(
            app,
            arch,
            SurfaceCode.q() * ExampleFactory.q(),
            PSSPC.q() * LatticeSurgery.q(),
            max_error=max_error,
        )

        assert len(results) == 1
        assert next(iter(results)).error <= max_error


def _assert_estimation_result(trace: Trace, result: EstimationResult, isa: ISA):
    actual_qubits = (
        isa[LATTICE_SURGERY].expect_space(trace.compute_qubits)
        + isa[T].expect_space() * result.factories[T].copies
    )
    if CCX in trace.resource_states:
        actual_qubits += isa[CCX].expect_space() * result.factories[CCX].copies
    assert result.qubits == actual_qubits

    assert (
        result.runtime
        == isa[LATTICE_SURGERY].expect_time(trace.compute_qubits) * trace.depth
    )

    actual_error = (
        trace.base_error
        + isa[LATTICE_SURGERY].expect_error_rate(trace.compute_qubits) * trace.depth
        + isa[T].expect_error_rate() * result.factories[T].states
    )
    if CCX in trace.resource_states:
        actual_error += isa[CCX].expect_error_rate() * result.factories[CCX].states
    assert abs(result.error - actual_error) <= 1e-8


# --- EstimationTable tests ---


def _make_entry(qubits, runtime, error, properties=None):
    """Helper to create an EstimationTableEntry with a dummy InstructionSource."""
    return EstimationTableEntry(
        qubits=qubits,
        runtime=runtime,
        error=error,
        source=InstructionSource(),
        properties=properties or {},
    )


def test_estimation_table_default_columns():
    """Test that a new EstimationTable has the three default columns."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error"]
    assert frame["qubits"][0] == 100
    assert frame["runtime"][0] == pd.Timedelta(5000, unit="ns")
    assert frame["error"][0] == 0.01


def test_estimation_table_multiple_rows():
    """Test as_frame with multiple entries."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))
    table.append(_make_entry(200, 10000, 0.02))
    table.append(_make_entry(300, 15000, 0.03))

    frame = table.as_frame()
    assert len(frame) == 3
    assert list(frame["qubits"]) == [100, 200, 300]
    assert list(frame["error"]) == [0.01, 0.02, 0.03]


def test_estimation_table_empty():
    """Test as_frame with no entries produces an empty DataFrame."""
    table = EstimationTable()
    frame = table.as_frame()
    assert len(frame) == 0


def test_estimation_table_add_column():
    """Test adding a column to the table."""
    VAL = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={VAL: 42}))
    table.append(_make_entry(200, 10000, 0.02, properties={VAL: 84}))

    table.add_column("val", lambda e: e.properties[VAL])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "val"]
    assert list(frame["val"]) == [42, 84]


def test_estimation_table_add_column_with_formatter():
    """Test adding a column with a formatter."""
    NS = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NS: 1000}))

    table.add_column(
        "duration",
        lambda e: e.properties[NS],
        formatter=lambda x: pd.Timedelta(x, unit="ns"),
    )

    frame = table.as_frame()
    assert frame["duration"][0] == pd.Timedelta(1000, unit="ns")


def test_estimation_table_add_multiple_columns():
    """Test adding multiple columns preserves order."""
    A = 0
    B = 1
    C = 2

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={A: 1, B: 2, C: 3}))

    table.add_column("a", lambda e: e.properties[A])
    table.add_column("b", lambda e: e.properties[B])
    table.add_column("c", lambda e: e.properties[C])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "a", "b", "c"]
    assert frame["a"][0] == 1
    assert frame["b"][0] == 2
    assert frame["c"][0] == 3


def test_estimation_table_insert_column_at_beginning():
    """Test inserting a column at index 0."""
    NAME = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NAME: "test"}))

    table.insert_column(0, "name", lambda e: e.properties[NAME])

    frame = table.as_frame()
    assert list(frame.columns) == ["name", "qubits", "runtime", "error"]
    assert frame["name"][0] == "test"


def test_estimation_table_insert_column_in_middle():
    """Test inserting a column between existing default columns."""
    EXTRA = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={EXTRA: 99}))

    # Insert between qubits and runtime (index 1)
    table.insert_column(1, "extra", lambda e: e.properties[EXTRA])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "extra", "runtime", "error"]
    assert frame["extra"][0] == 99


def test_estimation_table_insert_column_at_end():
    """Test inserting a column at the end (same effect as add_column)."""
    LAST = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={LAST: True}))

    # 3 default columns, inserting at index 3 = end
    table.insert_column(3, "last", lambda e: e.properties[LAST])

    frame = table.as_frame()
    assert list(frame.columns) == ["qubits", "runtime", "error", "last"]
    assert frame["last"][0]


def test_estimation_table_insert_column_with_formatter():
    """Test inserting a column with a formatter."""
    NS = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={NS: 2000}))

    table.insert_column(
        0,
        "custom_time",
        lambda e: e.properties[NS],
        formatter=lambda x: pd.Timedelta(x, unit="ns"),
    )

    frame = table.as_frame()
    assert frame["custom_time"][0] == pd.Timedelta(2000, unit="ns")
    assert list(frame.columns)[0] == "custom_time"


def test_estimation_table_insert_and_add_columns():
    """Test combining insert_column and add_column."""
    A = 0
    B = 0

    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01, properties={A: 1, B: 2}))

    table.add_column("b", lambda e: e.properties[B])
    table.insert_column(0, "a", lambda e: e.properties[A])

    frame = table.as_frame()
    assert list(frame.columns) == ["a", "qubits", "runtime", "error", "b"]


def test_estimation_table_factory_summary_no_factories():
    """Test factory summary column when entries have no factories."""
    table = EstimationTable()
    table.append(_make_entry(100, 5000, 0.01))

    table.add_factory_summary_column()

    frame = table.as_frame()
    assert "factories" in frame.columns
    assert frame["factories"][0] == "None"


def test_estimation_table_factory_summary_with_estimation():
    """Test factory summary column with real estimation results."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = AQREGateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_factory_summary_column()
    frame = results.as_frame()

    assert "factories" in frame.columns
    # Each result should mention T in the factory summary
    for val in frame["factories"]:
        assert "T" in val


def test_estimation_table_add_column_from_source():
    """Test adding a column that accesses the InstructionSource (like distance)."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = AQREGateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_column(
        "compute_distance",
        lambda entry: entry.source[LATTICE_SURGERY].instruction[DISTANCE],
    )

    frame = results.as_frame()
    assert "compute_distance" in frame.columns
    for d in frame["compute_distance"]:
        assert isinstance(d, int)
        assert d >= 3


def test_estimation_table_add_column_from_properties():
    """Test adding columns that access trace properties from estimation."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = AQREGateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
    )

    assert len(results) >= 1

    results.add_column(
        "num_ts_per_rotation",
        lambda entry: entry.properties[NUM_TS_PER_ROTATION],
    )

    frame = results.as_frame()
    assert "num_ts_per_rotation" in frame.columns
    for val in frame["num_ts_per_rotation"]:
        assert isinstance(val, int)
        assert val >= 1


def test_estimation_table_insert_column_before_defaults():
    """Test inserting a name column before all default columns, similar to the factoring notebook."""
    code = """
    {
        use (a, b, c) = (Qubit(), Qubit(), Qubit());
        T(a);
        CCNOT(a, b, c);
        Rz(1.2345, a);
    }
    """
    app = QSharpApplication(code)
    arch = AQREGateBased(gate_time=50, measurement_time=100)
    results = estimate(
        app,
        arch,
        SurfaceCode.q() * ExampleFactory.q(),
        PSSPC.q() * LatticeSurgery.q(),
        max_error=0.5,
        name="test_experiment",
    )

    assert len(results) >= 1

    # Add a factory summary at the end
    results.add_factory_summary_column()

    frame = results.as_frame()
    assert frame.columns[0] == "name"
    assert frame.columns[-1] == "factories"
    # Default columns should still be in order
    assert list(frame.columns[1:4]) == ["qubits", "runtime", "error"]


def test_estimation_table_as_frame_sortable():
    """Test that the DataFrame from as_frame can be sorted, as done in the factoring tests."""
    table = EstimationTable()
    table.append(_make_entry(300, 15000, 0.03))
    table.append(_make_entry(100, 5000, 0.01))
    table.append(_make_entry(200, 10000, 0.02))

    frame = table.as_frame()
    sorted_frame = frame.sort_values(by=["qubits", "runtime"]).reset_index(drop=True)

    assert list(sorted_frame["qubits"]) == [100, 200, 300]
    assert list(sorted_frame["error"]) == [0.01, 0.02, 0.03]


def test_estimation_table_computed_column():
    """Test adding a column that computes a derived value from the entry."""
    table = EstimationTable()
    table.append(_make_entry(100, 5_000_000, 0.01))
    table.append(_make_entry(200, 10_000_000, 0.02))

    # Compute qubits * error as a derived metric
    table.add_column("qubit_error_product", lambda e: e.qubits * e.error)

    frame = table.as_frame()
    assert frame["qubit_error_product"][0] == pytest.approx(1.0)
    assert frame["qubit_error_product"][1] == pytest.approx(4.0)


def test_estimation_table_plot_returns_figure():
    """Test that plot() returns a matplotlib Figure with correct axes."""
    from matplotlib.figure import Figure

    table = EstimationTable()
    table.append(_make_entry(100, 5_000_000_000, 0.01))
    table.append(_make_entry(200, 10_000_000_000, 0.02))
    table.append(_make_entry(50, 50_000_000_000, 0.005))

    fig = table.plot()

    assert isinstance(fig, Figure)
    ax = fig.axes[0]
    assert ax.get_ylabel() == "Physical qubits"
    assert ax.get_xlabel() == "Runtime"
    assert ax.get_xscale() == "log"
    assert ax.get_yscale() == "log"

    # Verify data points
    offsets = ax.collections[0].get_offsets()
    assert len(cast(Sized, offsets)) == 3


def test_estimation_table_plot_empty_raises():
    """Test that plot() raises ValueError on an empty table."""
    table = EstimationTable()
    with pytest.raises(ValueError, match="Cannot plot an empty EstimationTable"):
        table.plot()


def test_estimation_table_plot_single_entry():
    """Test that plot() works with a single entry."""
    from matplotlib.figure import Figure

    table = EstimationTable()
    table.append(_make_entry(100, 1_000_000, 0.01))

    fig = table.plot()
    assert isinstance(fig, Figure)

    offsets = fig.axes[0].collections[0].get_offsets()
    assert len(cast(Sized, offsets)) == 1


def test_estimation_table_plot_with_runtime_unit():
    """Test that plot(runtime_unit=...) scales x values and labels the axis."""
    table = EstimationTable()
    # 1 hour = 3600e9 ns, 2 hours = 7200e9 ns
    table.append(_make_entry(100, int(3600e9), 0.01))
    table.append(_make_entry(200, int(7200e9), 0.02))

    fig = table.plot(runtime_unit="hours")

    ax = fig.axes[0]
    assert ax.get_xlabel() == "Runtime (hours)"

    # Verify the x data is scaled: should be 1.0 and 2.0 hours
    offsets = cast(list, ax.collections[0].get_offsets())
    assert offsets[0][0] == pytest.approx(1.0)
    assert offsets[1][0] == pytest.approx(2.0)


def test_estimation_table_plot_invalid_runtime_unit():
    """Test that plot() raises ValueError for an unknown runtime_unit."""
    table = EstimationTable()
    table.append(_make_entry(100, 1000, 0.01))
    with pytest.raises(ValueError, match="Unknown runtime_unit"):
        table.plot(runtime_unit="fortnights")


def _ll_files():
    ll_dir = (
        Path(__file__).parent.parent
        / "tests-integration"
        / "resources"
        / "adaptive_ri"
        / "output"
    )
    return sorted(ll_dir.glob("*.ll"))


@pytest.mark.parametrize("ll_file", _ll_files(), ids=lambda p: p.stem)
def test_trace_from_qir(ll_file):
    # NOTE: This test is primarily to ensure that the function can parse real
    # QIR output without errors, rather than checking specific properties of the
    # trace.
    try:
        trace_from_qir(ll_file.read_text())
    except ValueError as e:
        # The only reason of failure is presence of control flow
        assert (
            str(e)
            == "simulation of programs with branching control flow is not supported"
        )


def test_trace_from_qir_handles_all_instruction_ids():
    """Verify that trace_from_qir handles every QirInstructionId except CorrelatedNoise.

    Generates a synthetic QIR program containing one instance of each gate
    intrinsic recognised by AggregateGatesPass and asserts that trace_from_qir
    processes all of them without error.
    """
    import pyqir
    import pyqir.qis as qis
    from qsharp._native import QirInstructionId
    from qsharp.qre.interop._qir import _GATE_MAP, _MEAS_MAP, _SKIP

    # -- Completeness check: every QirInstructionId must be covered --------
    handled_ids = (
        [qir_id for qir_id, _, _ in _GATE_MAP]
        + [qir_id for qir_id, _ in _MEAS_MAP]
        + list(_SKIP)
    )
    # Exhaustive list of all QirInstructionId variants (pyo3 enums are not iterable)
    all_ids = [
        QirInstructionId.I,
        QirInstructionId.H,
        QirInstructionId.X,
        QirInstructionId.Y,
        QirInstructionId.Z,
        QirInstructionId.S,
        QirInstructionId.SAdj,
        QirInstructionId.SX,
        QirInstructionId.SXAdj,
        QirInstructionId.T,
        QirInstructionId.TAdj,
        QirInstructionId.CNOT,
        QirInstructionId.CX,
        QirInstructionId.CY,
        QirInstructionId.CZ,
        QirInstructionId.CCX,
        QirInstructionId.SWAP,
        QirInstructionId.RX,
        QirInstructionId.RY,
        QirInstructionId.RZ,
        QirInstructionId.RXX,
        QirInstructionId.RYY,
        QirInstructionId.RZZ,
        QirInstructionId.RESET,
        QirInstructionId.M,
        QirInstructionId.MResetZ,
        QirInstructionId.MZ,
        QirInstructionId.Move,
        QirInstructionId.ReadResult,
        QirInstructionId.ResultRecordOutput,
        QirInstructionId.BoolRecordOutput,
        QirInstructionId.IntRecordOutput,
        QirInstructionId.DoubleRecordOutput,
        QirInstructionId.TupleRecordOutput,
        QirInstructionId.ArrayRecordOutput,
        QirInstructionId.CorrelatedNoise,
    ]
    unhandled = [
        i
        for i in all_ids
        if i not in handled_ids and i != QirInstructionId.CorrelatedNoise
    ]
    assert unhandled == [], (
        f"QirInstructionId values not covered by _GATE_MAP, _MEAS_MAP, or _SKIP: "
        f"{', '.join(str(i) for i in unhandled)}"
    )

    # -- Generate a QIR program with every producible gate -----------------
    simple = pyqir.SimpleModule("test_all_gates", num_qubits=4, num_results=3)
    builder = simple.builder
    ctx = simple.context
    q = simple.qubits
    r = simple.results

    void_ty = pyqir.Type.void(ctx)
    qubit_ty = pyqir.qubit_type(ctx)
    result_ty = pyqir.result_type(ctx)
    double_ty = pyqir.Type.double(ctx)
    i64_ty = pyqir.IntType(ctx, 64)

    def declare(name, param_types):
        return simple.add_external_function(
            name, pyqir.FunctionType(void_ty, param_types)
        )

    # Single-qubit gates (pyqir.qis builtins)
    qis.h(builder, q[0])
    qis.x(builder, q[0])
    qis.y(builder, q[0])
    qis.z(builder, q[0])
    qis.s(builder, q[0])
    qis.s_adj(builder, q[0])
    qis.t(builder, q[0])
    qis.t_adj(builder, q[0])

    # SX — not in pyqir.qis
    sx_fn = declare("__quantum__qis__sx__body", [qubit_ty])
    builder.call(sx_fn, [q[0]])

    # Two-qubit gates (qis.cx emits __quantum__qis__cnot__body which the
    # pass does not handle, so use builder.call with the correct name)
    cx_fn = declare("__quantum__qis__cx__body", [qubit_ty, qubit_ty])
    builder.call(cx_fn, [q[0], q[1]])
    qis.cz(builder, q[0], q[1])
    qis.swap(builder, q[0], q[1])

    cy_fn = declare("__quantum__qis__cy__body", [qubit_ty, qubit_ty])
    builder.call(cy_fn, [q[0], q[1]])

    # Three-qubit gate
    qis.ccx(builder, q[0], q[1], q[2])

    # Single-qubit rotations
    qis.rx(builder, 1.0, q[0])
    qis.ry(builder, 1.0, q[0])
    qis.rz(builder, 1.0, q[0])

    # Two-qubit rotations — not in pyqir.qis
    rot2_ty = [double_ty, qubit_ty, qubit_ty]
    angle = pyqir.const(double_ty, 1.0)
    for name in ("rxx", "ryy", "rzz"):
        fn = declare(f"__quantum__qis__{name}__body", rot2_ty)
        builder.call(fn, [angle, q[0], q[1]])

    # Measurements
    qis.mz(builder, q[0], r[0])

    m_fn = declare("__quantum__qis__m__body", [qubit_ty, result_ty])
    builder.call(m_fn, [q[1], r[1]])

    mresetz_fn = declare("__quantum__qis__mresetz__body", [qubit_ty, result_ty])
    builder.call(mresetz_fn, [q[2], r[2]])

    # Reset / Move
    qis.reset(builder, q[0])

    move_fn = declare("__quantum__qis__move__body", [qubit_ty])
    builder.call(move_fn, [q[0]])

    # Output recording
    tag = simple.add_byte_string(b"tag")
    arr_fn = declare("__quantum__rt__array_record_output", [i64_ty, tag.type])
    builder.call(arr_fn, [pyqir.const(i64_ty, 1), tag])

    rec_fn = declare("__quantum__rt__result_record_output", [result_ty, tag.type])
    builder.call(rec_fn, [r[0], tag])

    tup_fn = declare("__quantum__rt__tuple_record_output", [i64_ty, tag.type])
    builder.call(tup_fn, [pyqir.const(i64_ty, 1), tag])

    # -- Run trace_from_qir and verify it succeeds -------------------------
    trace = trace_from_qir(simple.ir())
    assert trace is not None


@pytest.mark.skipif(
    "SLOW_TESTS" not in os.environ,
    reason="turn on slow tests by setting SLOW_TESTS=1 in the environment",
)
@pytest.mark.parametrize(
    "post_process, use_graph",
    [
        (False, False),
        (True, False),
        (False, True),
        (True, True),
    ],
)
def test_estimation_methods(post_process, use_graph):
    counts = LogicalCounts(
        {
            "numQubits": 1000,
            "tCount": 1_500_000,
            "rotationCount": 0,
            "rotationDepth": 0,
            "cczCount": 1_000_000_000,
            "ccixCount": 0,
            "measurementCount": 25_000_000,
            "numComputeQubits": 200,
            "readFromMemoryCount": 30_000_000,
            "writeToMemoryCount": 30_000_000,
        }
    )

    trace_query = PSSPC.q() * LatticeSurgery.q(slow_down_factor=[1.0, 2.0])
    isa_query = (
        SurfaceCode.q()
        * RoundBasedFactory.q()
        * TwoDimensionalYokedSurfaceCode.q(source=SurfaceCode.q())
    )

    app = QSharpApplication(counts)
    arch = AQREGateBased(gate_time=50, measurement_time=100)

    results = estimate(
        app,
        arch,
        isa_query,
        trace_query,
        max_error=1 / 3,
        post_process=post_process,
        use_graph=use_graph,
    )
    results.add_factory_summary_column()

    assert [(result.qubits, result.runtime) for result in results] == [
        (238707, 23997050000000),
        (240407, 11998525000000),
    ]

    print()
    print(results.stats)


def test_rotation_buckets():
    from qsharp.qre.interop._qsharp import _bucketize_rotation_counts

    print()

    r_count = 15066
    r_depth = 14756
    q_count = 291

    result = _bucketize_rotation_counts(r_count, r_depth)

    a_count = 0
    a_depth = 0
    for c, d in result:
        print(c, d)
        assert c <= q_count
        assert c > 0
        a_count += c * d
        a_depth += d

    assert a_count == r_count
    assert a_depth == r_depth
