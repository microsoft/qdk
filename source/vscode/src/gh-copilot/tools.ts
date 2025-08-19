// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { CircuitData, log } from "qsharp-lang";
import * as vscode from "vscode";
import { EventType, sendTelemetryEvent, UserFlowStatus } from "../telemetry";
import { getRandomGuid } from "../utils";
import * as azqTools from "./azureQuantumTools";
import { updateCopilotInstructions } from "./instructions";
import { QSharpTools } from "./qsharpTools";
import { CopilotToolError } from "./types";
import { ToolState } from "./azureQuantumTools";
import { generateWebviewHtml } from "../circuitEditor";
import { _getWebviewContent } from "../webviewPanel";

// state
const workspaceState: ToolState = {};
let qsharpTools: QSharpTools | undefined;

const toolDefinitions: {
  name: string;
  tool: (input: any) => Promise<any>;
  confirm?: (input: any) => vscode.PreparedToolInvocation;
}[] = [
  // match these to the "languageModelTools" entries in package.json
  {
    name: "azure-quantum-get-jobs",
    tool: async (input) =>
      (await azqTools.getJobs(workspaceState, input)).result,
  },
  {
    name: "azure-quantum-get-job",
    tool: async (input: { job_id: string }) =>
      (await azqTools.getJob(workspaceState, input)).result,
  },
  {
    name: "azure-quantum-connect-to-workspace",
    tool: async () =>
      (await azqTools.connectToWorkspace(workspaceState)).result,
  },
  {
    name: "azure-quantum-download-job-results",
    tool: async (input: { job_id: string }) =>
      (await azqTools.downloadJobResults(workspaceState, input)).result,
  },
  {
    name: "azure-quantum-get-workspaces",
    tool: async () => (await azqTools.getWorkspaces()).result,
  },
  {
    name: "azure-quantum-submit-to-target",
    tool: async (input: {
      filePath: string;
      jobName: string;
      targetId: string;
      shots: number;
    }) =>
      (await azqTools.submitToTarget(workspaceState, qsharpTools!, input))
        .result,
    confirm: (input: {
      jobName: string;
      targetId: string;
      shots: number;
    }): vscode.PreparedToolInvocation => ({
      confirmationMessages: {
        title: "Submit Azure Quantum job",
        message: `Submit job "${input.jobName}" to ${input.targetId} for ${input.shots} shots?`,
      },
    }),
  },
  {
    name: "azure-quantum-get-active-workspace",
    tool: async () =>
      (await azqTools.getActiveWorkspace(workspaceState)).result,
  },
  {
    name: "azure-quantum-set-active-workspace",
    tool: async (input: { workspace_id: string }) =>
      (await azqTools.setActiveWorkspace(workspaceState, input)).result,
  },
  {
    name: "azure-quantum-get-providers",
    tool: async () => (await azqTools.getProviders(workspaceState)).result,
  },
  {
    name: "azure-quantum-get-target",
    tool: async (input: { target_id: string }) =>
      (await azqTools.getTarget(workspaceState, input)).result,
  },
  {
    name: "qdk-run-program",
    tool: async (input) => await qsharpTools!.runProgram(input),
  },
  {
    name: "qdk-generate-circuit",
    tool: async (input) => await qsharpTools!.generateCircuit(input),
  },
  {
    name: "qdk-run-resource-estimator",
    tool: async (input) => await qsharpTools!.runResourceEstimator(input),
  },
  {
    name: "qdk-test",
    tool: async (): Promise<vscode.LanguageModelToolResult> => {
      const result = new vscode.LanguageModelToolResult([
        new vscode.LanguageModelTextPart("hi"),
      ]);
      (result as vscode.ExtendedLanguageModelToolResult2).toolResultDetails2 = {
        mime: "application/x.qsharp-config",
        value: new TextEncoder().encode('{ "hi" : "yes" }'),
      };
      return result;
    },
  },
];

