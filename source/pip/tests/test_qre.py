# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from enum import Enum
from typing import Generator

from qsharp.qre import (
    ISA,
    LOGICAL,
    Architecture,
    ConstraintBound,
    ISARequirements,
    ISATransform,
    constraint,
    instruction,
    linear_function,
)
from qsharp.qre._enumeration import _enumerate_instances
from qsharp.qre._isa_enumeration import (
    BindingNode,
    Context,
    ISAQuery,
    ISARefNode,
    ProductNode,
    SumNode,
)
from qsharp.qre.instruction_ids import (
    CNOT,
    GENERIC,
    LATTICE_SURGERY,
    MEAS_Z,
    TWO_QUBIT_CLIFFORD,
    H,
    T,
)

# NOTE These classes will be generalized as part of the QRE API in the following
# pull requests and then moved out of the tests.


class ExampleArchitecture(Architecture):
    @property
    def provided_isa(self) -> ISA:
        return ISA(
            instruction(H, time=50, error_rate=1e-3),
            instruction(CNOT, arity=2, time=50, error_rate=1e-3),
            instruction(MEAS_Z, time=100, error_rate=1e-3),
            instruction(TWO_QUBIT_CLIFFORD, arity=2, time=50, error_rate=1e-3),
            instruction(GENERIC, time=50, error_rate=1e-4),
            instruction(T, time=50, error_rate=1e-4),
        )


@dataclass
class SurfaceCode(ISATransform):
    _: KW_ONLY
    distance: int = field(default=3, metadata={"domain": range(3, 26, 2)})

    @staticmethod
    def required_isa() -> ISARequirements:
        return ISARequirements(
            constraint(H, error_rate=ConstraintBound.lt(0.01)),
            constraint(CNOT, arity=2, error_rate=ConstraintBound.lt(0.01)),
            constraint(MEAS_Z, error_rate=ConstraintBound.lt(0.01)),
        )

    def provided_isa(self, impl_isa: ISA) -> Generator[ISA, None, None]:
        crossing_prefactor: float = 0.03
        error_correction_threshold: float = 0.01

        cnot_time = impl_isa[CNOT].expect_time()
        h_time = impl_isa[H].expect_time()
        meas_time = impl_isa[MEAS_Z].expect_time()

        physical_error_rate = max(
            impl_isa[CNOT].expect_error_rate(),
            impl_isa[H].expect_error_rate(),
            impl_isa[MEAS_Z].expect_error_rate(),
        )

        space_formula = linear_function(2 * self.distance**2)

        time_value = (h_time + meas_time + cnot_time * 4) * self.distance

        error_formula = linear_function(
            crossing_prefactor
            * (
                (physical_error_rate / error_correction_threshold)
                ** ((self.distance + 1) // 2)
            )
        )

        yield ISA(
            instruction(
                GENERIC,
                encoding=LOGICAL,
                arity=None,
                space=space_formula,
                time=time_value,
                error_rate=error_formula,
            ),
            instruction(
                LATTICE_SURGERY,
                encoding=LOGICAL,
                arity=None,
                space=space_formula,
                time=time_value,
                error_rate=error_formula,
            ),
        )


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
    arch = ExampleArchitecture()
    code = SurfaceCode()

    # Verify that the architecture satisfies the code requirements
    assert arch.provided_isa.satisfies(SurfaceCode.required_isa())

    # Generate logical ISAs
    isas = list(code.provided_isa(arch.provided_isa))

    # There is one ISA with two instructions
    assert len(isas) == 1
    assert len(isas[0]) == 2


def test_enumerate_instances():
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
    @dataclass
    class BoolConfig:
        _: KW_ONLY
        flag: bool

    instances = list(_enumerate_instances(BoolConfig))
    assert len(instances) == 2
    assert instances[0].flag is True
    assert instances[1].flag is False


def test_enumerate_instances_enum():
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
    import pytest

    @dataclass
    class InvalidConfig:
        _: KW_ONLY
        # This field has no domain, is not bool/enum, and has no default
        value: int

    with pytest.raises(ValueError, match="Cannot enumerate field value"):
        list(_enumerate_instances(InvalidConfig))


def test_enumerate_instances_single():
    @dataclass
    class SingleConfig:
        value: int = 42

    instances = list(_enumerate_instances(SingleConfig))
    assert len(instances) == 1
    assert instances[0].value == 42


def test_enumerate_instances_literal():
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
    ctx = Context(architecture=ExampleArchitecture())

    # This will enumerate the 4 ISAs for the error correction code
    count = sum(1 for _ in ISAQuery(SurfaceCode).enumerate(ctx))
    assert count == 12

    # This will enumerate the 2 ISAs for the error correction code when
    # restricting the domain
    count = sum(
        1 for _ in ISAQuery(SurfaceCode, kwargs={"distance": [3, 5]}).enumerate(ctx)
    )
    assert count == 2

    # This will enumerate the 3 ISAs for the factory
    count = sum(1 for _ in ISAQuery(ExampleFactory).enumerate(ctx))
    assert count == 3

    # This will enumerate 36 ISAs for all products between the 12 error
    # correction code ISAs and the 3 factory ISAs
    count = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(ExampleFactory),
            ]
        ).enumerate(ctx)
    )
    assert count == 36

    # When providing a list, components are chained (OR operation). This
    # enumerates ISAs from first factory instance OR second factory instance
    count = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                SumNode(
                    sources=[
                        ISAQuery(ExampleFactory),
                        ISAQuery(ExampleFactory),
                    ]
                ),
            ]
        ).enumerate(ctx)
    )
    assert count == 72

    # When providing separate arguments, components are combined via product
    # (AND). This enumerates ISAs from first factory instance AND second
    # factory instance
    count = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(ExampleFactory),
                ISAQuery(ExampleFactory),
            ]
        ).enumerate(ctx)
    )
    assert count == 108

    # Hierarchical factory using from_components: the component receives ISAs
    # from the product of other components as its source
    count = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(
                    ExampleLogicalFactory,
                    source=ProductNode(
                        sources=[
                            ISAQuery(SurfaceCode),
                            ISAQuery(ExampleFactory),
                        ]
                    ),
                ),
            ]
        ).enumerate(ctx)
    )
    assert count == 1296


