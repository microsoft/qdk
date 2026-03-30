# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from dataclasses import KW_ONLY, dataclass, field
from enum import Enum
from typing import cast

import pytest

from qsharp.qre import LOGICAL
from qsharp.qre.models import SurfaceCode, GateBased
from qsharp.qre._isa_enumeration import (
    ISARefNode,
    _ComponentQuery,
    _ProductNode,
    _SumNode,
)

from .conftest import ExampleFactory, ExampleLogicalFactory


def test_enumerate_instances():
    """Test enumeration of SurfaceCode instances with default and custom domains."""
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
    """Test that boolean dataclass fields enumerate both True and False."""
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
    """Test that Enum dataclass fields enumerate all members."""
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
    """Test that a field with no domain and no default raises ValueError."""
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class InvalidConfig:
        _: KW_ONLY
        # This field has no domain, is not bool/enum, and has no default
        value: int

    with pytest.raises(ValueError, match="Cannot enumerate field value"):
        list(_enumerate_instances(InvalidConfig))


def test_enumerate_instances_single():
    """Test enumeration of a dataclass with a single non-kw-only field."""
    from qsharp.qre._enumeration import _enumerate_instances

    @dataclass
    class SingleConfig:
        value: int = 42

    instances = list(_enumerate_instances(SingleConfig))
    assert len(instances) == 1
    assert instances[0].value == 42


def test_enumerate_instances_literal():
    """Test that Literal-typed fields enumerate their allowed values."""
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
    """Test enumeration of nested dataclass fields."""
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
    """Test enumeration of union-typed dataclass fields."""
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
    """Test constraining nested dataclass fields via a dict."""
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
    """Test restricting a union field to a single member type."""
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
    """Test restricting a union field to a subset of member types."""
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
    """Test constraining union field members via a type-to-kwargs dict."""
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
    """Test ISA enumeration with products, sums, and hierarchical factories."""
    ctx = GateBased(gate_time=50, measurement_time=100).context()

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
    ctx = GateBased(gate_time=50, measurement_time=100).context()

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
    ctx = GateBased(gate_time=50, measurement_time=100).context()

    # Test ISARefNode enumerate with undefined binding raises ValueError
    try:
        list(ISARefNode("test").enumerate(ctx))
        assert False, "Should have raised ValueError"
    except ValueError as e:
        assert "Undefined component reference: 'test'" in str(e)


def test_product_isa_enumeration_nodes():
    """Test that multiplying ISAQuery nodes produces flattened ProductNodes."""
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
    """Test that adding ISAQuery nodes produces flattened SumNodes."""
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
