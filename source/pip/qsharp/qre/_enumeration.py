# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Generator, Type, TypeVar, Literal, get_args, get_origin
from dataclasses import MISSING
from itertools import product
from enum import Enum


T = TypeVar("T")


def _enumerate_instances(cls: Type[T], **kwargs) -> Generator[T, None, None]:
    """
    Yields all instances of a dataclass given its class.

    The enumeration logic supports defining domains for fields using the `domain`
    metadata key. This allows fields to specify their valid range of values for
    enumeration directly in the definition. Additionally, boolean fields are
    automatically enumerated with `[True, False]`. Enum fields are enumerated
    with all their members, and Literal types with their defined values.

    Args:
        cls (Type[T]): The dataclass type to enumerate.
        **kwargs: Fixed values or domains for fields. If a value is a list
            and the corresponding field is kw_only, it is treated as a domain
            to enumerate over.

    Returns:
        Generator[T, None, None]: A generator yielding instances of the dataclass.

    Raises:
        ValueError: If a field cannot be enumerated (no domain found).

    Example:

    .. code-block:: python
        from dataclasses import dataclass, field, KW_ONLY
        @dataclass
        class MyConfig:
            # Not part of enumeration
            name: str
            _ : KW_ONLY
            # Part of enumeration with implicit domain [True, False]
            enable_logging: bool = field(kw_only=True)
            # Explicit domain in metadata
            retry_count: int = field(metadata={"domain": [1, 3, 5]}, kw_only=True)
    """

    names = []
    values = []
    fixed_kwargs = {}

    if not hasattr(cls, "__dataclass_fields__"):
        # There are no fields defined for this class, so just yield a single
        # instance
        yield cls(**kwargs)
        return

    for field in cls.__dataclass_fields__.values():
        name = field.name

        if name in kwargs:
            val = kwargs[name]
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

        if field.type is bool:
            values.append([True, False])
            continue

        if isinstance(field.type, type) and issubclass(field.type, Enum):
            values.append(list(field.type))
            continue

        if get_origin(field.type) is Literal:
            values.append(list(get_args(field.type)))
            continue

        if field.default is not MISSING:
            values.append([field.default])
            continue

        raise ValueError(f"Cannot enumerate field {name}.")

    for instance_values in product(*values):
        yield cls(**fixed_kwargs, **dict(zip(names, instance_values)))
