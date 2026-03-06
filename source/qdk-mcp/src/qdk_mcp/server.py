# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import asyncio
import json
import pathlib
from concurrent.futures import ThreadPoolExecutor
from typing import Optional

from fastmcp import FastMCP
from fastmcp.exceptions import ToolError
from fastmcp.tools.tool import ToolResult
from mcp.types import TextContent

import qsharp
from qsharp._native import CircuitGenerationMethod, QSharpError

# The Q# interpreter is bound to the thread that created it (Rust !Send).
# Use a single-thread executor so all qsharp calls run on the same thread.
_qsharp_thread = ThreadPoolExecutor(max_workers=1)

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


def _eval_sync(source: str) -> str:
    try:
        result = qsharp.eval(source, save_events=True)
        return json.dumps(_serialize_shot_result(result))
    except QSharpError as e:
        return json.dumps({"error": str(e)})


@mcp.tool()
async def eval(source: str) -> str:
    """Evaluate Q# source code and return structured JSON results.

    Args:
        source: Q# source code to evaluate.
    """
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(_qsharp_thread, _eval_sync, source)


_GENERATION_METHODS = {
    "ClassicalEval": CircuitGenerationMethod.ClassicalEval,
    "Simulate": CircuitGenerationMethod.Simulate,
}

# Add Static if available in this build of qsharp
if hasattr(CircuitGenerationMethod, "Static"):
    _GENERATION_METHODS["Static"] = CircuitGenerationMethod.Static


def _circuit_sync(
    entry_expr: str,
    operation: Optional[str],
    gen_method,
    max_operations: Optional[int],
    source_locations: bool,
    group_by_scope: bool,
    prune_classical_qubits: bool,
) -> ToolResult:
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
    # Wrap in the CircuitGroup format expected by the circuit renderer
    circuit_group = {"circuits": [circuit_json], "version": 1}
    return ToolResult(
        content=[TextContent(type="text", text=str(result))],
        structured_content=circuit_group,
    )


@mcp.tool(
    meta={
        "ui": {"resourceUri": CIRCUIT_RESOURCE_URI},
        "ui/resourceUri": CIRCUIT_RESOURCE_URI,
    }
)
async def circuit(
    entry_expr: str,
    operation: Optional[str] = None,
    generation_method: Optional[str] = None,
    max_operations: Optional[int] = None,
    source_locations: bool = False,
    group_by_scope: bool = True,
    prune_classical_qubits: bool = False,
) -> ToolResult:
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
    gen_method = None
    if generation_method is not None:
        gen_method = _GENERATION_METHODS.get(generation_method)
        if gen_method is None:
            raise ToolError(
                f"Invalid generation_method: {generation_method!r}. Must be one of: {', '.join(repr(k) for k in _GENERATION_METHODS)}."
            )

    loop = asyncio.get_event_loop()
    try:
        return await loop.run_in_executor(
            _qsharp_thread,
            _circuit_sync,
            entry_expr,
            operation,
            gen_method,
            max_operations,
            source_locations,
            group_by_scope,
            prune_classical_qubits,
        )
    except QSharpError as e:
        raise ToolError(str(e)) from e


def main():
    mcp.run()


if __name__ == "__main__":
    main()
