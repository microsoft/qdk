// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { EventType, sendTelemetryEvent, UserFlowStatus } from "../telemetry";
import { getRandomGuid } from "../utils";
import * as azqTools from "./azureQuantumTools";
import { LearningTools } from "./learningTools";
import { QSharpTools } from "./qsharpTools";
import { CopilotToolError } from "./types";
import { ToolState } from "./azureQuantumTools";
import type { LearningService } from "../learningService/index";

// state
const workspaceState: ToolState = {};
let qsharpTools: QSharpTools | undefined;
let learningTools: LearningTools | undefined;

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
    name: "qsharp-get-library-descriptions",
    tool: async () => await qsharpTools!.qsharpGetLibraryDescriptions(),
  },
  // ─── QDK Learning tools ───
  {
    name: "qdk-learning-init",
    tool: async (input) => await learningTools!.init(input),
    confirm: (input: {
      workspacePath?: string;
    }): vscode.PreparedToolInvocation => ({
      confirmationMessages: {
        title: "Initialize QDK Learning workspace",
        message: input.workspacePath
          ? `Initialize a Quantum Katas learning workspace at ${input.workspacePath}? Exercise files and progress will be saved there.`
          : `Initialize a Quantum Katas learning workspace in the current folder? Exercise files and progress will be saved there.`,
      },
    }),
  },
  {
    name: "qdk-learning-show-panel",
    tool: async () => await learningTools!.showPanel(),
  },
  {
    name: "qdk-learning-get-state",
    tool: async () => learningTools!.getState(),
  },
  {
    name: "qdk-learning-get-progress",
    tool: async () => learningTools!.getProgress(),
  },
  {
    name: "qdk-learning-list-katas",
    tool: async () => learningTools!.listKatas(),
  },
  {
    name: "qdk-learning-next",
    tool: async () => learningTools!.next(),
  },
  {
    name: "qdk-learning-previous",
    tool: async () => learningTools!.previous(),
  },
  {
    name: "qdk-learning-goto",
    tool: async (input) => learningTools!.goTo(input),
  },
  {
    name: "qdk-learning-run",
    tool: async (input) => await learningTools!.run(input),
  },
  {
    name: "qdk-learning-run-with-noise",
    tool: async (input) => await learningTools!.runWithNoise(input),
  },
  {
    name: "qdk-learning-circuit",
    tool: async () => await learningTools!.circuit(),
  },
  {
    name: "qdk-learning-estimate",
    tool: async () => await learningTools!.estimate(),
  },
  {
    name: "qdk-learning-check",
    tool: async () => await learningTools!.check(),
  },
  {
    name: "qdk-learning-hint",
    tool: async () => learningTools!.hint(),
  },
  {
    name: "qdk-learning-reveal-answer",
    tool: async () => learningTools!.revealAnswer(),
  },
  {
    name: "qdk-learning-solution",
    tool: async () => learningTools!.solution(),
  },
];

export function registerLanguageModelTools(
  context: vscode.ExtensionContext,
  learningService: LearningService,
) {
  qsharpTools = new QSharpTools(context.extensionUri);
  learningTools = new LearningTools(learningService, qsharpTools);
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
