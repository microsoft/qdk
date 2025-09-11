// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, QdkDiagnostics, TargetProfile } from "qsharp-lang";
import * as vscode from "vscode";
import { getCircuitOrErrorWithTimeout } from "../circuit";
import { qsharpExtensionId } from "../common";
import { getUploadSupplementalData } from "../config";
import { FullProgramConfig, getActiveProgram } from "../programConfig";
import { getQirForProgram, QirGenerationError } from "../qirGeneration";
import {
  EventType,
  getActiveDocumentType,
  QsharpDocumentType,
  sendTelemetryEvent,
  UserFlowStatus,
  UserTaskInvocationType,
} from "../telemetry";
import { getRandomGuid } from "../utils";
import { sendMessageToPanel } from "../webviewPanel";
import { getTokenForWorkspace } from "./auth";
import { QuantumUris, StorageUris } from "./networkRequests";
import {
  getPreferredTargetProfile,
  targetSupportQir,
} from "./providerProperties";
import { startRefreshCycle } from "./treeRefresher";
import {
  Job,
  Target,
  WorkspaceConnection,
  WorkspaceTreeItem,
  WorkspaceTreeProvider,
} from "./treeView";
import {
  getAzurePortalWorkspaceLink,
  getJobFiles,
  getPythonCodeForWorkspace,
  queryWorkspaces,
  submitJob,
  uploadBlob,
} from "./workspaceActions";

const workspacesSecret = `${qsharpExtensionId}.workspaces`;
let extensionUri: vscode.Uri;

