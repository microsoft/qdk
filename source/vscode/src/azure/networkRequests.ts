// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import { EventType, getUserAgent, sendTelemetryEvent } from "../telemetry";
import * as vscode from "vscode";

export async function azureRequest(
  uri: string,
  token: string,
  associationId?: string,
  method = "GET",
  body?: string,
) {
  const headers: [string, string][] = [
    ["Content-Type", "application/json"],
    ["x-ms-useragent", getUserAgent()],
  ];

  // For API simpilcity and storage back-compat, any api key is passed via the 'token' field.
  if (token.startsWith("apiKey=")) {
    headers.push(["x-ms-quantum-api-key", token.substring(7)]);
  } else {
    headers.push(["Authorization", `Bearer ${token}`]);
  }

  try {
    log.debug(`Fetching ${uri} with method ${method}`);
    log.trace("Request headers & body: ", headers, body);
    const response = await fetch(uri, {
      headers,
      method,
      body,
    });

    if (!response.ok) {
      log.error("Azure request failed", response);
      if (associationId) {
        sendTelemetryEvent(
          EventType.AzureRequestFailed,
          {
            reason: `request to azure returned code ${response.status}`,
            associationId,
          },
          {},
        );
      }
      throw await getAzureQuantumError(response);
    }

    log.debug(`Got response ${response.status} ${response.statusText}`);
    const result = await response.json();
    log.trace("Response value: ", result);

    return result;
  } catch (e) {
    if (associationId) {
      sendTelemetryEvent(
        EventType.AzureRequestFailed,
        { reason: `request to azure failed to return`, associationId },
        {},
      );
    }
    log.error(`Failed to fetch ${uri}: ${e}`);
    throw new Error(getErrorMessage(e));
  }
}

// Different enough to above to warrant it's own function
export async function storageRequest(
  uri: string,
  method: string,
  token?: string,
  proxy?: string,
  extraHeaders?: [string, string][],
  body?: string | Uint8Array,
  associationId?: string,
) {
  const headers: [string, string][] = [
    ["x-ms-version", "2023-01-03"],
    ["x-ms-date", new Date().toUTCString()],
    ["x-ms-useragent", getUserAgent()],
  ];
  if (token) {
    // For API simpilcity and storage back-compat, any api key is passed via the 'token' field.
    if (token.startsWith("apiKey=")) {
      headers.push(["x-ms-quantum-api-key", token.substring(7)]);
    } else {
      headers.push(["Authorization", `Bearer ${token}`]);
    }
  }

  if (extraHeaders?.length) headers.push(...extraHeaders);
  if (proxy) {
    log.debug(`Setting x-proxy-to header to ${uri}`);
    headers.push(["x-proxy-to", uri]);
    uri = proxy;
  }
  try {
    log.debug(`Fetching ${uri} with method ${method}`);
    const response = await fetch(uri, { method, headers, body });
    if (!response.ok) {
      log.error("Storage request failed", response);
      if (associationId) {
        sendTelemetryEvent(
          EventType.StorageRequestFailed,
          {
            reason: `request to storage on azure returned code ${response.status}`,
            associationId,
          },
          {},
        );
      }
      throw await getAzureStorageError(response);
    }
    log.debug(`Got response ${response.status} ${response.statusText}`);
    return response;
  } catch (e) {
    log.error(`Failed to fetch ${uri}: ${e}`);
    if (associationId) {
      sendTelemetryEvent(
        EventType.StorageRequestFailed,
        {
          reason: `request to storage on azure failed to return`,
          associationId,
        },
        {},
      );
    }
    throw new Error(getErrorMessage(e));
  }
}

class AzureError extends Error {
  constructor(message: string) {
    super(message);
  }
}

