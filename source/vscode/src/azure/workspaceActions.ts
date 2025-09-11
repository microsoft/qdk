// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import {
  azureRequest,
  AzureUris,
  QuantumUris,
  ResponseTypes,
  storageRequest,
  StorageUris,
} from "./networkRequests";
import { Job, WorkspaceConnection } from "./treeView";
import {
  shouldExcludeProvider,
  shouldExcludeTarget,
} from "./providerProperties";
import { getRandomGuid } from "../utils";
import { EventType, sendTelemetryEvent, UserFlowStatus } from "../telemetry";
import { getTenantIdAndToken, getTokenForWorkspace } from "./auth";

export function getAzurePortalWorkspaceLink(workspace: WorkspaceConnection) {
  // Portal link format:
  // - https://portal.azure.com/#resource/subscriptions/<sub guid>/resourceGroups/<group>/providers/Microsoft.Quantum/Workspaces/<name>/overview

  return `https://portal.azure.com/#resource${workspace.id}/overview`;
}

export type EndEventProperties = {
  associationId: string;
  reason: string;
  flowStatus: UserFlowStatus;
};

export async function queryWorkspaces(): Promise<
  WorkspaceConnection | undefined
> {
  log.debug("Querying for account workspaces");

  // Show a quick-pick to ask if they want to connect via AzureAD or a connection string
  const choice = await vscode.window.showQuickPick(
    [
      { label: "Azure account", type: "aad" },
      { label: "Connection string", type: "connStr" },
    ],
    { canPickMany: false, title: "Select an authentication method" },
  );
  if (!choice?.type) return;

  const start = performance.now();
  const associationId = getRandomGuid();
  const endEventProperties: EndEventProperties = {
    associationId,
    reason: "",
    flowStatus: UserFlowStatus.Succeeded,
  };

  sendTelemetryEvent(EventType.QueryWorkspaceStart, { associationId }, {});
  try {
    switch (choice?.type) {
      case "connStr":
        endEventProperties.reason = "Queried via connection string";
        return await getWorkspaceWithConnectionString(endEventProperties);
      case "aad":
        endEventProperties.reason = "Queried via AzureAD";
        return await getWorkspaceWithAzureAD(endEventProperties);
      default:
        throw Error("Unexpected connection type");
    }
  } catch (e: any) {
    // Handle exceptions in Azure requests, etc.
    endEventProperties.reason = e.message || "Unexpected exception occurred";
    endEventProperties.flowStatus = UserFlowStatus.Failed;
  } finally {
    sendTelemetryEvent(EventType.QueryWorkspaceEnd, endEventProperties, {
      timeToCompleteMs: performance.now() - start,
    });
  }
}

export function getPythonCodeForWorkspace(
  id: string,
  endpointUri: string,
  name: string,
) {
  // id starts with the pattern: "/subscriptions/<sub guid>/resourceGroups/<group>>/providers/Microsoft.Quantum/Workspaces/<name>"
  // endpointUri format: "https:/westus2.quantum.azure.com"

  // Regular expression to extract subscriptionId and resourceGroup from the id
  const idRegex =
    /\/subscriptions\/(?<subscriptionId>[^/]+)\/resourceGroups\/(?<resourceGroup>[^/]+)/;

  // Regular expression to extract the first part of the endpointUri
  const endpointRegex = /https:\/\/(?<location>[^.]+)\./;

  const idMatch = id.match(idRegex);
  const endpointMatch = endpointUri.match(endpointRegex);

  const subscriptionId = idMatch?.groups?.subscriptionId;
  const resourceGroup = idMatch?.groups?.resourceGroup;
  const location = endpointMatch?.groups?.location;

  // TODO: Mention how to fetch/use connection strings

  const pythonCode = `# If developing locally, on first run this will open a browser to authenticate the
# connection with Azure. In remote scenarios, such as SSH or Codespaces, it may
# be necessary to install the Azure CLI and run 'az login --use-device-code' to
# authenticate. For unattended scenarios, such as batch jobs, a service principal
# should be configured and used for authentication. For more information, see
# https://learn.microsoft.com/en-us/azure/developer/python/sdk/authentication-overview

from azure.quantum import Workspace

# If using an access key, replace the below with: Workspace.from_connection_string(connection_string)
# Or set the "AZURE_QUANTUM_CONNECTION_STRING" environment variable and just use: Workspace()
# See https://learn.microsoft.com/en-us/azure/quantum/how-to-connect-workspace for more details.

workspace = Workspace(
    subscription_id = "${subscriptionId || "MY_SUBSCRIPTION_ID"}",
    resource_group = "${resourceGroup || "MY_RESOURCE_GROUP"}",
    name = "${name || "MY_WORKSPACE_NAME"}",
    location = "${location || "MY_WORKSPACE_LOCATION"}",
)
`;

  return pythonCode;
}