export async function initAzureWorkspaces(context: vscode.ExtensionContext) {
  extensionUri = context.extensionUri;
  const workspaceTreeProvider = new WorkspaceTreeProvider();
  WorkspaceTreeProvider.instance = workspaceTreeProvider;

  const treeView = vscode.window.createTreeView("quantum-workspaces", {
    treeDataProvider: workspaceTreeProvider,
  });
  context.subscriptions.push(treeView);

  let currentTreeItem: WorkspaceTreeItem | undefined = undefined;

  // Add any previously saved workspaces
  const savedWorkspaces = await context.secrets.get(workspacesSecret);
  if (savedWorkspaces) {
    log.debug("Loading workspaces: ", savedWorkspaces);
    const workspaces: WorkspaceConnection[] = JSON.parse(savedWorkspaces);
    for (const workspace of workspaces) {
      workspaceTreeProvider.updateWorkspace(workspace);
      // Start refreshing each workspace until pending jobs are complete
      startRefreshCycle(workspaceTreeProvider, workspace);
    }
  } else {
    log.debug("No saved workspaces found.");
  }

  context.subscriptions.push(
    treeView.onDidChangeSelection(async (e) => {
      // Capture the selected item and set context if the supports job submission or results download.
      let supportsQir = false;
      let supportsDownload = false;
      let isWorkspace = false;

      if (e.selection.length === 1) {
        currentTreeItem = e.selection[0] as WorkspaceTreeItem;
        if (
          currentTreeItem.type === "target" &&
          targetSupportQir(currentTreeItem.label?.toString() || "")
        ) {
          supportsQir = true;
        }
        if (currentTreeItem.type === "job") {
          const job = currentTreeItem.itemData as Job;
          if (job.status === "Succeeded" && job.outputDataUri) {
            supportsDownload = true;
          }
        }
        if (currentTreeItem.type === "workspace") {
          isWorkspace = true;
        }
      } else {
        currentTreeItem = undefined;
      }

      await vscode.commands.executeCommand(
        "setContext",
        `${qsharpExtensionId}.treeItemSupportsQir`,
        supportsQir,
      );
      await vscode.commands.executeCommand(
        "setContext",
        `${qsharpExtensionId}.treeItemSupportsDownload`,
        supportsDownload,
      );
      await vscode.commands.executeCommand(
        "setContext",
        `${qsharpExtensionId}.treeItemIsWorkspace`,
        isWorkspace,
      );
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.targetSubmit`,
      async (arg: WorkspaceTreeItem) => {
        // Could be run via the treeItem icon or the menu command.
        const treeItem = arg || currentTreeItem;
        if (treeItem?.type !== "target") return;

        const target = treeItem.itemData as Target;

        try {
          const preferredTargetProfile = getPreferredTargetProfile(target.id);
          const program = await getActiveProgram({
            showModalError: true,
            targetProfileFallback: preferredTargetProfile,
          });

          if (!program.success) {
            throw new QirGenerationError(program.errorMsg);
          }

          await compileAndSubmit(
            program.programConfig,
            preferredTargetProfile,
            getActiveDocumentType(),
            UserTaskInvocationType.Command,
            workspaceTreeProvider,
            treeItem.workspace,
            target,
          );
        } catch (e: unknown) {
          log.warn("Failed to submit job. ", e);

          if (e instanceof QirGenerationError) {
            vscode.window.showErrorMessage(e.message, { modal: true });
            return;
          }

          vscode.window.showErrorMessage("Failed to submit the job to Azure.", {
            modal: true,
            detail: e instanceof Error ? e.message : undefined,
          });
        }
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.workspacesRefresh`,
      () => {
        // The user manually triggered a refresh. Start a cycle for each workspace
        const workspaceIds = workspaceTreeProvider.getWorkspaceIds();

        workspaceIds.forEach((id) => {
          const workspace = workspaceTreeProvider.getWorkspace(id);
          if (workspace) {
            startRefreshCycle(workspaceTreeProvider, workspace);
          }
        });
      },
    ),
  );

  async function saveWorkspaceList() {
    // Save the list of workspaces
    const savedWorkspaces: WorkspaceConnection[] = [];
    const workspaces = workspaceTreeProvider
      .getWorkspaceIds()
      .map((id) => workspaceTreeProvider.getWorkspace(id)!);

    for (const elem of workspaces) {
      // Save only the general workspace information, not the providers and jobs
      savedWorkspaces.push({
        id: elem.id,
        name: elem.name,
        endpointUri: elem.endpointUri,
        tenantId: elem.tenantId,
        apiKey: elem.apiKey,
        providers: [],
        jobs: [],
      });
    }
    log.debug("Saving workspaces: ", savedWorkspaces);
    await context.secrets.store(
      workspacesSecret,
      JSON.stringify(savedWorkspaces),
    );
  }

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.workspacesAdd`,
      async () => {
        const workspace = await queryWorkspaces();
        if (workspace) {
          workspaceTreeProvider.updateWorkspace(workspace);
          await saveWorkspaceList();
          // Just kick off the refresh loop, no need to await
          startRefreshCycle(workspaceTreeProvider, workspace);
        }
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.workspacesRemove`,
      async (arg: WorkspaceTreeItem) => {
        // Could be run via the treeItem icon or the menu command.
        const treeItem = arg || currentTreeItem;
        if (treeItem?.type !== "workspace") return;
        const workspace = treeItem.itemData as WorkspaceConnection;

        workspaceTreeProvider.removeWorkspace(workspace.id);
        await saveWorkspaceList();
      },
    ),
  );

  async function downloadResults(arg?: WorkspaceTreeItem, showText?: boolean) {
    // Could be run via the treeItem icon or the menu command.
    const treeItem = arg || currentTreeItem;
    if (treeItem?.type !== "job") return;

    const job = treeItem.itemData as Job;

    if (!job.outputDataUri) {
      log.error("Download called for job with null outputDataUri", job);
      return;
    }

    const fileUri = vscode.Uri.parse(job.outputDataUri);
    const [, container, blob] = fileUri.path.split("/");

    const quantumUris = new QuantumUris(
      treeItem.workspace.endpointUri,
      treeItem.workspace.id,
    );

    try {
      const token = await getTokenForWorkspace(treeItem.workspace);
      if (!token) return;

      const file = await getJobFiles(container, blob, token, quantumUris);
      const buckets = !showText && getHistogramBucketsFromData(file, job.shots);
      if (buckets) {
        sendMessageToPanel({ panelType: "histogram", id: job.name }, true, {
          ...buckets,
          suppressSettings: true, // Don't want to show noise settings on downloaded results
        });
      } else {
        if (!showText)
          vscode.window.showInformationMessage(
            "Unable to display results as a histogram. Opening as text.",
          );
        const doc = await vscode.workspace.openTextDocument({
          content: file,
          language: "json",
        });
        vscode.window.showTextDocument(doc);
      }
    } catch (e: any) {
      log.error("Failed to download result file. ", e);
      vscode.window.showErrorMessage("Failed to download the results file.", {
        modal: true,
        detail: e instanceof Error ? e.message : undefined,
      });
    }
  }

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.downloadResults`,
      async (arg: WorkspaceTreeItem) => await downloadResults(arg, false),
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.downloadRawResults`,
      async (arg: WorkspaceTreeItem) => await downloadResults(arg, true),
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.workspacePythonCode`,
      async (arg: WorkspaceTreeItem) => {
        // Could be run via the treeItem icon or the menu command.
        const treeItem = arg || currentTreeItem;
        if (treeItem?.type !== "workspace") return;
        const workspace = treeItem.itemData as WorkspaceConnection;
        const str = getPythonCodeForWorkspace(
          workspace.id,
          workspace.endpointUri,
          workspace.name,
        );
        if (str) {
          vscode.env.clipboard.writeText(str);
          vscode.window.showInformationMessage(
            "Python code has been copied to the clipboard",
          );
        } else {
          vscode.window.showErrorMessage(
            "Failed to generate Python code for workspace",
          );
        }
      },
    ),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.workspaceOpenPortal`,
      async (arg: WorkspaceTreeItem) => {
        // Could be run via the treeItem icon or the menu command.
        const treeItem = arg || currentTreeItem;
        if (treeItem?.type !== "workspace") return;
        const workspace = treeItem.itemData as WorkspaceConnection;

        const link = getAzurePortalWorkspaceLink(workspace);
        vscode.env.openExternal(vscode.Uri.parse(link));
      },
    ),
  );
}

