# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Validate the qdk-mcp server over stdio JSON-RPC."""

import json
import subprocess
import sys
import threading


def main():
    server = subprocess.Popen(
        [sys.executable, "-m", "qdk_mcp.server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    # Forward stderr in a background thread
    def forward_stderr():
        for line in server.stderr:
            sys.stderr.buffer.write(line)
            sys.stderr.buffer.flush()

    threading.Thread(target=forward_stderr, daemon=True).start()

    msg_id = 0

    def send(method, params):
        nonlocal msg_id
        msg_id += 1
        msg = json.dumps(
            {"jsonrpc": "2.0", "id": msg_id, "method": method, "params": params}
        )
        server.stdin.write((msg + "\n").encode())
        server.stdin.flush()
        return msg_id

    def recv():
        line = server.stdout.readline().decode().strip()
        if not line:
            raise RuntimeError("Server closed stdout unexpectedly")
        return json.loads(line)

    try:
        # 1. Initialize
        send(
            "initialize",
            {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"},
            },
        )
        data = recv()
        assert "result" in data, f"Expected result in init response: {data}"
        info = data["result"]["serverInfo"]
        print("=== INIT ===")
        print(f"Server: {info['name']}")
        print(f"Capabilities: {json.dumps(data['result']['capabilities'])}")
        print()

        # 2. List tools
        send("tools/list", {})
        data = recv()
        tools = data["result"]["tools"]
        print("=== TOOLS ===")
        tool_names = set()
        for t in tools:
            tool_names.add(t["name"])
            print(f"- {t['name']}: {t.get('description', '')[:80]}")
            meta_ui = (t.get("_meta") or {}).get("ui")
            if meta_ui:
                print(f"  UI: {json.dumps(meta_ui)}")
        print()

        assert "eval" in tool_names, "Missing 'eval' tool"
        assert "circuit" in tool_names, "Missing 'circuit' tool"

        # 3. List resources
        send("resources/list", {})
        data = recv()
        resources = data["result"]["resources"]
        print("=== RESOURCES ===")
        for r in resources:
            print(f"- {r['uri']} ({r.get('mimeType', 'N/A')})")
        print()

        assert any(
            r["uri"] == "ui://qdk/circuit.html" for r in resources
        ), "Missing circuit.html resource"

        # 4. Call eval tool
        send(
            "tools/call",
            {
                "name": "eval",
                "arguments": {
                    "source": "1 + 1",
                },
            },
        )
        data = recv()
        print("=== EVAL RESULT ===")
        assert "result" in data, f"Expected result: {data}"
        eval_text = data["result"]["content"][0]["text"]
        eval_parsed = json.loads(eval_text)
        print(f"Result: {eval_parsed['result']}")
        assert eval_parsed["result"] == 2, f"Expected 2, got {eval_parsed['result']}"
        print()

        # 5. Call circuit tool
        send(
            "tools/call",
            {
                "name": "circuit",
                "arguments": {
                    "entry_expr": "{ use q = Qubit(); H(q); let r = M(q); Reset(q); r }",
                },
            },
        )
        data = recv()
        print("=== CIRCUIT RESULT ===")
        assert "result" in data, f"Expected result: {data}"
        result = data["result"]
        assert not result.get("isError"), f"Circuit call returned error: {result}"
        print(f"Text: {result['content'][0]['text'][:200]}...")
        sc = result.get("structuredContent")
        assert sc is not None, "Expected structuredContent in circuit result"
        assert "circuits" in sc, "Expected 'circuits' key in structuredContent"
        assert "version" in sc, "Expected 'version' key in structuredContent"
        assert len(sc["circuits"]) >= 1, "Expected at least 1 circuit"
        circ = sc["circuits"][0]
        qubits = circ.get("qubits", [])
        grid = circ.get("componentGrid", [])
        print(f"Qubits: {len(qubits)}")
        print(f"Gate columns: {len(grid)}")
        assert len(qubits) >= 1, f"Expected at least 1 qubit, got {len(qubits)}"
        assert len(grid) >= 1, f"Expected at least 1 gate column, got {len(grid)}"
        print()

        # 6. Call circuit tool with invalid generation_method
        send(
            "tools/call",
            {
                "name": "circuit",
                "arguments": {
                    "entry_expr": "{ use q = Qubit(); H(q); }",
                    "generation_method": "InvalidMethod",
                },
            },
        )
        data = recv()
        print("=== CIRCUIT ERROR TEST ===")
        result = data.get("result", {})
        assert result.get(
            "isError"
        ), "Expected isError=True for invalid generation_method"
        error_text = result["content"][0]["text"]
        print(f"Error (expected): {error_text}")
        assert "InvalidMethod" in error_text
        print()

        print("=== ALL TESTS PASSED ===")

    finally:
        server.kill()
        server.wait()


if __name__ == "__main__":
    main()
