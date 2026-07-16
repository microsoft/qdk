# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportAttributeAccessIssue=false


from .._native import property_keys

for name in property_keys.__all__:
    globals()[name] = getattr(property_keys, name)

# Some starting index for custom properties, to avoid conflicts with the
# built-in ones. We do not expect to have more than 1 million built-in
# properties anytime soon.
CUSTOM_PROPERTY: int = 1_000_000


def custom_property(index: int) -> int:
    """Returns a custom property key for the given index."""
    if index < 0:
        raise ValueError("Custom property index must be non-negative.")
    return CUSTOM_PROPERTY + index


def custom_properties(count: int) -> list[int]:
    """Returns a list of custom property keys for the given count."""
    if count < 0:
        raise ValueError("Custom property count must be non-negative.")
    return [custom_property(i) for i in range(count)]