async function getWorkspaceWithConnectionString(
  endEventProperties: EndEventProperties,
): Promise<WorkspaceConnection | undefined> {
  for (;;) {
    const connStr = await vscode.window.showInputBox({
      prompt: "Enter the connection string",
      placeHolder:
        "SubscriptionId=<guid>;ResourceGroupName=<name>;WorkspaceName=<name>;ApiKey=<secret>;QuantumEndpoint=<serviceUri>;",
    });
    if (!connStr) {
      endEventProperties.reason = "no connection string entered";
      endEventProperties.flowStatus = UserFlowStatus.Aborted;
      return;
    }

    const partsMap = new Map<string, string>();
    connStr.split(";").forEach((part) => {
      const eq = part.indexOf("=");
      if (eq === -1) return;
      partsMap.set(part.substring(0, eq).toLowerCase(), part.substring(eq + 1));
    });

    if (
      !partsMap.has("subscriptionid") ||
      !partsMap.has("resourcegroupname") ||
      !partsMap.has("workspacename") ||
      !partsMap.has("apikey") ||
      !partsMap.has("quantumendpoint")
    ) {
      const action = await vscode.window.showErrorMessage(
        "Invalid connection string. Please follow the placeholder format.",
        { modal: true },
        "Retry",
      );
      if (action === "Retry") {
        continue;
      } else {
        endEventProperties.reason = "invalid connection string entered";
        endEventProperties.flowStatus = UserFlowStatus.Aborted;
        return;
      }
    }

    const workspaceId =
      `/subscriptions/${partsMap.get("subscriptionid")}` +
      `/resourceGroups/${partsMap.get("resourcegroupname")}` +
      `/providers/Microsoft.Quantum/Workspaces/${partsMap.get(
        "workspacename",
      )}`;

    const workspace: WorkspaceConnection = {
      id: workspaceId,
      name: partsMap.get("workspacename")!,
      endpointUri: partsMap.get("quantumendpoint")!,
      tenantId: "", // Blank means not authenticated via a token
      apiKey: partsMap.get("apikey"),
      providers: [], // Providers and jobs will be populated by a following 'queryWorkspace' call
      jobs: [],
    };

    // Validate the connection string info before returning as valid for further use.
    try {
      const token = await getTokenForWorkspace(workspace);
      const quantumUris = new QuantumUris(workspace.endpointUri, workspace.id);
      const associationId = getRandomGuid();
      const providerStatus: ResponseTypes.ProviderStatusList =
        await azureRequest(quantumUris.providerStatus(), token, associationId);
      if (!providerStatus.value?.length) {
        // There should always be providers, so this is an exceptional condition
        throw Error("No providers returned");
      }
    } catch (e: any) {
      log.debug("Workspace connection error", e);
      // e.g. check for 401 (invalid key), 404 (invalid workspace), failed network requests (invalid endpoint), etc.
      let errorText = "An unexpected error occured";
      const message: string | undefined = e.message;
      if (message?.includes("status 401")) {
        errorText =
          "Authentication failed. Please check the ApiKey is valid and active.";
      } else if (message?.includes("status 404")) {
        errorText =
          "Workspace not found. Please check the WorkspaceName and ResourceGroupName values.";
      } else if (message?.includes("Failed to fetch")) {
        errorText =
          "Request failed. Please check the QuantumEndpoint value and network connectivity.";
      }
      const action = await vscode.window.showErrorMessage(
        errorText,
        { modal: true },
        "Retry",
      );
      if (action === "Retry") {
        continue;
      } else {
        endEventProperties.reason = errorText;
        endEventProperties.flowStatus = UserFlowStatus.Aborted;
        return;
      }
    }

    return workspace;
  }
}

