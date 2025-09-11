// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, TargetProfile } from "qsharp-lang";
import * as vscode from "vscode";
import { getTokenForWorkspace } from "../azure/auth.js";
import { compileAndSubmit } from "../azure/commands.js";
import { QuantumUris } from "../azure/networkRequests.js";
import { getPreferredTargetProfile } from "../azure/providerProperties.js";
import {
  Job,
  Provider,
  Target,
  WorkspaceConnection,
  WorkspaceTreeProvider,
} from "../azure/treeView.js";
import { getJobFiles } from "../azure/workspaceActions.js";
import { QirGenerationError } from "../qirGeneration.js";
import { UserTaskInvocationType } from "../telemetry.js";
import { sendMessageToPanel } from "../webviewPanel.js";
import { ProjectInfo, QSharpTools } from "./qsharpTools.js";
import { CopilotToolError, HistogramData } from "./types.js";

export type ToolResult<T = any> = { result: T };
/**
 * State that can be shared between tool calls in a conversation.
 */
export type ToolState = Record<string, any>;

/**
 * Gets the first available workspace connection, or throws if there are none.
 */
async function getFirstWorkspace(): Promise<WorkspaceConnection> {
  const tree = WorkspaceTreeProvider.instance;
  const workspaces = tree.getWorkspaceIds();
  const workspace = workspaces[0] && tree.getWorkspace(workspaces[0]);
  if (workspace) {
    return workspace;
  } else {
    throw new CopilotToolError(
      "There are no Azure Quantum workspace connections set up.",
    );
  }
}

/**
 * Gets the current workspace for the conversation,
 * or the first available workspace workspace connection if none has been set.
 * Throws if there are no workspace connections available.
 */
async function getConversationWorkspace(
  toolState: ToolState,
): Promise<WorkspaceConnection> {
  if (toolState.activeWorkspace) {
    return toolState.activeWorkspace;
  } else {
    const initialWorkspaceResult = await getFirstWorkspace();
    toolState.activeWorkspace = initialWorkspaceResult;
    return initialWorkspaceResult;
  }
}

/**
 * Gets the list of available workspace connections.
 */
export async function getWorkspaces(): Promise<{
  result: { workspaceIds: string[] };
}> {
  return {
    result: { workspaceIds: WorkspaceTreeProvider.instance.getWorkspaceIds() },
  };
}

/**
 * Gets the ID of the active workspace for this conversation,
 * or throws if no workspace connections are available.
 */
export async function getActiveWorkspace(
  toolState: ToolState,
): Promise<{ result: { workspaceId: string } }> {
  const workspace = await getConversationWorkspace(toolState);
  return { result: { workspaceId: workspace.id } };
}

/**
 * Sets the active workspace for this conversation.
 */
export async function setActiveWorkspace(
  toolState: ToolState,
  { workspace_id: workspaceId }: { workspace_id: string },
): Promise<{ result: string }> {
  const tree = WorkspaceTreeProvider.instance;
  const workspace = tree.getWorkspace(workspaceId);
  if (!workspace) {
    throw new CopilotToolError(
      "A workspace with id " + workspaceId + " was not found.",
    );
  } else {
    toolState.activeWorkspace = workspace;
    return { result: "Workspace " + workspaceId + " set as active." };
  }
}

const jobLimit = 10;
const jobLimitDays = 14;

async function getRecentJobs(
  workspace: WorkspaceConnection,
  lastNDays?: number,
): Promise<Job[]> {
  if (workspace) {
    const jobs = workspace.jobs;

    const start = new Date();
    start.setHours(0, 0, 0, 0);
    start.setDate(start.getDate() - (lastNDays ?? jobLimitDays));

    const limitedJobs = (
      jobs.length > jobLimit ? jobs.slice(0, jobLimit) : jobs
    ).filter((j) => new Date(j.creationTime) > start);
    return limitedJobs;
  } else {
    return [];
  }
}