async function getAzureQuantumError(response: Response): Promise<AzureError> {
  let error: { code: string; message: string } | undefined = undefined;
  try {
    const json = await response.json();
    // Extract the error data if it conforms to the Azure Quantum error schema defined in
    // https://github.com/Azure/azure-rest-api-specs/blob/957fd518388828b31126417415b04f859b95c586/specification/quantum/data-plane/Microsoft.Quantum/preview/2022-09-12-preview/quantum.json#L1186
    if (json && json.error && json.error.code && json.error.message) {
      error = json.error;
    }
  } catch {
    /* empty */
  }

  let message;
  if (error) {
    message = `Azure Quantum request failed with status ${response.status}.\n${error.code}: ${error.message}`;
  } else {
    message = `Azure Quantum request failed with status ${response.status}.`;
  }
  return new AzureError(message);
}

function getAzureStorageError(response: Response): AzureError {
  // Azure Storage appears to uses headers and xml responses to communicate error data,
  // but we have not seen yet these in practice.
  // https://github.com/Azure/azure-rest-api-specs/blob/eb06c34581dc6f56868ee9cc811a51f0e1a50770/specification/storage/data-plane/Microsoft.BlobStorage/preview/2021-12-02/blob.json#L75C30-L75C30
  return new AzureError(
    `Storage request failed with status ${response.status}.`,
  );
}

// Generate a user friendly error message
function getErrorMessage(e: any): string {
  if (e instanceof AzureError) {
    return e.message;
  } else if (e instanceof Error) {
    return `Request failed: ${e.message}`;
  } else {
    return `Request failed.`;
  }
}

export class AzureUris {
  static readonly apiVersion = "2020-01-01";
  static readonly mgmtEndpoint = "https://management.azure.com";

  static tenants() {
    // https://learn.microsoft.com/en-us/rest/api/resources/tenants/list
    return `${this.mgmtEndpoint}/tenants?api-version=${this.apiVersion}`;
  }

  static subscriptions() {
    // https://learn.microsoft.com/en-us/rest/api/resources/subscriptions/list
    return `${this.mgmtEndpoint}/subscriptions?api-version=${this.apiVersion}`;
  }

  static workspaces(subscriptionId: string) {
    // https://github.com/Azure/azure-rest-api-specs/blob/main/specification/quantum/resource-manager/Microsoft.Quantum/preview/2022-01-10-preview/quantum.json#L221
    return `${this.mgmtEndpoint}/subscriptions/${subscriptionId}/providers/Microsoft.Quantum/workspaces?api-version=2022-01-10-preview`;
  }

  static listKeys(
    subscriptionId: string,
    resourceGroupName: string,
    workspaceName: string,
  ) {
    // https://github.com/Azure/azure-rest-api-specs/blob/main/specification/quantum/resource-manager/Microsoft.Quantum/preview/2023-11-13-preview/quantum.json#L419
    return `${this.mgmtEndpoint}/subscriptions/${subscriptionId}/resourceGroups/${resourceGroupName}/providers/Microsoft.Quantum/workspaces/${workspaceName}/listKeys?api-version=2023-11-13-preview`;
  }
}

export class QuantumUris {
  readonly apiVersion = "2022-09-12-preview";

  constructor(
    public endpoint: string, // e.g. "https://westus.quantum.azure.com"
    public id: string, // e.g. "/subscriptions/00000000-1111-2222-3333-444444444444/resourceGroups/quantumResourcegroup/providers/Microsoft.Quantum/Workspaces/quantumworkspace1"
  ) {}

  quotas() {
    return `${this.endpoint}${this.id}/quotas?api-version=${this.apiVersion}`;
  }

  providerStatus() {
    return `${this.endpoint}${this.id}/providerStatus?api-version=${this.apiVersion}`;
  }

  jobs(jobId?: string) {
    return !jobId
      ? `${this.endpoint}${this.id}/jobs?api-version=${this.apiVersion}`
      : `${this.endpoint}${this.id}/jobs/${jobId}?api-version=${this.apiVersion}`;
  }

