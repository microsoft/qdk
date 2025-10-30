// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { queryWorkspace } from "./workspaceActions";
import { log } from "qsharp-lang";
import { targetSupportQir } from "./providerProperties";

// See docs at https://code.visualstudio.com/api/extension-guides/tree-view

const pendingStatuses = ["Waiting", "Executing", "Finishing"];
const noQirMsq = `Note: As this target does not currently support QIR, this VS Code extension cannot submit jobs to it. See https://aka.ms/qdk.qir for more info`;

// Convert a date such as "2023-07-24T17:25:09.1309979Z" into local time
function localDate(date: string) {
  return new Date(date).toLocaleString();
}

export class WorkspaceTreeProvider
  implements vscode.TreeDataProvider<WorkspaceTreeItem>
{
  static instance: WorkspaceTreeProvider;
  private treeState: Map<string, WorkspaceConnection> = new Map();

  private didChangeTreeDataEmitter = new vscode.EventEmitter<
    WorkspaceTreeItem | undefined
  >();

  readonly onDidChangeTreeData: vscode.Event<WorkspaceTreeItem | undefined> =
    this.didChangeTreeDataEmitter.event;

  updateWorkspace(workspace: WorkspaceConnection) {
    this.treeState.set(workspace.id, workspace);
    this.didChangeTreeDataEmitter.fire(undefined);
  }

  removeWorkspace(workspaceId: string) {
    this.treeState.delete(workspaceId);
    this.didChangeTreeDataEmitter.fire(undefined);
  }

  async refreshWorkspace(workspace: WorkspaceConnection) {
    log.debug("In refreshWorkspace for workspace: ", workspace.id);
    await queryWorkspace(workspace);
    this.updateWorkspace(workspace);
  }

  getWorkspaceIds() {
    return [...this.treeState.keys()];
  }

  getWorkspace(workspaceId: string) {
    return this.treeState.get(workspaceId);
  }

  hasWorkspace(workspaceId: string) {
    return this.treeState.has(workspaceId);
  }

  workspaceHasJob(workspaceId: string, jobId: string): boolean {
    const workspace = this.getWorkspace(workspaceId);
    if (!workspace) return false;

    return workspace.jobs.some((job) => job.id === jobId);
  }

  hasJobsPending(workspaceId: string): boolean {
    const workspace = this.getWorkspace(workspaceId);
    if (!workspace) return false;

    return workspace.jobs.some((job) => pendingStatuses.includes(job.status));
  }

  getTreeItem(
    element: WorkspaceTreeItem,
  ): vscode.TreeItem | Thenable<vscode.TreeItem> {
    return element;
  }

  getChildren(
    element?: WorkspaceTreeItem | undefined,
  ): vscode.ProviderResult<WorkspaceTreeItem[]> {
    if (!element) {
      return [...this.treeState.values()].map(
        (workspace) =>
          new WorkspaceTreeItem(
            workspace.name,
            workspace,
            "workspace",
            workspace,
          ),
      );
    } else {
      return element.getChildren();
    }
  }
}

export type WorkspaceConnection = {
  id: string;
  name: string;
  endpointUri: string;
  tenantId: string;
  apiKey?: string;
  providers: Provider[];
  jobs: Job[];
  lastRequestError?: string;
};

export type Provider = {
  providerId: string;
  currentAvailability: "Available" | "Degraded" | "Unavailable";
  targets: Target[];
};

export type Target = {
  id: string;
  providerId: string;
  currentAvailability: "Available" | "Degraded" | "Unavailable";
  averageQueueTime: number; // minutes
};

export type Job = {
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
  outputDataUri?: string;
  count?: number;
  shots?: number;
  creationTime: string;
  beginExecutionTime?: string;
  endExecutionTime?: string;
  cancellationTime?: string;
  costEstimate?: any;
  errorData?: { code: string; message: string };
};

function shouldShowQueueTime(target: Target) {
  return (
    target.currentAvailability !== "Unavailable" &&
    typeof target.averageQueueTime === "number"
  ); // Could be 0 seconds
}

function getQueueTimeText(seconds: number): string {
  // If the queue time is less than 2 minute, show it in seconds
  if (seconds < 120) {
    return `${seconds} seconds`;
  } else if (seconds < 60 * 60 * 4) {
    // Otherwise, show it in minutes if less than 4 hours
    return `${Math.round(seconds / 60)} minutes`;
  } else {
    // Otherwise, show it in hours
    return `${Math.round(seconds / (60 * 60))} hours`;
  }
}

// A workspace has an array in properties.providers, each of which has a 'providerId' property,
// e.g. 'ionq', and a 'provisioningState' property, e.g. 'Succeeded'. Filter the list to only
// include those that have succeeded. Then, when querying the providerStatus, only add the targets
// for the providers that are present. (Also, filter out providers that have no targets).

