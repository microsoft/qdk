// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { queryWorkspace } from "./workspaceActions";
import { log } from "qsharp-lang";

// See docs at https://code.visualstudio.com/api/extension-guides/tree-view

// Convert a date such as "2023-07-24T17:25:09.1309979Z" into local time
function localDate(date: string) {
  return new Date(date).toLocaleString();
}

export class WorkspaceTreeProvider
  implements vscode.TreeDataProvider<WorkspaceTreeItem>
{
  private treeState: Map<string, WorkspaceConnection> = new Map();

  private didChangeTreeDataEmitter = new vscode.EventEmitter<
    WorkspaceTreeItem | undefined
  >();
  readonly onDidChangeTreeData: vscode.Event<WorkspaceTreeItem | undefined> =
    this.didChangeTreeDataEmitter.event;

  updateWorkspace(workspace: WorkspaceConnection) {
    this.treeState.set(workspace.id, workspace);
  }

  async refresh() {
    log.debug("Refreshing workspace tree");

    const workspaces = [...this.treeState.values()];

    for (const workspace of workspaces) {
      await queryWorkspace(workspace).then(() =>
        this.updateWorkspace(workspace)
      );
    }

    this.didChangeTreeDataEmitter.fire(undefined);
  }

  getTreeItem(
    element: WorkspaceTreeItem
  ): vscode.TreeItem | Thenable<vscode.TreeItem> {
    return element;
  }

  getChildren(
    element?: WorkspaceTreeItem | undefined
  ): vscode.ProviderResult<WorkspaceTreeItem[]> {
    if (!element) {
      return [...this.treeState.values()].map(
        (workspace) =>
          new WorkspaceTreeItem(
            workspace.name,
            workspace,
            "workspace",
            workspace
          )
      );
    } else {
      return element.getChildren();
    }
  }
}

export type WorkspaceConnection = {
  connection: any;
  id: string;
  name: string;
  storageAccount: string;
  endpointUri: string;
  tenantId: string;
  quota?: any;
  providers: Provider[];
  jobs: Job[];
};

export type Provider = {
  providerId: string;
  currentAvailability: "Available" | "Degraded" | "Unavailable";
  targets: Target[];
};

export type Target = {
  id: string;
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
  creationTime: string;
  beginExecutionTime?: string;
  endExecutionTime?: string;
  cancellationTime?: string;
  costEstimate?: any;
};

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
      | Job
  ) {
    super(label, vscode.TreeItemCollapsibleState.Collapsed);

    this.contextValue = type;

    switch (type) {
      case "workspace":
        this.iconPath = new vscode.ThemeIcon("notebook");
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
              target.averageQueueTime
                ? `__Queue time__: ${target.averageQueueTime}mins<br>`
                : ""
            }`
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
          case "Finishing":
            this.iconPath = new vscode.ThemeIcon("run-all");
            break;
          case "Waiting":
            this.iconPath = new vscode.ThemeIcon("loading~spin");
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
            this.contextValue = "result";
            break;
        }
        // Tooltip
        const hover = new vscode.MarkdownString(
          `__Created__: ${localDate(job.creationTime)}<br>
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
            job.costEstimate ? `__Cost estimate__: ${job.costEstimate}<br>` : ""
          }
        `
        );
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
            this.workspace.providers
          ),
          new WorkspaceTreeItem(
            "Jobs",
            this.workspace,
            "jobHeader",
            this.workspace.jobs
          ),
        ];

      case "providerHeader":
        return (this.itemData as Provider[]).map(
          (provider) =>
            new WorkspaceTreeItem(
              provider.providerId,
              this.workspace,
              "provider",
              provider
            )
        );
      case "provider":
        return (this.itemData as Provider).targets.map(
          (target) =>
            new WorkspaceTreeItem(target.id, this.workspace, "target", target)
        );
      case "jobHeader":
        return (this.itemData as Job[]).map(
          (job) =>
            new WorkspaceTreeItem(
              job.name || job.id,
              this.workspace,
              "job",
              job
            )
        );
      case "target":
      case "job":
      default:
        return [];
    }
  }
}