async function chooseWorkspaceFromSubscription(
  subId: string,
  token: string,
  endEventProperties: EndEventProperties,
) {
  // *** Fetch the Quantum Workspaces in the subscription ***
  const workspaces: ResponseTypes.WorkspaceList = await azureRequest(
    AzureUris.workspaces(subId),
    token,
    endEventProperties.associationId,
  );
  if (log.getLogLevel() >= 5) {
    log.trace(`Got workspaces: ${JSON.stringify(workspaces, null, 2)}`);
  }
  if (!workspaces.value.length) {
    log.info("No workspaces returned for the subscription");
    endEventProperties.reason = "no quantum workspaces in azure subscription";
    endEventProperties.flowStatus = UserFlowStatus.Aborted;
    vscode.window.showErrorMessage(
      "No Quantum Workspaces found in the Azure subscription",
    );
    return;
  }

  // id will be similar to: "/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/quantumResourcegroup/providers/Microsoft.Quantum/Workspaces/quantumworkspace1"
  // endpointUri will be like: "https://quantumworkspace1.westus.quantum.azure.com" (but first segment should be removed)

  // Quick-pick if more than one
  let workspace = workspaces.value[0];
  if (workspaces.value.length > 1) {
    const pickItems = workspaces.value.map((worksp) => ({
      label: worksp.name,
      detail: worksp.id,
      selection: worksp,
    }));
    const choice = await vscode.window.showQuickPick(pickItems, {
      title: "Select a workspace",
    });
    if (!choice) {
      endEventProperties.reason = "aborted workspace selection";
      endEventProperties.flowStatus = UserFlowStatus.Aborted;
      return;
    }
    workspace = choice.selection;
  }
  return workspace;
}

async function chooseSubscriptionFromTenant(
  tenantToken: string,
  endEventProperties: EndEventProperties,
) {
  const subs: ResponseTypes.SubscriptionList = await azureRequest(
    AzureUris.subscriptions(),
    tenantToken,
    endEventProperties.associationId,
  );
  log.trace(`Got subscriptions: ${JSON.stringify(subs, null, 2)}`);
  if (!subs?.value?.length) {
    log.info("No subscriptions returned for the AAD account and tenant");
    vscode.window.showErrorMessage(
      "No Azure subscriptions found for the account and tenant",
    );
    endEventProperties.reason = "no subscriptions found";
    endEventProperties.flowStatus = UserFlowStatus.Failed;
    return;
  }

  // Quick-pick if more than one
  let subId = subs.value[0].subscriptionId;
  if (subs.value.length > 1) {
    const pickItems = subs.value.map((sub) => ({
      label: sub.displayName,
      detail: sub.subscriptionId,
    }));
    const choice = await vscode.window.showQuickPick(pickItems, {
      title: "Select a subscription",
    });
    if (!choice) {
      // User probably cancelled
      endEventProperties.reason = "aborted subscription choice";
      endEventProperties.flowStatus = UserFlowStatus.Aborted;
      return;
    }
    subId = choice.detail;
  }
  return subId;
}

async function getWorkspaceWithAzureAD(
  endEventProperties: EndEventProperties,
): Promise<WorkspaceConnection | undefined> {
  // *** Authenticate and retrieve tenants the user has Azure resources for ***
  const tenantAuth = await getTenantIdAndToken(endEventProperties);
  if (!tenantAuth) return;

  const subscriptionId = await chooseSubscriptionFromTenant(
    tenantAuth.tenantToken,
    endEventProperties,
  );
  if (!subscriptionId) return;

  const workspace = await chooseWorkspaceFromSubscription(
    subscriptionId,
    tenantAuth.tenantToken,
    endEventProperties,
  );
  if (!workspace) return;

  // Need to remove the first part of the endpoint
  const fixedEndpoint =
    workspace.properties.endpointUri?.replace(
      `https://${workspace.name}.`,
      "https://",
    ) || "";

  const result: WorkspaceConnection = {
    id: workspace.id,
    name: workspace.name,
    endpointUri: fixedEndpoint,
    tenantId: tenantAuth.tenantId,
    providers: [], // Providers and jobs will be populated by a following 'queryWorkspace' call
    jobs: [],
  };
  if (log.getLogLevel() >= 5) {
    log.trace(`Workspace object: ${JSON.stringify(result, null, 2)}`);
  }

  return result;
}