def test_binding_node():
    """Test BindingNode with ISARefNode for component bindings"""
    ctx = Context(architecture=ExampleArchitecture())

    # Test basic binding: same code used twice
    # Without binding: 12 codes × 12 codes = 144 combinations
    count_without = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(SurfaceCode),
            ]
        ).enumerate(ctx)
    )
    assert count_without == 144

    # With binding: 12 codes (same instance used twice)
    count_with = sum(
        1
        for _ in BindingNode(
            name="c",
            component=ISAQuery(SurfaceCode),
            node=ProductNode(
                sources=[ISARefNode("c"), ISARefNode("c")],
            ),
        ).enumerate(ctx)
    )
    assert count_with == 12

    # Verify the binding works: with binding, both should use same params
    for isa in BindingNode(
        name="c",
        component=ISAQuery(SurfaceCode),
        node=ProductNode(
            sources=[ISARefNode("c"), ISARefNode("c")],
        ),
    ).enumerate(ctx):
        logical_gates = [g for g in isa if g.encoding == LOGICAL]
        # Should have 2 logical gates (GENERIC and LATTICE_SURGERY)
        assert len(logical_gates) == 2

    # Test binding with factories (nested bindings)
    count_without = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(ExampleFactory),
                ISAQuery(SurfaceCode),
                ISAQuery(ExampleFactory),
            ]
        ).enumerate(ctx)
    )
    assert count_without == 1296  # 12 * 3 * 12 * 3

    count_with = sum(
        1
        for _ in BindingNode(
            name="c",
            component=ISAQuery(SurfaceCode),
            node=BindingNode(
                name="f",
                component=ISAQuery(ExampleFactory),
                node=ProductNode(
                    sources=[
                        ISARefNode("c"),
                        ISARefNode("f"),
                        ISARefNode("c"),
                        ISARefNode("f"),
                    ],
                ),
            ),
        ).enumerate(ctx)
    )
    assert count_with == 36  # 12 * 3

    # Test binding with from_components equivalent (hierarchical)
    # Without binding: 4 outer codes × (4 inner codes × 3 factories × 3 levels)
    count_without = sum(
        1
        for _ in ProductNode(
            sources=[
                ISAQuery(SurfaceCode),
                ISAQuery(
                    ExampleLogicalFactory,
                    source=ProductNode(
                        sources=[
                            ISAQuery(SurfaceCode),
                            ISAQuery(ExampleFactory),
                        ]
                    ),
                ),
            ]
        ).enumerate(ctx)
    )
    assert count_without == 1296  # 12 * 12 * 3 * 3

    # With binding: 4 codes (same used twice) × 3 factories × 3 levels
    count_with = sum(
        1
        for _ in BindingNode(
            name="c",
            component=ISAQuery(SurfaceCode),
            node=ProductNode(
                sources=[
                    ISARefNode("c"),
                    ISAQuery(
                        ExampleLogicalFactory,
                        source=ProductNode(
                            sources=[
                                ISARefNode("c"),
                                ISAQuery(ExampleFactory),
                            ]
                        ),
                    ),
                ]
            ),
        ).enumerate(ctx)
    )
    assert count_with == 108  # 12 * 3 * 3

    # Test binding with kwargs
    count_with_kwargs = sum(
        1
        for _ in BindingNode(
            name="c",
            component=ISAQuery(SurfaceCode, kwargs={"distance": 5}),
            node=ProductNode(
                sources=[ISARefNode("c"), ISARefNode("c")],
            ),
        ).enumerate(ctx)
    )
    assert count_with_kwargs == 1  # Only distance=5

    # Verify kwargs are applied
    for isa in BindingNode(
        name="c",
        component=ISAQuery(SurfaceCode, kwargs={"distance": 5}),
        node=ProductNode(
            sources=[ISARefNode("c"), ISARefNode("c")],
        ),
    ).enumerate(ctx):
        logical_gates = [g for g in isa if g.encoding == LOGICAL]
        assert all(g.space(1) == 50 for g in logical_gates)

    # Test multiple independent bindings (nested)
    count = sum(
        1
        for _ in BindingNode(
            name="c1",
            component=ISAQuery(SurfaceCode),
            node=BindingNode(
                name="c2",
                component=ISAQuery(ExampleFactory),
                node=ProductNode(
                    sources=[
                        ISARefNode("c1"),
                        ISARefNode("c1"),
                        ISARefNode("c2"),
                        ISARefNode("c2"),
                    ],
                ),
            ),
        ).enumerate(ctx)
    )
    # 12 codes for c1 × 3 factories for c2
    assert count == 36


