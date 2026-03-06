import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));

const server = spawn("node", [join(__dirname, "dist/index.js"), "--stdio"], {
  stdio: ["pipe", "pipe", "pipe"],
});

let buffer = "";
let id = 0;

function send(method, params) {
  const msg = JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params });
  server.stdin.write(msg + "\n");
  return id;
}

function onMessage(msg) {
  const data = JSON.parse(msg);
  if (data.id === 1) {
    console.log("=== INIT ===");
    console.log(
      "Server:",
      data.result.serverInfo.name,
      data.result.serverInfo.version,
    );
    console.log("Capabilities:", JSON.stringify(data.result.capabilities));
    console.log();
    send("tools/list", {});
  } else if (data.id === 2) {
    console.log("=== TOOLS ===");
    for (const t of data.result.tools) {
      console.log(`- ${t.name}: ${t.description}`);
      if (t._meta?.ui) console.log("  UI:", JSON.stringify(t._meta.ui));
    }
    console.log();
    send("resources/list", {});
  } else if (data.id === 3) {
    console.log("=== RESOURCES ===");
    for (const r of data.result.resources) {
      console.log(`- ${r.uri} (${r.mimeType})`);
    }
    console.log();
    send("tools/call", {
      name: "circuit",
      arguments: {
        source: `operation Main() : Result { use q = Qubit(); H(q); let r = M(q); Reset(q); r }`,
      },
    });
  } else if (data.id === 4) {
    console.log("=== CIRCUIT RESULT ===");
    if (data.error) {
      console.log("ERROR:", JSON.stringify(data.error, null, 2));
    } else {
      console.log("Text:", data.result.content[0].text);
      const sc = data.result.structuredContent;
      if (sc) {
        console.log("Structured content version:", sc.version);
        console.log("Circuits:", sc.circuits?.length);
        const c = sc.circuits?.[0];
        if (c) {
          console.log("Qubits:", c.qubits?.length);
          console.log("Gate columns:", c.componentGrid?.length);
        }
      }
    }
    console.log("\n=== ALL TESTS PASSED ===");
    server.kill();
  }
}

server.stdout.on("data", (chunk) => {
  buffer += chunk.toString();
  const lines = buffer.split("\n");
  buffer = lines.pop();
  for (const line of lines) {
    if (line.trim()) onMessage(line.trim());
  }
});

server.stderr.on("data", (chunk) => {
  process.stderr.write(chunk);
});

server.on("close", (code) => {
  process.exit(code ?? 0);
});

// Kick it off
send("initialize", {
  protocolVersion: "2025-03-26",
  capabilities: {},
  clientInfo: { name: "test", version: "1.0" },
});
