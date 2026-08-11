// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { EXTENSION_ID, PythonEnvironments } from "@vscode/python-environments";
import type {
  PythonEnvironmentApi,
  PythonEnvironment,
} from "@vscode/python-environments";
import { CopilotToolError } from "./gh-copilot/types.js";

const pythonEnvsNotInstalledMsg = `The Python Environments extension (${EXTENSION_ID}) is not installed or is disabled.`;

// Fixed set for the Copilot tool.
const toolPackages = ["qdk[jupyter]", "ipympl", "ipykernel"];

// All packages offered in the command palette picker.
const packagePickItems: vscode.QuickPickItem[] = [
  {
    label: "qdk",
    description: "Quantum Development Kit (core)",
    detail: "Compile, simulate, and estimate resources for quantum programs",
    picked: true,
  },
  {
    label: "qdk[azure]",
    description: "QDK optional support for Azure Quantum",
    detail: "Submit jobs to Azure Quantum hardware and cloud simulators",
    picked: false,
  },
  {
    label: "qdk[cirq]",
    description: "QDK optional support for Cirq",
    detail: "Interop with Cirq via qdk.cirq",
    picked: false,
  },
  {
    label: "qdk[jupyter]",
    description: "QDK optional support for Jupyter notebooks",
    detail: "Enable magic commands and rich output in Jupyter notebooks",
    picked: true,
  },
  {
    label: "qdk[qiskit]",
    description: "QDK optional support for Qiskit",
    detail: "Interop with Qiskit via qdk.cirq",
    picked: false,
  },
  {
    label: "qdk-chemistry",
    description: "Microsoft Quantum Development Kit for Chemistry",
    detail: "End-to-end toolkit for quantum chemistry",
    picked: false,
  },
  {
    label: "ipykernel",
    description: "Jupyter kernel",
    detail: "Enable Jupyter notebook functionality in VS Code",
    picked: true,
  },
  {
    label: "ipympl",
    description: "Interactive Matplotlib widgets",
    detail: "Enable interactive plots in Jupyter notebooks",
    picked: true,
  },
];

// Merge selected qdk extras (e.g. qdk + qdk[azure] + qdk[jupyter]) into one specifier.
function coalesceQdkExtras(packages: string[]): string[] {
  const extras: string[] = [];
  const rest: string[] = [];
  let hasQdk = false;
  for (const pkg of packages) {
    if (pkg === "qdk") {
      hasQdk = true;
    } else if (pkg.startsWith("qdk[") && pkg.endsWith("]")) {
      hasQdk = true;
      extras.push(...pkg.slice(4, -1).split(","));
    } else {
      rest.push(pkg);
    }
  }
  if (hasQdk) {
    rest.unshift(extras.length > 0 ? `qdk[${extras.join(",")}]` : "qdk");
  }
  return rest;
}

async function getPythonEnvsApi(): Promise<PythonEnvironmentApi | undefined> {
  try {
    return await PythonEnvironments.api();
  } catch {
    return undefined;
  }
}

function isUnderWorkspaceRoot(
  env: PythonEnvironment,
  root: vscode.Uri,
): boolean {
  const envStr = env.environmentPath.toString();
  const rootStr = root.toString();
  return envStr === rootStr || envStr.startsWith(rootStr + "/");
}

function getActiveWorkspaceRoot(): vscode.Uri | undefined {
  const activeUri = vscode.window.activeTextEditor?.document.uri;
  const fromEditor = activeUri
    ? vscode.workspace.getWorkspaceFolder(activeUri)
    : undefined;
  return (fromEditor ?? vscode.workspace.workspaceFolders?.[0])?.uri;
}

export async function findWorkspaceVenv(): Promise<boolean> {
  const api = await getPythonEnvsApi();
  if (!api) return false;
  const root = getActiveWorkspaceRoot();
  if (!root) return false;
  await api.refreshEnvironments(root);
  const envs = await api.getEnvironments(root);
  return envs.some((env) => isUnderWorkspaceRoot(env, root));
}

export async function createQuantumVenv(): Promise<{ action: string }> {
  const api = await getPythonEnvsApi();
  if (!api) {
    throw new CopilotToolError(pythonEnvsNotInstalledMsg);
  }

  const root = getActiveWorkspaceRoot();
  if (!root) {
    throw new CopilotToolError("No workspace folder is open.");
  }

  await api.refreshEnvironments(root);
  const existingEnvs = await api.getEnvironments(root);
  const existingEnv = existingEnvs.find((env) =>
    isUnderWorkspaceRoot(env, root),
  );

  if (existingEnv) {
    try {
      await api.managePackages(existingEnv, {
        install: toolPackages,
        upgrade: true,
      });
    } catch (e: any) {
      throw new CopilotToolError(
        `Failed to install packages: ${e.message ?? e}`,
      );
    }
    return { action: "updated" };
  }

  let env;
  try {
    // Quick create so the user isn't prompted
    env = await api.createEnvironment(root, { quickCreate: true });
  } catch (e: any) {
    throw new CopilotToolError(
      `Failed to create environment: ${e.message ?? e}`,
    );
  }
  if (!env) {
    throw new CopilotToolError("Environment creation was cancelled or failed.");
  }
  try {
    await api.managePackages(env, { install: toolPackages });
  } catch (e: any) {
    throw new CopilotToolError(`Failed to install packages: ${e.message ?? e}`);
  }
  return { action: "created" };
}

// Command palette handler — includes interactive prompt for existing venvs.
export async function createQuantumVenvCommand(): Promise<void> {
  const api = await getPythonEnvsApi();
  if (!api) {
    vscode.window.showErrorMessage(pythonEnvsNotInstalledMsg);
    return;
  }

  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    vscode.window.showErrorMessage("No workspace folder is open.");
    return;
  }
  let root: vscode.Uri;
  if (folders.length === 1) {
    root = folders[0].uri;
  } else {
    const picked = await vscode.window.showWorkspaceFolderPick({
      placeHolder: "Select the workspace folder for the virtual environment",
    });
    if (!picked) return;
    root = picked.uri;
  }

  await api.refreshEnvironments(root);
  const existingEnvs = await api.getEnvironments(root);
  const existingEnv = existingEnvs.find((env) =>
    isUnderWorkspaceRoot(env, root),
  );

  if (existingEnv) {
    const choice = await vscode.window.showQuickPick(
      ["Update existing environment", "Cancel"],
      {
        placeHolder: "A virtual environment already exists at this workspace.",
      },
    );
    if (choice !== "Update existing environment") return;
  }

  const selected = await vscode.window.showQuickPick(
    packagePickItems.map((item) => ({ ...item })),
    {
      canPickMany: true,
      placeHolder: "Select packages to install",
    },
  );
  if (!selected || selected.length === 0) return;

  const install = coalesceQdkExtras(selected.map((item) => item.label));

  try {
    if (existingEnv) {
      await api.managePackages(existingEnv, { install, upgrade: true });
      vscode.window.showInformationMessage(
        "Quantum notebook packages updated in existing environment.",
      );
      return;
    }

    // Quick create so the user isn't prompted
    const env = await api.createEnvironment(root, { quickCreate: true });
    if (!env) {
      log.warn(
        `Failed to create a Python environment in ${root.fsPath}. ` +
          `Ensure the Python Environments extension has a registered environment manager.`,
      );
      return;
    }
    await api.managePackages(env, { install });
    vscode.window.showInformationMessage(
      "Quantum notebook virtual environment created.",
    );
  } catch (e: any) {
    vscode.window.showErrorMessage(
      `Failed to set up environment: ${e.message ?? e}`,
    );
  }
}