export function registerLanguageModelTools(context: vscode.ExtensionContext) {
  vscode.chat.registerChatOutputRenderer("qdk-config-renderer", {
    async renderChatOutput({ value }, webview) {
      const content = _getWebviewContent(webview);
      // const source = new TextDecoder().decode(value);

      // let nonce = "";
      // const possible =
      //   "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
      // for (let i = 0; i < 64; i++) {
      //   nonce += possible.charAt(Math.floor(Math.random() * possible.length));
      // }

      log.info(content);
      webview.html = content;

      const title = "hi title";
      const target = `hi target`;
      const circuit: CircuitData = {
        version: 1,
        circuits: [
          {
            qubits: [],
            componentGrid: [
              // {
              // components: [{
              //   gate: "X",
              //   kind: "unitary",
              //   targets: ""
              // }
            ],
          },
        ],
      };

      const props = {
        title,
        targetProfile: target,
        simulated: false,
        calculating: false,
        circuit,
        errorHtml: undefined,
      };

      const message = {
        command: "circuit",
        props,
      };

      setTimeout(() => webview.postMessage(message), 10);

      // webview.html = `
      // 		<!DOCTYPE html>
      // 		<html lang="en">

      // 		<head>
      // 			<meta charset="UTF-8">
      // 			<meta name="viewport" content="width=device-width, initial-scale=1.0">
      // 			<title>omg</title>
      // 			<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src ${webview.cspSource} 'nonce-${nonce}'; style-src 'self' 'unsafe-inline';" />
      // 		</head>

      // 		<body>
      // 			<pre>
      // 				${source}
      // 			</pre>
      // 		</body>
      // 		</html>`;
    },
  });

  qsharpTools = new QSharpTools(context.extensionUri);
  for (const { name, tool: fn, confirm: confirmFn } of toolDefinitions) {
    context.subscriptions.push(
      vscode.lm.registerTool(name, tool(context, name, fn, confirmFn)),
    );
  }
}

function tool<T>(
  context: vscode.ExtensionContext,
  toolName: string,
  toolFn: (input: T) => Promise<any>,
  confirmFn?: (input: T) => vscode.PreparedToolInvocation,
): vscode.LanguageModelTool<any> {
  return {
    invoke: (options: vscode.LanguageModelToolInvocationOptions<T>) =>
      invokeTool(context, toolName, options, toolFn),
    prepareInvocation:
      confirmFn &&
      ((options: vscode.LanguageModelToolInvocationPrepareOptions<T>) =>
        confirmFn(options.input)),
  };
}

async function invokeTool<T>(
  context: vscode.ExtensionContext,
  toolName: string,
  options: vscode.LanguageModelToolInvocationOptions<T>,
  toolFn: (input: T) => Promise<any>,
): Promise<vscode.LanguageModelToolResult> {
  updateCopilotInstructions("ChatToolCall", context);

  const associationId = getRandomGuid();
  sendTelemetryEvent(EventType.LanguageModelToolStart, {
    associationId,
    toolName,
  });

  log.debug(
    `Invoking tool: ${toolName}, tokenBudget: ${options.tokenizationOptions?.tokenBudget}`,
  );

  let resultText: string;
  try {
    const result = await toolFn(options.input);

    sendTelemetryEvent(EventType.LanguageModelToolEnd, {
      associationId,
      flowStatus: UserFlowStatus.Succeeded,
    });

    if (result instanceof vscode.LanguageModelToolResult) {
      log.debug("returning tool result directly");
      return result;
    }

    resultText = JSON.stringify(result);
  } catch (e) {
    sendTelemetryEvent(EventType.LanguageModelToolEnd, {
      associationId,
      flowStatus: UserFlowStatus.Failed,
      reason: e instanceof Error ? e.name : typeof e, // avoid sending error content in telemetry
    });

    if (e instanceof CopilotToolError) {
      resultText = "Tool error:\n" + e.message;
    } else {
      // We'll avoid adding arbitrary error details to the conversation history
      // since they can get large and use up a lot of tokens with essentially noise.
      //
      // If you need to include the error details for a specific error, catch
      // it and rethrow it as a CopilotToolError the relevant context.
      resultText = "An error occurred.";
    }
  }

  const tokens = await options.tokenizationOptions?.countTokens(resultText);
  log.debug(`Tool result: ${toolName}, tokens: ${tokens}`);

  return {
    content: [new vscode.LanguageModelTextPart(resultText)],
  };
}
