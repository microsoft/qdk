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
import type { LearningService } from "../learning/index";

// state
const workspaceState: ToolState = {};
let qsharpTools: QSharpTools | undefined;
let learningTools: LearningTools | undefined;

const toolDefinitions: {
  name: string;
  tool: (input: any) => Promise<any>;
  confirm?: (
    input: any,
  ) => vscode.ProviderResult<vscode.PreparedToolInvocation>;
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
    name: "qdk-learning-show",
    tool: async () => await learningTools!.show(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-get-state",
    tool: async () => await learningTools!.getState(),
  },
  {
    name: "qdk-learning-get-progress",
    tool: async () => await learningTools!.getProgress(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-list-units",
    tool: async (input) => await learningTools!.listUnits(input),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-next",
    tool: async () => await learningTools!.next(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-previous",
    tool: async () => await learningTools!.previous(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-goto",
    tool: async (input) => await learningTools!.goTo(input),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-run",
    tool: async (input) => await learningTools!.run(input),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-read-code",
    tool: async () => await learningTools!.readCode(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-check",
    tool: async () => await learningTools!.check(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-hint",
    tool: async () => await learningTools!.hint(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-solution",
    tool: async () => await learningTools!.solution(),
    confirm: async () => learningTools!.confirmInit(),
  },
  {
    name: "qdk-learning-reset",
    tool: async () => await learningTools!.resetExercise(),
    confirm: async () => {
      // If the service is not yet initialized, show the init confirmation
      // first. Otherwise show the reset-specific confirmation.
      const initConfirm = await learningTools!.confirmInit();
      if (initConfirm) {
        return initConfirm;
      }
      return {
        confirmationMessages: {
          title: "Reset Exercise",
          message:
            "Reset the current exercise to the original placeholder? Your code will be lost.",
        },
      };
    },
  },
];

export function registerLanguageModelTools(
  context: vscode.ExtensionContext,
  learningService: LearningService,
) {
  qsharpTools = new QSharpTools(context.extensionUri);
  learningTools = new LearningTools(learningService);
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
  confirmFn?: (
    input: T,
  ) => vscode.ProviderResult<vscode.PreparedToolInvocation>,
): vscode.LanguageModelTool<any> {
  return {
    invoke: (options: vscode.LanguageModelToolInvocationOptions<T>) =>
      invokeTool(context, toolName, options, toolFn),
    prepareInvocation: confirmFn
      ? (options: vscode.LanguageModelToolInvocationPrepareOptions<T>) =>
          confirmFn(options.input)
      : undefined,
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
