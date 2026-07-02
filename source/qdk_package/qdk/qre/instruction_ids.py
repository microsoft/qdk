# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# pyright: reportAttributeAccessIssue=false


from .._native import instruction_ids

INSTRUCTION_ID_MAP: dict[str, int] = {
    attr: getattr(instruction_ids, attr)
    for attr in dir(instruction_ids)
    if not attr.startswith("_") and isinstance(getattr(instruction_ids, attr), int)
}

globals().update(INSTRUCTION_ID_MAP)
