import {
  App,
  applyDocumentTheme,
  applyHostFonts,
  applyHostStyleVariables,
  type McpUiHostContext,
} from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { useEffect, useRef, useState } from "preact/hooks";
import { render } from "preact";
import { draw } from "qsharp-lang/circuit-vis";
import { toCircuitGroup } from "qsharp-lang/circuit-group";
import type { CircuitGroup } from "qsharp-lang/circuit-vis";
import "qsharp-lang/qsharp-ux.css";
import "qsharp-lang/qsharp-circuit.css";

function CircuitApp() {
  const [app, setApp] = useState<App | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [circuitData, setCircuitData] = useState<CircuitGroup | null>(null);
  const [hostContext, setHostContext] = useState<
    McpUiHostContext | undefined
  >();

  useEffect(() => {
    if (hostContext?.theme) {
      applyDocumentTheme(hostContext.theme);
    }
    if (hostContext?.styles?.variables) {
      applyHostStyleVariables(hostContext.styles.variables);
    }
    if (hostContext?.styles?.css?.fonts) {
      applyHostFonts(hostContext.styles.css.fonts);
    }
  }, [hostContext]);

  useEffect(() => {
    const instance = new App({ name: "Q# Circuit App", version: "1.0.0" });

    instance.ontoolinput = async (input) => {
      console.info("Received tool call input:", input);
    };

    instance.ontoolresult = async (result: CallToolResult) => {
      console.info("Received tool call result:", result);
      if (result.structuredContent) {
        const parsed = toCircuitGroup(result.structuredContent);
        if (parsed.ok) {
          setCircuitData(parsed.circuitGroup);
        } else {
          setError(new Error(parsed.error));
        }
      }
    };

    instance.ontoolcancelled = (params) => {
      console.info("Tool call cancelled:", params.reason);
    };

    instance.onerror = console.error;

    instance.onhostcontextchanged = (params) => {
      setHostContext((prev) => ({ ...prev, ...params }));
    };

    instance
      .connect()
      .then(() => {
        setApp(instance);
        setHostContext(instance.getHostContext());
      })
      .catch(setError);
  }, []);

  if (error)
    return (
      <div style={{ padding: "1rem" }}>
        <strong>Error:</strong> <pre>{error.message}</pre>
      </div>
    );
  if (!app) return <div style={{ padding: "1rem" }}>Connecting...</div>;
  if (!circuitData)
    return <div style={{ padding: "1rem" }}>Waiting for circuit data...</div>;

  return <CircuitRenderer circuitGroup={circuitData} />;
}

function CircuitRenderer({ circuitGroup }: { circuitGroup: CircuitGroup }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    container.innerHTML = "";
    draw(circuitGroup, container);
  }, [circuitGroup]);

  return (
    <div style={{ padding: "0.5rem" }}>
      <div class="qs-circuit" ref={containerRef}></div>
    </div>
  );
}

render(<CircuitApp />, document.getElementById("root")!);
