# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
from mcp.server.fastmcp import FastMCP

import qsharp
from qsharp._native import QSharpError

mcp = FastMCP("qdk")


def _serialize_shot_result(shot_result: dict) -> dict:
    """Serialize a ShotResult into a JSON-friendly dictionary."""
    return {
        "result": _serialize_value(shot_result["result"]),
        "messages": shot_result["messages"],
        "dumps": [_serialize_state_dump(d) for d in shot_result["dumps"]],
        "matrices": [str(m) for m in shot_result["matrices"]],
        "events": [_serialize_event(e) for e in shot_result["events"]],
    }


def _serialize_value(value):
    """Serialize a Q# value to a JSON-compatible Python object."""
    if isinstance(value, complex):
        return {"real": value.real, "imag": value.imag}
    if isinstance(value, tuple):
        return [_serialize_value(v) for v in value]
    if isinstance(value, list):
        return [_serialize_value(v) for v in value]
    if isinstance(value, dict):
        return {k: _serialize_value(v) for k, v in value.items()}
    # int, float, str, bool, None are already JSON-friendly
    return value


def _serialize_state_dump(dump) -> dict:
    """Serialize a StateDump as sparse {state_index: {real, imag}} + qubit_count."""
    amplitudes = {}
    for index in dump:
        c = dump[index]
        amplitudes[str(index)] = {"real": c.real, "imag": c.imag}
    return {
        "qubit_count": dump.qubit_count,
        "amplitudes": amplitudes,
    }


def _serialize_event(event):
    """Serialize a single event from the events list."""
    if isinstance(event, str):
        return {"type": "message", "message": event}
    # StateDump (from qsharp._qsharp)
    if hasattr(event, "qubit_count") and hasattr(event, "__iter__"):
        return {"type": "state_dump", **_serialize_state_dump(event)}
    # Output object (matrix)
    return {"type": "matrix", "matrix": str(event)}


@mcp.tool()
def eval(source: str) -> str:
    """Evaluate Q# source code and return structured JSON results.

    Args:
        source: Q# source code to evaluate.
    """
    try:
        result = qsharp.eval(source, save_events=True)
        return json.dumps(_serialize_shot_result(result))
    except QSharpError as e:
        return json.dumps({"error": str(e)})


def main():
    mcp.run()


if __name__ == "__main__":
    main()