type Buckets = {
  buckets: [string, number][];
  shotCount: number;
};
function getHistogramBucketsFromData(
  file: string,
  shotCount?: number,
): Buckets | undefined {
  try {
    const parsed = JSON.parse(file);
    if (!parsed || typeof parsed !== "object") {
      throw "Histogram data is not in the expected format";
    }
    if (parsed.DataFormat === "microsoft.quantum-results.v2") {
      // New 'v2' format will be in the format
      // {
      //   "DataFormat": "microsoft.quantum-results.v2",
      //   "Results": [
      //     {
      //       "Histogram": [
      //         { "Outcome": [0, 1, 1, 1], "Display": "[0, 1, 1, 1]", "Count": 8 },
      //         { "Outcome": [1, 1, 0, 0], "Display": "[1, 1, 0, 0]", "Count": 10 },
      // etc..
      // Only Results[0] is used (batching may have more entries, but we don't support that)
      type v2Bucket = { Display: string; Count: number };
      const histogramData: v2Bucket[] = parsed.Results?.[0]?.Histogram;
      if (!Array.isArray(histogramData)) throw "Histogram data not found";
      const buckets: Array<[string, number]> = [];
      let shotTotal = 0;
      histogramData.forEach((entry) => {
        shotTotal += entry.Count;
        buckets.push([entry.Display, entry.Count]);
      });

      return { buckets, shotCount: shotTotal };
    } else if (Array.isArray(parsed.Histogram)) {
      // v1 format should be an object with a "Histogram" property, which is an array of ["label", <float>, ...] entries.
      // e.g., something as simple as: {"Histogram":["(0, 0)",0.54,"(1, 1)",0.46]}
      // Note this doesn't include the shotCount, so we need it from the job
      if (!shotCount || !Number.isInteger(shotCount)) {
        throw "job.shots data was not a positive integer";
      }

      // Turn the flat histogram list into buckets for the histogram.
      const histogram: Array<string | number> = parsed.Histogram;
      const buckets: Array<[string, number]> = [];
      for (let i = 0; i < parsed.Histogram.length; i += 2) {
        const label = histogram[i].toString();
        const value = histogram[i + 1]; // This is a percentage, not a count
        if (typeof value !== "number") throw "Invalid histogram value";
        buckets.push([label, Math.round(value * shotCount)]);
      }

      return { buckets, shotCount };
    } else {
      throw "Unrecognized histogram file format";
    }
  } catch (e: any) {
    log.debug("Failed to parse job results as histogram data.", file, e);
  }
  return undefined;
}