export async function getJobs(
  conversationState: ToolState,
  args?: { lastNDays: number },
): Promise<{
  result: {
    recentJobs: JobOverview[];
    lastNJobs: number;
    lastNDays: number;
  };
}> {
  const workspace = await getConversationWorkspace(conversationState);

  const recentJobs = (await getRecentJobs(workspace, args?.lastNDays)).map(
    (job) => {
      // Don't return the object directly as it may contain extra properties
      // that may be too large when the tool output is JSON-ified.
      // (notably `errorData`).
      //
      // Only explicitly include fields that are part of the `JobOverview` type,
      // drop the rest.
      return {
        id: job.id,
        name: job.name,
        target: job.target,
        status: job.status,
        count: job.count,
        shots: job.shots,
        creationTime: job.creationTime,
        beginExecutionTime: job.beginExecutionTime,
        endExecutionTime: job.endExecutionTime,
        cancellationTime: job.cancellationTime,
        costEstimate: job.costEstimate,
      };
    },
  );

  return {
    result: {
      recentJobs,
      lastNJobs: jobLimit,
      lastNDays: args?.lastNDays ?? jobLimitDays,
    },
  };
}

/**
 * This is similar to the `Job` type but with only the fields we want
 * to include in the overall `GetJobs` output. Notably, it excludes
 * `errorData` since its size is unbounded.
 */
type JobOverview = {
  id: string;
  name: string;
  target: string;
  status:
    | "Waiting"
    | "Executing"
    | "Succeeded"
    | "Failed"
    | "Finishing"
    | "Cancelled";
  count?: number;
  shots?: number;
  creationTime: string;
  beginExecutionTime?: string;
  endExecutionTime?: string;
  cancellationTime?: string;
  costEstimate?: any;
};

/**
 * Gets job details for the job with the given ID from the active workspace,
 * or throws if the job is not found.
 */
export async function getJob(
  toolState: ToolState,
  { job_id }: { job_id: string },
): Promise<{
  result: Job;
}> {
  const workspace = await getConversationWorkspace(toolState);
  const jobs = workspace.jobs;
  const job = jobs.find((job) => job.id === job_id);

  if (!job) {
    throw new CopilotToolError(
      "A job with ID " + job_id + " was not found in the workspace.",
    );
  }

  return { result: job };
}

type DownloadJobResult = {
  result: string;
  widgetData?: HistogramData;
};

/**
 * Download the results of the job with the given ID from the active workspace.
 * Throws if the job can't be found or the results can't be downloaded for any reason.
 */
export async function downloadJobResults(
  toolState: ToolState,
  args: { job_id: string },
): Promise<DownloadJobResult> {
  const job = (await getJob(toolState, args)).result;

  if (job.status !== "Succeeded") {
    throw new CopilotToolError("Job has not completed successfully.");
  }

  const workspace = await getConversationWorkspace(toolState);

  if (!job.outputDataUri) {
    log.error("Download called for job with null outputDataUri", job);
    throw new CopilotToolError(
      "Failed to download the output data for the job.",
    );
  }

  const fileUri = vscode.Uri.parse(job.outputDataUri);
  const [, container, blob] = fileUri.path.split("/");

  const quantumUris = new QuantumUris(workspace.endpointUri, workspace.id);

  const token = await getTokenForWorkspace(workspace);
  if (!token) {
    log.error("Unable to get token for the workspace", workspace);
    throw new CopilotToolError("Failed to connect to the workspace.");
  }

  try {
    const outputData = await getJobFiles(container, blob, token, quantumUris);
    const buckets = formatHistogramBuckets(outputData);
    const shotCount = job.shots ?? job.count;
    if (!buckets || !shotCount) {
      const doc = await vscode.workspace.openTextDocument({
        content: outputData,
        language: "json",
      });
      vscode.window.showTextDocument(doc);
      return { result: "Results downloaded successfully as file." };
    } else {
      const histogram = {
        buckets,
        shotCount,
      };
      sendMessageToPanel(
        { panelType: "histogram", id: args.job_id },
        false,
        histogram,
      );
      return {
        result: JSON.stringify({ histogram }),
        widgetData: histogram,
      };
    }
  } catch {
    throw new CopilotToolError("Failed to download the results for the job.");
  }
}

/**
 * Convert raw output data from a job to the histogram buckets
 * format we use for display.
 */
function formatHistogramBuckets(
  outputData: string,
): [string, number][] | undefined {
  try {
    // Parse the JSON file
    const parsedArray = JSON.parse(outputData).Histogram as Array<any>;

    if (parsedArray.length % 2 !== 0) {
      // "Data is not in correct format for histogram."
      return undefined;
    }

    // Transform the flat array into an array of pairs [string, number]
    const histo: Array<[string, number]> = [];
    for (let i = 0; i < parsedArray.length; i += 2) {
      histo.push([parsedArray[i], parsedArray[i + 1]]);
    }

    return histo;
  } catch (e: any) {
    log.error("Error rendering results as histogram: ", e);
    return undefined;
  }
}

