# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import json
import pathlib
from typing import Optional

from mcp.server.fastmcp import FastMCP
from mcp.types import CallToolResult, TextContent

import qsharp
from qsharp._native import CircuitGenerationMethod, QSharpError

mcp = FastMCP("qdk")

CIRCUIT_RESOURCE_URI = "ui://qdk/circuit.html"

_STATIC_DIR = pathlib.Path(__file__).parent / "static"


@mcp.resource(
    CIRCUIT_RESOURCE_URI,
    mime_type="text/html;profile=mcp-app",
    meta={"ui": {}},
)
def circuit_resource() -> str:
    """Serve the circuit visualization MCP App HTML."""
    return (_STATIC_DIR / "index.html").read_text(encoding="utf-8")


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


_GENERATION_METHODS = {
    "ClassicalEval": CircuitGenerationMethod.ClassicalEval,
    "Simulate": CircuitGenerationMethod.Simulate,
}


@mcp.tool(
    meta={
        "ui": {"resourceUri": CIRCUIT_RESOURCE_URI},
        "ui/resourceUri": CIRCUIT_RESOURCE_URI,
    }
)
def circuit(
    entry_expr: str,
    operation: Optional[str] = None,
    generation_method: Optional[str] = None,
    max_operations: Optional[int] = None,
    source_locations: bool = False,
    group_by_scope: bool = True,
    prune_classical_qubits: bool = False,
) -> CallToolResult:
    """Synthesize a circuit diagram for a Q# program.

    Args:
        entry_expr: Q# expression to synthesize a circuit for.
        operation: Operation to synthesize (name or lambda). Must take only qubits or qubit arrays.
        generation_method: Circuit generation method: "ClassicalEval" or "Simulate".
        max_operations: Maximum number of operations in the circuit.
        source_locations: Include source locations in the circuit.
        group_by_scope: Group operations by scope.
        prune_classical_qubits: Prune classical qubits from the circuit.
    """
    try:
        gen_method = None
        if generation_method is not None:
            gen_method = _GENERATION_METHODS.get(generation_method)
            if gen_method is None:
                return CallToolResult(
                    content=[
                        TextContent(
                            type="text",
                            text=f"Invalid generation_method: {generation_method!r}. Must be 'ClassicalEval' or 'Simulate'.",
                        )
                    ],
                    isError=True,
                )

        result = qsharp.circuit(
            entry_expr,
            operation=operation,
            generation_method=gen_method,
            max_operations=max_operations,
            source_locations=source_locations,
            group_by_scope=group_by_scope,
            prune_classical_qubits=prune_classical_qubits,
        )

        circuit_json = json.loads(result.json())

        return CallToolResult(
            content=[TextContent(type="text", text=str(result))],
            structuredContent=circuit_json,
        )
    except QSharpError as e:
        return CallToolResult(
            content=[TextContent(type="text", text=str(e))],
            isError=True,
        )


def main():
    mcp.run()


if __name__ == "__main__":
    main()