export async function compileAndSubmit(
  program: FullProgramConfig,
  targetProfile: TargetProfile,
  telemetryDocumentType: QsharpDocumentType,
  telemetryInvocationType: UserTaskInvocationType,
  workspaceTreeProvider: WorkspaceTreeProvider,
  workspace: WorkspaceConnection,
  target: Target,
  parameters: { jobName: string; shots: number } | undefined = undefined,
) {
  const associationId = getRandomGuid();
  const start = performance.now();
  sendTelemetryEvent(
    EventType.SubmitToAzureStart,
    { associationId, invocationType: telemetryInvocationType },
    {},
  );

  const qir = await getQirForProgram(
    program,
    targetProfile,
    telemetryDocumentType,
  );

  if (!parameters) {
    const result = await promptForJobParameters();
    if (!result) {
      sendTelemetryEvent(
        EventType.SubmitToAzureEnd,
        {
          associationId,
          reason: "user cancelled parameter input",
          flowStatus: UserFlowStatus.Aborted,
        },
        { timeToCompleteMs: performance.now() - start },
      );
      return;
    }
    parameters = { jobName: result.jobName, shots: result.numberOfShots };
  }

  const { jobId, storageUris, quantumUris, token } = await submitJob(
    workspace,
    associationId,
    qir,
    target.providerId,
    target.id,
    parameters.jobName,
    parameters.shots,
  );

  sendTelemetryEvent(
    EventType.SubmitToAzureEnd,
    {
      associationId,
      reason: "job submitted",
      flowStatus: UserFlowStatus.Succeeded,
    },
    { timeToCompleteMs: performance.now() - start },
  );

  // The job submitted fine. Refresh the workspace until it shows up
  // and all jobs are finished. Don't await on this, just let it run
  startRefreshCycle(workspaceTreeProvider, workspace, jobId);

  if (getUploadSupplementalData()) {
    // Now generate and upload the supplemental data.
    // Fire and forget - the supplemental data is best-effort .
    uploadSupplementalData(
      program,
      storageUris,
      quantumUris,
      token,
      associationId,
    ).catch((e) => {
      log.warn("Failed to upload supplemental job data", e);
    });
  }

  return jobId;
}

async function promptForJobParameters(): Promise<
  { jobName: string; numberOfShots: number } | undefined
> {
  const jobName = await vscode.window.showInputBox({
    prompt: "Job name",
    value: new Date().toISOString(),
  });
  if (!jobName) return;

  // validator for the user-provided number of shots input
  const validateShotsInput = (input: string) => {
    const result = parseFloat(input);
    if (isNaN(result) || Math.floor(result) !== result) {
      return "Number of shots must be an integer";
    }
  };

  // prompt the user for the number of shots
  const numberOfShotsPrompted = await vscode.window.showInputBox({
    value: "100",
    prompt: "Number of shots",
    validateInput: validateShotsInput,
  });

  // abort if the user hits <Esc> during shots entry
  if (numberOfShotsPrompted === undefined) {
    return;
  }

  const numberOfShots = parseInt(numberOfShotsPrompted);
  return { jobName, numberOfShots };
}

/**
 * Uploads supplemental input data for the job, which is currently just
 * the circuit diagram (if it can be generated).
 *
 * Throws an exception if any part of this process fails.
 */
async function uploadSupplementalData(
  program: FullProgramConfig,
  storageUris: StorageUris,
  quantumUris: QuantumUris,
  token: string,
  associationId: string,
) {
  const circuitDiagram = await getCircuitJson(program);

  await uploadBlob(
    storageUris,
    quantumUris,
    token,
    "circuitDiagram",
    circuitDiagram,
    "application/json",
    associationId,
  );
}

/**
 * Generates a circuit diagram for the program, or throws if it can't be generated.
 */
async function getCircuitJson(program: FullProgramConfig): Promise<string> {
  const circuit = await getCircuitOrErrorWithTimeout(
    extensionUri,
    {
      program,
    },
    false,
    5000, // If we can't generate in 5 seconds, give up - something's wrong or program is way too complex
  );

  if (circuit.result === "success") {
    return JSON.stringify(circuit.circuit);
  } else {
    if (circuit.errors?.length > 0) {
      throw new QdkDiagnostics(circuit.errors);
    }
    if (circuit.timeout) {
      throw new Error(`Timed out generating circuit`);
    }
    throw new Error("Unknown error generating circuit diagram");
  }
}
