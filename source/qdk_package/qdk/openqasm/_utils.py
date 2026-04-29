# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from .. import Result


def _map_qsharp_result_to_bit(v) -> str:
    if isinstance(v, Result):
        if v == Result.One:
            return "1"
        else:
            return "0"
    return str(v)


def _convert_result_arrays_to_bitstrings(obj):
    if isinstance(obj, tuple):
        return tuple([_convert_result_arrays_to_bitstrings(term) for term in obj])
    elif isinstance(obj, list):
        # if all elements are Q# results, convert to bitstring
        if all([isinstance(bit, Result) for bit in obj]):
            return "".join([_map_qsharp_result_to_bit(bit) for bit in obj])
        return [_convert_result_arrays_to_bitstrings(bit) for bit in obj]
    elif isinstance(obj, Result):
        if obj == Result.One:
            return 1
        else:
            return 0
    else:
        return obj


def as_bitstring(obj):
    """
    Convert Q# results to bitstrings.

    :param obj: The object to convert.
    :return: The converted object.
    """
    return _convert_result_arrays_to_bitstrings(obj)