  // Needs to POST an application/json payload such as: {"containerName": "job-073064ed-2a47-11ee-b8e7-010101010000","blobName":"outputData"}
  sasUri() {
    return `${this.endpoint}${this.id}/storage/sasUri?api-version=${this.apiVersion}`;
  }

  storageProxy() {
    return `${this.endpoint}${this.id}/storage/proxy?api-version=${this.apiVersion}`;
  }
}

export class StorageUris {
  private storageAccount: string;
  private sasTokenRaw: string;

  constructor(
    private sasUri: string,
    private containerName: string,
  ) {
    // Parse the Uri to get the storage account and sasToken
    const sasUriObj = vscode.Uri.parse(sasUri);
    this.storageAccount = sasUriObj.scheme + "://" + sasUriObj.authority;

    // Get the raw value to append to other query strings
    this.sasTokenRaw = sasUri.substring(sasUri.indexOf("?") + 1);
  }

  blob(blobName: string) {
    return `${this.storageAccount}/${this.containerName}/${blobName}`;
  }

  blobWithSasToken(blobName: string) {
    return `${this.storageAccount}/${this.containerName}/${blobName}?${this.sasTokenRaw}`;
  }

  containerWithSasToken() {
    return this.sasUri;
  }

  containerPutWithSasToken() {
    return `${this.storageAccount}/${this.containerName}?restype=container&${this.sasTokenRaw}`;
  }
}

// Put all the Response types in a namespace for easy importing

// eslint-disable-next-line @typescript-eslint/no-namespace
export namespace ResponseTypes {
  export type Tenant = {
    id: string;
    tenantId: string;
    displayName: string;
  };

  export type TenantList = {
    value: Array<Tenant>;
  };

  export type Subscription = {
    id: string;
    subscriptionId: string;
    tenantId: string;
    displayName: string;
  };

  export type SubscriptionList = {
    value: Array<Subscription>;
  };

  export type Provider = {
    providerId: string;
    providerSku: string;
    provisioningState: string; // e.g. 'Succeeded'
    resourceUsageId: string;
  };

  export type Workspace = {
    id: string;
    name: string;
    location: string;
    properties: {
      providers: Array<Provider>;
      provisioningState: string;
      storageAccount: string; // e.g. "/subscriptions/<guid>/resourceGroups/<name>/providers/Microsoft.Storage/storageAccounts/<id>"
      endpointUri: string; // e.g. "https://<workspace-name>.westus.quantum.azure.com". Note: workspace-name should be removed.
    };
  };

  export type WorkspaceList = {
    value: Array<Workspace>;
  };

  export type Quota = {
    scope: string;
    providerId: string;
    period: string;
    holds: number;
    utilization: number;
    limit: number;
  };

  export type QuotaList = {
    nextLink: string | null;
    value: Array<Quota>;
  };

  export type Target = {
    id: string;
    currentAvailability: "Available" | "Degraded" | "Unavailable";
    averageQueueTime: number; // minutes
    statusPage: string; // url
  };

  export type ProviderStatus = {
    id: string;
    currentAvailability: "Available" | "Degraded" | "Unavailable";
    targets: Array<Target>;
  };

  export type ProviderStatusList = {
    nextLink: string | null;
    value: Array<ProviderStatus>;
  };

  export type Job = {
    id: string;
    name: string;
    target: string;
    containerUri: string;
    inputDataUri: string;
    outputDataUri: string;
    inputDataFormat: string;
    outputDataFormat: string;
    inputParams: any;
    status: "Waiting" | "Executing" | "Succeeded" | "Failed" | "Cancelled";
    creationTime: string;
    beginExecutionTime: string;
    endExecutionTime: string;
    cancellationTime?: string;
    costEstimate?: any;
    errorData?: any;
  };

  export type JobList = {
    nextLink: string | null;
    value: Array<Job>;
  };

  export type SasUri = {
    sasUri: string;
  };
}
