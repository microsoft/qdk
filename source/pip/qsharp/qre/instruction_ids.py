# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportAttributeAccessIssue=false


from .._native import instruction_ids

for name in instruction_ids.__all__:
    globals()[name] = getattr(instruction_ids, name)
