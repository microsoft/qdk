# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
from typing import Any, Dict, List, Union


def extract_qubit_metric(
    qubit: Dict[str, Any], keys: Union[str, List[str]], combine=sum
):
    """
    Extracts a metric from a dictionary and combine multiple metrics if a list
    of keys is provided.

    :param qubit: Dictionary containing qubit metrics
    :param keys: Key name or list of key names to extract from the dictionary
    :param combine: Function to combine multiple metrics (default: sum)
    """

    try:
        if isinstance(keys, str):
            return qubit[keys]
        else:
            return combine(qubit[key] for key in keys)
    except KeyError as e:
        raise KeyError(f"Missing qubit property: {e}") from e