type ProviderWithPreferredTargetProfile = Provider & {
  targets: (Target & {
    preferredProfile: TargetProfile;
  })[];
};

/**
 * Gets the list of the providers and targets in the current workspace.
 */
export async function getProviders(toolState: ToolState): Promise<{
  result: ProviderWithPreferredTargetProfile[];
}> {
  const workspace = await getConversationWorkspace(toolState);
  const providers = workspace?.providers ?? [];

  const providersWithPreferredProfile = providers.map((p) => {
    return {
      ...p,
      targets: p.targets.map((t) => {
        return {
          ...t,
          preferredProfile: getPreferredTargetProfile(t.id),
        };
      }),
    };
  });

  return { result: providersWithPreferredProfile };
}

/**
 * Gets details about a specific target by its name.
 */
export async function getTarget(
  toolState: ToolState,
  { target_id }: { target_id: string },
): Promise<{ result: Target | undefined }> {
  const providers = (await getProviders(toolState)).result;
  for (const provider of providers) {
    const target = provider.targets.find((target) => target.id === target_id);
    if (target) {
      return { result: target };
    }
  }
  return { result: undefined };
}

/**
 * Submits the Q# or OpenQASM program in the currently visible editor window to Azure Quantum.
 */
export async function submitToTarget(
  toolState: ToolState,
  qsharpTools: QSharpTools,
  {
    filePath,
    jobName,
    targetId,
    shots,
  }: {
    filePath: string;
    jobName: string;
    targetId: string;
    shots: number;
  },
): Promise<ProjectInfo & { result: string }> {
  try {
    const target = (await getTarget(toolState, { target_id: targetId })).result;
    if (!target) {
      throw new CopilotToolError(
        "A target with the name " +
          targetId +
          " does not exist in the workspace.",
      );
    }

    if (target.currentAvailability !== "Available")
      throw new CopilotToolError(
        "The target " + targetId + " is not available.",
      );

    const preferredTargetProfile = getPreferredTargetProfile(target.id);
    const program = await qsharpTools.getProgram(filePath, {
      targetProfileFallback: preferredTargetProfile,
    });
    const programConfig = program.config;

    const workspace = await getConversationWorkspace(toolState);

    const jobId = await compileAndSubmit(
      programConfig,
      preferredTargetProfile,
      program.telemetryDocumentType,
      UserTaskInvocationType.ChatToolCall,
      WorkspaceTreeProvider.instance,
      workspace,
      target,
      { jobName, shots },
    );

    return {
      result: "Job submitted successfully with ID: " + jobId,
      ...program.additionalContextForModel,
    };
  } catch (e: any) {
    if (e instanceof QirGenerationError) {
      throw new CopilotToolError(e.message);
    }
    const error = e instanceof Error ? e.message : "";
    throw new CopilotToolError("Failed to submit the job. " + error);
  }
}

/**
 * Starts the user flow to connect to an Azure Quantum Workspace.
 */
export async function connectToWorkspace(
  conversationState: ToolState,
): Promise<{ result: string }> {
  const initialWsList = await getWorkspaces();
  try {
    await vscode.commands.executeCommand("qsharp-vscode.workspacesAdd");
  } catch {
    throw new CopilotToolError(
      `An error occurred while trying to connect to an Azure Quantum Workspace.`,
    );
  }
  const newWsList = await getWorkspaces();
  // Kind of a silly way to do this, but the new workspace is the one
  // that exists in `newWsList` but not in `initialWsList`
  const newWorkspaceId = newWsList.result.workspaceIds.filter(
    (id: string) => !initialWsList.result.workspaceIds.includes(id),
  )[0];

  if (newWorkspaceId) {
    await setActiveWorkspace(conversationState, {
      workspace_id: newWorkspaceId,
    });

    return {
      result:
        "Connected to Azure Quantum Workspace with ID `" + newWorkspaceId + "`",
    };
  }

  return {
    result:
      "A new workspace was not added. Available workspaces: " +
      JSON.stringify(newWsList),
  };
}