// Reference for existing queries in Python SDK and Azure schema:
// - https://github.com/microsoft/qdk-python/blob/main/azure-quantum/azure/quantum/_client/aio/operations/_operations.py
// - https://github.com/Azure/azure-rest-api-specs/blob/main/specification/quantum/data-plane/Microsoft.Quantum/preview/2022-09-12-preview/quantum.json
export async function queryWorkspace(workspace: WorkspaceConnection) {
  const start = performance.now();
  const token = await getTokenForWorkspace(workspace);

  const associationId = getRandomGuid();
  sendTelemetryEvent(EventType.QueryWorkspaceStart, { associationId }, {});

  const quantumUris = new QuantumUris(workspace.endpointUri, workspace.id);

  const providerStatus: ResponseTypes.ProviderStatusList = await azureRequest(
    quantumUris.providerStatus(),
    token,
    associationId,
  );
  if (log.getLogLevel() >= 5) {
    log.trace(
      `Got provider status: ${JSON.stringify(providerStatus, null, 2)}`,
    );
  }

  // Update the providers with the target list
  workspace.providers = providerStatus.value.map((provider) => {
    return {
      providerId: provider.id,
      currentAvailability: provider.currentAvailability,
      targets: provider.targets
        .map((target) => ({ ...target, providerId: provider.id }))
        .filter((target) => !shouldExcludeTarget(target.id)),
    };
  });

  workspace.providers = workspace.providers.filter(
    (provider) => !shouldExcludeProvider(provider.providerId),
  );

  log.debug("Fetching the jobs for the workspace");
  const jobs: ResponseTypes.JobList = await azureRequest(
    quantumUris.jobs(),
    token,
    associationId,
  );
  log.debug(`Query returned ${jobs.value.length} jobs`);

  if (log.getLogLevel() >= 5) {
    log.trace(`Got jobs: ${JSON.stringify(jobs, null, 2)}`);
  }

  if (jobs.nextLink) {
    log.error("Jobs returned a nextLink. This is not supported yet.");
  }

  if (jobs.value.length === 0) {
    sendTelemetryEvent(
      EventType.QueryWorkspaceEnd,
      {
        associationId,
        reason: "no jobs returned",
        flowStatus: UserFlowStatus.Aborted,
      },
      { timeToCompleteMs: performance.now() - start },
    );
    return;
  }

  // Sort by creation time from newest to oldest
  workspace.jobs = jobs.value
    .sort((a, b) => (a.creationTime < b.creationTime ? 1 : -1))
    .map(responseJobToJob);

  sendTelemetryEvent(
    EventType.QueryWorkspaceEnd,
    { associationId, flowStatus: UserFlowStatus.Succeeded },
    { timeToCompleteMs: performance.now() - start },
  );

  return;
}

// Drop properties we don't care to track and clean up the types.
function responseJobToJob(job: ResponseTypes.Job): Job {
  return {
    id: job.id,
    name: job.name,
    target: job.target,
    status: job.status,
    outputDataUri: job.outputDataUri,
    count: numberOrUndefined(job.inputParams.count),
    shots: numberOrUndefined(job.inputParams.shots),
    creationTime: job.creationTime,
    beginExecutionTime: job.beginExecutionTime,
    endExecutionTime: job.endExecutionTime,
    cancellationTime: job.cancellationTime,
    costEstimate: job.costEstimate,
    errorData: job.errorData,
  };
}

function numberOrUndefined(i: unknown): number | undefined {
  if (typeof i === "string") {
    const c = parseInt(i);
    return isNaN(c) ? undefined : c;
  } else if (typeof i === "number") {
    return i;
  }
  return undefined;
}

