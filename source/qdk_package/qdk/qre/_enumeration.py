# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import types
from typing import (
    Generator,
    Type,
    TypeVar,
    Literal,
    Union,
    cast,
    get_args,
    get_origin,
    get_type_hints,
)
from dataclasses import MISSING
from itertools import product
from enum import Enum


T = TypeVar("T")


def _is_union_type(tp) -> bool:
    """Check if a type is a Union or Python 3.10+ union (X | Y)."""
    return get_origin(tp) is Union or isinstance(tp, types.UnionType)


def _is_type_filter(val, union_members: tuple) -> bool:
    """
    Check if *val* is a union member type or a list of union member types,
    i.e. a type filter for a union field (as opposed to a fixed value or
    instance domain).
    """
    member_set = set(union_members)
    if isinstance(val, type) and val in member_set:
        return True
    if isinstance(val, list) and all(
        isinstance(v, type) and v in member_set for v in val
    ):
        return True
    return False


def _is_union_constraint_dict(val) -> bool:
    """
    Check if *val* is a dict whose keys are all types, i.e. a per-member
    constraint mapping for a union field.

    Example: ``{OptionA: {"number": [2, 3]}, OptionB: {}}``
    """
    return isinstance(val, dict) and all(isinstance(k, type) for k in val)


def _enumerate_union_members(
    union_members: tuple,
    val=None,
) -> list:
    """
    Enumerate instances for a union-typed field.

    *val* controls which members are enumerated and how:

    - ``None`` - enumerate all members with their default domains.
    - A single type (e.g. ``OptionB``) - enumerate only that member.
    - A list of types (e.g. ``[OptionA, OptionB]``) - enumerate those members.
    - A dict mapping types to constraint dicts
      (e.g. ``{OptionA: {"number": [2, 3]}, OptionB: {}}``) -
      enumerate only the listed members, forwarding the constraint dicts.
    """
    # No override - enumerate all members with defaults
    if val is None:
        domain: list = []
        for member_type in union_members:
            domain.extend(_enumerate_instances(member_type))
        return domain

    # Single type
    if isinstance(val, type):
        return list(_enumerate_instances(val))

    # List of types
    if isinstance(val, list) and all(isinstance(v, type) for v in val):
        domain = []
        for member_type in val:
            domain.extend(_enumerate_instances(member_type))
        return domain

    # Dict of type → constraint dict
    if _is_union_constraint_dict(val):
        domain = []
        for member_type, member_kwargs in cast(dict, val).items():
            domain.extend(_enumerate_instances(member_type, **member_kwargs))
        return domain

    raise ValueError(
        f"Invalid value for union field: {val!r}. "
        "Expected a union member type, a list of types, or a dict mapping "
        "types to constraint dicts."
    )


def _enumerate_instances(cls: Type[T], **kwargs) -> Generator[T, None, None]:
    """
    Yield all instances of a dataclass given its class.

    The enumeration logic supports defining domains for fields using the
    ``domain`` metadata key.  Additionally, boolean fields are automatically
    enumerated with ``[True, False]``, Enum fields with all their members,
    and Literal types with their defined values.

    **Nested dataclass fields** can be constrained by passing a dict::

        _enumerate_instances(Outer, inner={"option": True})

    **Union-typed fields** support several override forms:

    - A single type to select one member::

          _enumerate_instances(Config, option=OptionB)

    - A list of types to select a subset::

          _enumerate_instances(Config, option=[OptionA, OptionB])

    - A dict mapping types to constraint dicts::

          _enumerate_instances(Config, option={OptionA: {"number": [2, 3]}, OptionB: {}})

    Args:
        cls (Type[T]): The dataclass type to enumerate.
        **kwargs: Fixed values or domains for fields.  If a value is a list
            and the corresponding field is kw_only, it is treated as a domain
            to enumerate over.  For nested dataclass fields a ``dict`` value
            is forwarded as keyword arguments.  For union-typed fields a type,
            list of types, or ``dict[type, dict]`` controls member selection
            and constraints.

    Returns:
        Generator[T, None, None]: A generator yielding instances of the
        dataclass.

    Raises:
        ValueError: If a field cannot be enumerated (no domain found).
    """

    names = []
    values = []
    fixed_kwargs = {}

    if (fields := getattr(cls, "__dataclass_fields__", None)) is None:
        # There are no fields defined for this class, so just yield a single
        # instance
        yield cls(**kwargs)
        return

    # Resolve type hints to handle stringified types from __future__.annotations
    type_hints = get_type_hints(cls)

    for field in fields.values():  # type: ignore
        name = field.name
        # Get resolved type or fallback to field.type
        current_type = type_hints.get(name, field.type)

        if name in kwargs:
            val = kwargs[name]

            is_union = _is_union_type(current_type)
            union_members = get_args(current_type) if is_union else ()

            # Union field with a type filter or constraint dict
            if is_union and (
                _is_type_filter(val, union_members) or _is_union_constraint_dict(val)
            ):
                names.append(name)
                values.append(_enumerate_union_members(union_members, val))
                continue

            # Nested dataclass field with a dict of constraints
            if (
                isinstance(val, dict)
                and not is_union
                and isinstance(current_type, type)
                and hasattr(current_type, "__dataclass_fields__")
            ):
                names.append(name)
                values.append(list(_enumerate_instances(current_type, **val)))
                continue

            # If kw_only and list, it's a domain to enumerate
            if field.kw_only and isinstance(val, list):
                names.append(name)
                values.append(val)
            else:
                # Otherwise, it's a fixed value
                fixed_kwargs[name] = val
            continue

        if not field.kw_only:
            # We don't enumerate non-kw-only fields that aren't in kwargs
            continue

        # Derived domain logic
        names.append(name)

        domain = field.metadata.get("domain", None)
        if domain is not None:
            values.append(domain)
            continue

        if current_type is bool:
            values.append([True, False])
            continue

        if isinstance(current_type, type) and issubclass(current_type, Enum):
            values.append(list(current_type))
            continue

        if get_origin(current_type) is Literal:
            values.append(list(get_args(current_type)))
            continue

        # Union types (e.g., OptionA | OptionB or Union[OptionA, OptionB])
        if _is_union_type(current_type):
            values.append(_enumerate_union_members(get_args(current_type), None))
            continue

        # Nested dataclass types
        if isinstance(current_type, type) and hasattr(
            current_type, "__dataclass_fields__"
        ):
            values.append(list(_enumerate_instances(current_type)))
            continue

        if field.default is not MISSING:
            values.append([field.default])
            continue

        raise ValueError(f"Cannot enumerate field {name}.")

    for instance_values in product(*values):
        yield cls(**fixed_kwargs, **dict(zip(names, instance_values)))
