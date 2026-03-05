import {
  App,
  applyDocumentTheme,
  applyHostFonts,
  applyHostStyleVariables,
  type McpUiHostContext,
} from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { useCallback, useEffect, useState } from "preact/hooks";
import { render } from "preact";

function extractTime(callToolResult: CallToolResult): string {
  const item = callToolResult.content?.find((c) => c.type === "text");
  return item ? (item as { text: string }).text : "";
}

function GetTimeApp() {
  const [app, setApp] = useState<App | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [toolResult, setToolResult] = useState<CallToolResult | null>(null);
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
    const instance = new App({ name: "Get Time App", version: "1.0.0" });

    instance.ontoolinput = async (input) => {
      console.info("Received tool call input:", input);
    };

    instance.ontoolresult = async (result) => {
      console.info("Received tool call result:", result);
      setToolResult(result);
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
      <div>
        <strong>ERROR:</strong> {error.message}
      </div>
    );
  if (!app) return <div>Connecting...</div>;

  return <GetTimeInner app={app} toolResult={toolResult} />;
}

function GetTimeInner({
  app,
  toolResult,
}: {
  app: App;
  toolResult: CallToolResult | null;
}) {
  const [serverTime, setServerTime] = useState("Loading...");

  useEffect(() => {
    if (toolResult) {
      setServerTime(extractTime(toolResult));
    }
  }, [toolResult]);

  const handleGetTime = useCallback(async () => {
    try {
      const result = await app.callServerTool({
        name: "get-time",
        arguments: {},
      });
      setServerTime(extractTime(result));
    } catch (e) {
      console.error(e);
      setServerTime("[ERROR]");
    }
  }, [app]);

  return (
    <main style={{ padding: "1rem" }}>
      <p>
        <strong>Server Time:</strong> <code>{serverTime}</code>
      </p>
      <button onClick={handleGetTime}>Refresh</button>
    </main>
  );
}

render(<GetTimeApp />, document.getElementById("root")!);