export async function getJobFiles(
  containerName: string,
  blobName: string,
  token: string,
  quantumUris: QuantumUris,
): Promise<string> {
  const start = performance.now();
  const associationId = getRandomGuid();
  log.debug(`Fetching job file from ${containerName}/${blobName}`);
  sendTelemetryEvent(EventType.GetJobFilesStart, { associationId }, {});

  const body = JSON.stringify({ containerName, blobName });
  const sasResponse: ResponseTypes.SasUri = await azureRequest(
    quantumUris.sasUri(),
    token,
    associationId,
    "POST",
    body,
  );
  const sasUri = decodeURI(sasResponse.sasUri);
  log.trace(`Got SAS URI: ${sasUri}`);

  const file = await storageRequest(
    sasUri,
    "GET",
    token,
    quantumUris.storageProxy(),
  );

  if (!file) {
    sendTelemetryEvent(
      EventType.GetJobFilesEnd,
      {
        associationId,
        reason: "no files returned",
        flowStatus: UserFlowStatus.Aborted,
      },
      { timeToCompleteMs: performance.now() - start },
    );
    throw new Error("Storage service did not return a file.");
  }

  const blob = await file.text();
  sendTelemetryEvent(
    EventType.GetJobFilesEnd,
    { associationId, flowStatus: UserFlowStatus.Succeeded },
    { timeToCompleteMs: performance.now() - start },
  );
  return blob;
}

type JobContext = {
  jobId: string;
  storageUris: StorageUris;
  quantumUris: QuantumUris;
  token: string;
};

export async function submitJob(
  workspace: WorkspaceConnection,
  associationId: string,
  qir: string,
  providerId: string,
  target: string,
  jobName: string,
  numberOfShots: number,
): Promise<JobContext> {
  const quantumUris = new QuantumUris(workspace.endpointUri, workspace.id);
  const token = await getTokenForWorkspace(workspace);
  const jobId = getRandomGuid();

  const storageUris = await createStorageContainer(
    jobId,
    quantumUris,
    token,
    associationId,
  );

  await uploadBlob(
    storageUris,
    quantumUris,
    token,
    "inputData",
    qir,
    "text/plain",
    associationId,
  );

  await putJobData(
    quantumUris,
    storageUris,
    jobId,
    jobName,
    associationId,
    token,
    providerId,
    target,
    numberOfShots,
  );

  vscode.window.showInformationMessage(`Job ${jobName} submitted`);

  return { jobId, storageUris, quantumUris, token };
}

async function putJobData(
  quantumUris: QuantumUris,
  storageUris: StorageUris,
  jobId: string,
  jobName: string,
  associationId: string,
  token: string,
  providerId: string,
  target: string,
  numberOfShots: number,
) {
  const putJobUri = quantumUris.jobs(jobId);

  const payload = {
    id: jobId,
    name: jobName,
    providerId,
    target,
    itemType: "Job",
    containerUri: storageUris.containerWithSasToken(),
    inputDataUri: storageUris.blob("inputData"),
    inputDataFormat: "qir.v1",
    outputDataFormat: "microsoft.quantum-results.v2",
    inputParams: {
      entryPoint: "ENTRYPOINT__main",
      arguments: [],
      count: numberOfShots,
      shots: numberOfShots,
    },
  };
  await azureRequest(
    putJobUri,
    token,
    associationId,
    "PUT",
    JSON.stringify(payload),
  );
}

async function createStorageContainer(
  containerName: string,
  quantumUris: QuantumUris,
  token: string,
  associationId: string,
): Promise<StorageUris> {
  // Get a sasUri for the container
  const body = JSON.stringify({ containerName });
  const sasResponse = await azureRequest(
    quantumUris.sasUri(),
    token,
    associationId,
    "POST",
    body,
  );

  const storageUris = new StorageUris(
    decodeURI(sasResponse.sasUri),
    containerName,
  );

  // Create the container
  await storageRequest(
    storageUris.containerPutWithSasToken(),
    "PUT",
    token,
    quantumUris.storageProxy(),
    undefined,
    undefined,
    associationId,
  );

  return storageUris;
}

export async function uploadBlob(
  storageUris: StorageUris,
  quantumUris: QuantumUris,
  token: string,
  blobName: string,
  content: string,
  contentType: string,
  associationId: string,
) {
  await storageRequest(
    storageUris.blobWithSasToken(blobName),
    "PUT",
    token,
    quantumUris.storageProxy(),
    [
      ["x-ms-blob-type", "BlockBlob"],
      ["Content-Type", contentType],
    ],
    content,
    associationId,
  );
}