def test_binding_node_errors():
    """Test error handling for BindingNode"""
    ctx = Context(architecture=ExampleArchitecture())

    # Test ISARefNode enumerate with undefined binding raises ValueError
    try:
        list(ISARefNode("test").enumerate(ctx))
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "Undefined component reference: 'test'" in str(e)


def test_product_isa_enumeration_nodes():
    terminal = ISAQuery(SurfaceCode)
    query = terminal * terminal

    # Multiplication should create ProductNode
    assert isinstance(query, ProductNode)
    assert len(query.sources) == 2
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Multiplying again should extend the sources
    query = query * terminal
    assert isinstance(query, ProductNode)
    assert len(query.sources) == 3
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Also from the other side
    query = terminal * query
    assert isinstance(query, ProductNode)
    assert len(query.sources) == 4
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Also for two ProductNodes
    query = query * query
    assert isinstance(query, ProductNode)
    assert len(query.sources) == 8
    for source in query.sources:
        assert isinstance(source, ISAQuery)


def test_sum_isa_enumeration_nodes():
    terminal = ISAQuery(SurfaceCode)
    query = terminal + terminal

    # Multiplication should create SumNode
    assert isinstance(query, SumNode)
    assert len(query.sources) == 2
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Multiplying again should extend the sources
    query = query + terminal
    assert isinstance(query, SumNode)
    assert len(query.sources) == 3
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Also from the other side
    query = terminal + query
    assert isinstance(query, SumNode)
    assert len(query.sources) == 4
    for source in query.sources:
        assert isinstance(source, ISAQuery)

    # Also for two SumNodes
    query = query + query
    assert isinstance(query, SumNode)
    assert len(query.sources) == 8
    for source in query.sources:
        assert isinstance(source, ISAQuery)
