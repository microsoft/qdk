# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# flake8: noqa F403
# pyright: ignore[reportWildcardImportFromLibrary]

"""QRE property key constants.

This module re-exports all public symbols from [qsharp.qre.property_keys](:mod:`qsharp.qre.property_keys`),
making them available under the ``qdk.qre.property_keys`` namespace. It also
provides helpers for defining custom property keys that don't conflict with
built-in ones.

Requires the ``qre`` extra: ``pip install qdk[qre]``.

Example:

    from qdk.qre.property_keys import *
"""

try:
    # Re-export the top-level qsharp.qre.property_keys names.
    from qsharp.qre.property_keys import *
except Exception as ex:
    raise ImportError(
        "qdk.qre.property_keys requires the qre extras. Install with 'pip install \"qdk[qre]\"'."
    ) from ex

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
