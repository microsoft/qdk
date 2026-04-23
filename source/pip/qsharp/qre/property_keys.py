# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportAttributeAccessIssue=false


from .._native import property_keys

for name in property_keys.__all__:
    globals()[name] = getattr(property_keys, name)
