---
name: model-context-protocol
description: 'Build, maintain, and debug MCP (Model Context Protocol) servers. Use when: creating an MCP server, adding tools/resources/prompts to an MCP server, implementing MCP transports (stdio, Streamable HTTP), debugging MCP server issues, configuring MCP server security, writing MCP tool definitions, setting up MCP server projects with Python or TypeScript SDKs, testing MCP servers with the Inspector.'
---

# Model Context Protocol (MCP) Server Development

## When to Use

- Building a new MCP server (Python or TypeScript)
- Adding tools, resources, or prompts to an existing MCP server
- Debugging MCP server communication or protocol issues
- Implementing or changing transport mechanisms (stdio, Streamable HTTP)
- Configuring MCP server security, authorization, or session management
- Testing MCP servers with the MCP Inspector
- Publishing or distributing MCP servers

## Key Concepts

MCP servers expose three core primitives to AI applications:

| Primitive     | Purpose                                    | Control    |
|---------------|--------------------------------------------|------------|
| **Tools**     | Functions the LLM can call to take actions | Model      |
| **Resources** | Read-only data sources for context         | Application|
| **Prompts**   | Reusable interaction templates             | User       |

Servers communicate via JSON-RPC 2.0 over two transport mechanisms:
- **stdio**: Local subprocess, newline-delimited messages on stdin/stdout. Never write non-MCP output to stdout.
- **Streamable HTTP**: Remote HTTP POST/GET with optional SSE streaming. Validate `Origin` header, bind to localhost when local.

## Procedure

### 1. Choose SDK and Set Up Project

**Python** (requires Python 3.10+, MCP SDK 1.2.0+):
```bash
uv init my-server && cd my-server
uv venv && source .venv/bin/activate
uv add "mcp[cli]" httpx
```

**TypeScript** (requires Node.js 16+):
```bash
mkdir my-server && cd my-server
npm init -y
npm install @modelcontextprotocol/sdk zod
```

### 2. Implement the Server

**Python — FastMCP pattern:**
```python
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("my-server")

@mcp.tool()
async def my_tool(param: str) -> str:
    """Description used by the LLM to decide when to call this tool.

    Args:
        param: What this parameter controls
    """
    return f"Result for {param}"

@mcp.resource("data://items/{item_id}")
async def get_item(item_id: str) -> str:
    """Retrieve an item by ID."""
    return f"Item: {item_id}"

if __name__ == "__main__":
    mcp.run(transport="stdio")
```

**TypeScript — McpServer pattern:**
```typescript
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const server = new McpServer({ name: "my-server", version: "1.0.0" });

server.tool("my_tool", { param: z.string() }, async ({ param }) => ({
  content: [{ type: "text", text: `Result for ${param}` }],
}));

const transport = new StdioServerTransport();
await server.connect(transport);
```

### 3. Define Tools Properly

- **name**: 1-128 chars, alphanumeric + `_`, `-`, `.` only. Case-sensitive.
- **description**: Clear explanation of what the tool does and when to use it.
- **inputSchema**: JSON Schema object defining parameters. Use `{ "type": "object", "additionalProperties": false }` for no-param tools.
- **outputSchema** (optional): JSON Schema for structured results. If provided, include both `structuredContent` and a `content` text fallback.
- Return errors via `isError: true` in the result for recoverable failures (LLM can retry). Use JSON-RPC errors for protocol-level issues.

### 4. Handle Transport Correctly

**stdio servers:**
- NEVER write to stdout except valid MCP JSON-RPC messages
- Use stderr or a logging library for debug output
- `print("debug", file=sys.stderr)` is safe; bare `print()` is not

**Streamable HTTP servers:**
- Validate `Origin` header on all requests; return 403 if invalid
- Bind to `127.0.0.1` (not `0.0.0.0`) for local servers
- Implement authentication for all connections
- Use `MCP-Session-Id` header for session management
- Session IDs must be cryptographically secure and non-deterministic

### 5. Test with MCP Inspector

```bash
npx @modelcontextprotocol/inspector uv run my_server.py
# or for TypeScript:
npx @modelcontextprotocol/inspector node my_server.js
```

### 6. Security Checklist

- [ ] Validate all tool inputs
- [ ] Implement access controls
- [ ] Rate limit tool invocations
- [ ] Sanitize tool outputs
- [ ] Never accept tokens not issued for your server (no token passthrough)
- [ ] Use secure, non-deterministic session IDs
- [ ] Bind local servers to localhost only
- [ ] Validate Origin headers for HTTP transport

## Reference

Full MCP specification, SDK docs, security guidance, and protocol details are in [llms-full.txt](./llms-full.txt). Key sections:

- **Build an MCP server** (line 1656): End-to-end tutorial for Python and TypeScript
- **Architecture overview** (line 5128): Protocol layers, participants, lifecycle
- **Understanding MCP servers** (line 5830): Tools, resources, prompts in depth
- **Tools specification** (line 16043): Tool schema, naming, error handling, structured output
- **Resources specification** (line 15628): URIs, templates, subscriptions
- **Prompts specification** (line 15339): Template definitions and arguments
- **Transports** (line 11091): stdio and Streamable HTTP details
- **Security Best Practices** (line 7367): SSRF, session hijacking, confused deputy mitigations
- **SDKs** (line 6114): Available SDKs (TypeScript, Python, C#, Go, Java, Rust, etc.)
- **MCP Inspector** (line 6164): Testing and debugging tool