export class WorkspaceTreeItem extends vscode.TreeItem {
  constructor(
    label: string,
    public workspace: WorkspaceConnection,
    public type:
      | "workspace"
      | "providerHeader"
      | "provider"
      | "target"
      | "jobHeader"
      | "job",
    public itemData:
      | WorkspaceConnection
      | Provider[]
      | Provider
      | Target[]
      | Target
      | Job[]
      | Job,
  ) {
    super(label, vscode.TreeItemCollapsibleState.Collapsed);

    this.contextValue = type;

    switch (type) {
      case "workspace":
        if (!this.workspace.lastRequestError) {
          this.iconPath = new vscode.ThemeIcon("notebook");
        } else {
          this.iconPath = new vscode.ThemeIcon("alert");
          this.tooltip = this.workspace.lastRequestError;
        }
        break;
      case "providerHeader": {
        break;
      }
      case "provider": {
        this.iconPath = new vscode.ThemeIcon("layers");
        break;
      }
      case "target": {
        const target = itemData as Target;
        const supportsQir = targetSupportQir(target.id);

        if (supportsQir) {
          this.contextValue = "qir-target";
        }

        this.iconPath = new vscode.ThemeIcon("package");
        this.collapsibleState = vscode.TreeItemCollapsibleState.None;
        if (target.currentAvailability || target.averageQueueTime) {
          const hover = new vscode.MarkdownString(
            `${
              target.currentAvailability
                ? `__Status__: ${target.currentAvailability}<br>`
                : ""
            }
            ${
              shouldShowQueueTime(target)
                ? `__Queue time__: ${getQueueTimeText(
                    target.averageQueueTime,
                  )}<br>`
                : ""
            }
            ${supportsQir ? "" : "\n" + noQirMsq}`,
          );
          hover.supportHtml = true;
          this.tooltip = hover;
        }
        break;
      }
      case "job": {
        const job = itemData as Job;
        this.collapsibleState = vscode.TreeItemCollapsibleState.None;
        switch (job.status) {
          case "Executing":
            this.contextValue = "job-cancelable";
          // falls through
          case "Finishing":
            this.iconPath = new vscode.ThemeIcon("run-all");
            break;
          case "Waiting":
            this.iconPath = new vscode.ThemeIcon("loading~spin");
            this.contextValue = "job-cancelable";
            break;
          case "Cancelled":
            this.iconPath = new vscode.ThemeIcon("circle-slash");
            this.contextValue = "result";
            break;
          case "Failed":
            this.iconPath = new vscode.ThemeIcon("error");
            this.contextValue = "result";
            break;
          case "Succeeded":
            this.iconPath = new vscode.ThemeIcon("pass");
            this.contextValue = "result-download";
            break;
        }
        // Tooltip
        const md = `__Created__: ${localDate(job.creationTime)}<br>
__Target__: ${job.target}<br>
__Status__: ${job.status}<br>
${
  job.beginExecutionTime
    ? `__Started__: ${localDate(job.beginExecutionTime)}<br>`
    : ""
}
${
  job.endExecutionTime
    ? `__Completed__: ${localDate(job.endExecutionTime)}<br>`
    : ""
}
${
  job.errorData
    ? `__Error__: <br>\n\`\`\`\n${job.errorData.code}: ${job.errorData.message}\n\`\`\`\n<br>`
    : ""
}
`;
        const hover = new vscode.MarkdownString(md);
        hover.supportHtml = true;
        this.tooltip = hover;
        break;
      }

      default:
        break;
    }
  }

  getChildren(): WorkspaceTreeItem[] {
    switch (this.type) {
      case "workspace":
        return [
          new WorkspaceTreeItem(
            "Providers",
            this.workspace,
            "providerHeader",
            this.workspace.providers,
          ),
          new WorkspaceTreeItem(
            "Jobs",
            this.workspace,
            "jobHeader",
            this.workspace.jobs,
          ),
        ];

      case "providerHeader":
        return (this.itemData as Provider[]).map(
          (provider) =>
            new WorkspaceTreeItem(
              provider.providerId,
              this.workspace,
              "provider",
              provider,
            ),
        );
      case "provider":
        return (this.itemData as Provider).targets.map(
          (target) =>
            new WorkspaceTreeItem(target.id, this.workspace, "target", target),
        );
      case "jobHeader":
        return (this.itemData as Job[]).map(
          (job) =>
            new WorkspaceTreeItem(
              job.name || job.id,
              this.workspace,
              "job",
              job,
            ),
        );
      case "target":
      case "job":
      default:
        return [];
    }
  }
}
