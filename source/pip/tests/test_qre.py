# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from enum import Enum
from typing import Generator

import qsharp
from qsharp.qre import (
    ISA,
    LOGICAL,
    PSSPC,
    EstimationResult,
    ISARequirements,
    ISATransform,
    LatticeSurgery,
    QSharpApplication,
    Trace,
    constraint,
    instruction,
    linear_function,
)
from qsharp.qre.models import SurfaceCode, AQREGateBased
from qsharp.qre._isa_enumeration import (
    ISARefNode,
)
from qsharp.qre.instruction_ids import (
    CCX,
    GENERIC,
    LATTICE_SURGERY,
    T,
)

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

    def provided_isa(self, impl_isa: ISA) -> Generator[ISA, None, None]:
        yield ISA(
            instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-8),
        )


@dataclass
class ExampleLogicalFactory(ISATransform):
    _: KW_ONLY
    level: int = field(default=1, metadata={"domain": range(1, 4)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(GENERIC, encoding=LOGICAL),
            constraint(T, encoding=LOGICAL),
        )

    def provided_isa(self, impl_isa: ISA) -> Generator[ISA, None, None]:
        yield ISA(
            instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-10),
        )


def test_isa_from_architecture():
    arch = AQREGateBased()
    code = SurfaceCode()

    # Verify that the architecture satisfies the code requirements
    assert arch.provided_isa.satisfies(SurfaceCode.required_isa())

    # Generate logical ISAs
    isas = list(code.provided_isa(arch.provided_isa))

    # There is one ISA with two instructions
    assert len(isas) == 1
    assert len(isas[0]) == 2


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


def test_enumerate_isas():
    ctx = AQREGateBased().context()

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
    ctx = AQREGateBased().context()

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
        # Should have 2 logical gates (GENERIC and LATTICE_SURGERY)
        assert len(logical_gates) == 2

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
        assert all(g.space(1) == 50 for g in logical_gates)

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
    ctx = AQREGateBased().context()

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

    isa = ISA(
        instruction(
            LATTICE_SURGERY,
            encoding=LOGICAL,
            arity=None,
            time=1000,
            error_rate=linear_function(1e-6),
            space=linear_function(50),
        ),
        instruction(T, encoding=LOGICAL, time=1000, error_rate=1e-8, space=400),
        instruction(CCX, encoding=LOGICAL, time=2000, error_rate=1e-10, space=800),
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
    assert counter == 40


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

    from qsharp.qre._trace import RootNode

    ctx = app.context()
    root = RootNode()
    assert sum(1 for _ in root.enumerate(ctx)) == 1

    assert sum(1 for _ in PSSPC.q().enumerate(ctx)) == 40

    assert sum(1 for _ in LatticeSurgery.q().enumerate(ctx)) == 1

    q = PSSPC.q() * LatticeSurgery.q()
    assert sum(1 for _ in q.enumerate(ctx)) == 40


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
