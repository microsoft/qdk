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

/**
 * A package offered in the venv picker. `label` is display text only; the
 * requirement passed to pip is built from the fields below, so a version
 * constraint never leaks into the UI.
 */
interface PackagePickItem extends vscode.QuickPickItem {
  /** Distribution name, e.g. `qdk-chemistry`. */
  packageName: string;
  /** Extras to request, e.g. `["jupyter"]`. */
  extras?: string[];
  /** Version constraint, e.g. `>=6.0,<7`. */
  versionSpecifier?: string;
}

// All packages offered in the command palette picker (in display order)
const packagePickItems: PackagePickItem[] = [
  {
    label: "qdk",
    packageName: "qdk",
    description: "Quantum Development Kit (core)",
    detail: "Compile, simulate, and estimate resources for quantum programs",
    picked: true,
  },
  {
    label: "qdk[azure]",
    packageName: "qdk",
    extras: ["azure"],
    description: "QDK optional support for Azure Quantum",
    detail: "Submit jobs to Azure Quantum hardware and cloud simulators",
    picked: false,
  },
  {
    label: "qdk[cirq]",
    packageName: "qdk",
    extras: ["cirq"],
    description: "QDK optional support for Cirq",
    detail: "Interop with Cirq via qdk.cirq",
    picked: false,
  },
  {
    label: "qdk[jupyter]",
    packageName: "qdk",
    extras: ["jupyter"],
    description: "QDK optional support for Jupyter notebooks",
    detail:
      "Enable Q# code cells and interactive quantum widgets in Jupyter notebooks",
    picked: true,
  },
  {
    label: "qdk[qiskit]",
    packageName: "qdk",
    extras: ["qiskit"],
    description: "QDK optional support for Qiskit",
    detail: "Interop with Qiskit via qdk.qiskit",
    picked: false,
  },
  {
    label: "qdk-chemistry",
    packageName: "qdk-chemistry",
    description: "Microsoft Quantum Development Kit for Chemistry (core)",
    detail: "Chemistry library only, without the notebook or PySCF plugins",
    picked: false,
  },
  {
    // The `jupyter` extra pulls in `plugins`, which is where qdk-chemistry
    // bounds pyscf. Without it pyscf would be left unconstrained.
    label: "qdk-chemistry[jupyter]",
    packageName: "qdk-chemistry",
    extras: ["jupyter"],
    description: "QDK/Chemistry optional support for Jupyter notebooks",
    detail:
      "Add the notebook and simulation plugins, including PySCF. Required by the chemistry course.",
    picked: false,
  },
  {
    label: "ipykernel",
    packageName: "ipykernel",
    // Pinned to 6.x: ipykernel 7 can leave notebooks hanging on the first cell.
    // Remove once https://github.com/microsoft/qdk/issues/3662 is fixed.
    versionSpecifier: ">=6.0,<7",
    description: "Jupyter kernel",
    detail: "Enable Jupyter notebook functionality in VS Code",
    picked: true,
  },
  {
    label: "ipympl",
    packageName: "ipympl",
    description: "Interactive Matplotlib widgets",
    detail: "Enable interactive plots in Jupyter notebooks",
    picked: true,
  },
];

/**
 * Build the pip requirements for the selected items, merging selections that
 * name the same package so ticking `qdk` and `qdk[jupyter]` installs
 * `qdk[jupyter]` rather than passing both. Selection order is preserved.
 */
function toRequirements(selected: readonly PackagePickItem[]): string[] {
  const order: string[] = [];
  const byName = new Map<
    string,
    { extras: string[]; versionSpecifiers: string[] }
  >();

  for (const item of selected) {
    let merged = byName.get(item.packageName);
    if (!merged) {
      merged = { extras: [], versionSpecifiers: [] };
      byName.set(item.packageName, merged);
      order.push(item.packageName);
    }
    for (const extra of item.extras ?? []) {
      if (!merged.extras.includes(extra)) {
        merged.extras.push(extra);
      }
    }
    // Constraints from every selected row are kept, so pinning two rows of the
    // same package narrows the range instead of silently dropping one.
    if (
      item.versionSpecifier &&
      !merged.versionSpecifiers.includes(item.versionSpecifier)
    ) {
      merged.versionSpecifiers.push(item.versionSpecifier);
    }
  }

  return order.map((name) => {
    const { extras, versionSpecifiers } = byName.get(name)!;
    const extrasPart = extras.length > 0 ? `[${extras.join(",")}]` : "";
    return `${name}${extrasPart}${versionSpecifiers.join(",")}`;
  });
}

async function getPythonEnvsApi(): Promise<PythonEnvironmentApi | undefined> {
  try {
    return await PythonEnvironments.api();
  } catch {
    return undefined;
  }
}

function isEnvInFolder(env: PythonEnvironment, folder: vscode.Uri): boolean {
  const envStr = vscode.Uri.file(env.sysPrefix).toString();
  const folderStr = folder.toString();
  return envStr.startsWith(folderStr + "/");
}

// Find a venv whose sysPrefix is in the given folder.
// Prefer the active environment if it qualifies.
async function getEnvInFolder(
  api: PythonEnvironmentApi,
  folder: vscode.Uri,
): Promise<PythonEnvironment | undefined> {
  log.trace(`Searching for existing venvs in ${folder.fsPath}`);

  await api.refreshEnvironments(folder);

  // This can return the global environment, for example, so we have to check
  // whether it's actually in the folder.
  const activeEnv = await api.getEnvironment(folder);
  if (activeEnv && isEnvInFolder(activeEnv, folder)) {
    log.trace(`Preferring active venv in ${activeEnv.environmentPath.fsPath}`);
    return activeEnv;
  }

  const allEnvs = await api.getEnvironments(folder);
  const matchingEnvs = allEnvs.filter((env) => isEnvInFolder(env, folder));
  if (matchingEnvs.length == 0) {
    log.trace(`Found no venvs in ${folder.fsPath}`);
    return undefined;
  }

  if (matchingEnvs.length > 1) {
    log.warn(
      `Found multiple venvs in ${folder.fsPath} - using ${matchingEnvs[0].environmentPath}`,
    );
  }

  log.trace(
    `Found existing venv in ${folder.fsPath} - using ${matchingEnvs[0].environmentPath}`,
  );
  return matchingEnvs[0];
}

function getActiveWorkspaceRoot(): vscode.Uri | undefined {
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (!workspaceFolders) {
    return undefined;
  }
  if (workspaceFolders.length == 1) {
    return workspaceFolders[0].uri;
  }

  const activeUri = vscode.window.activeTextEditor?.document.uri;
  const fromEditor = activeUri
    ? vscode.workspace.getWorkspaceFolder(activeUri)
    : undefined;
  const result = (fromEditor ?? workspaceFolders[0])?.uri;
  log.warn(
    `Found multiple workspace roots while searching for existing venvs - preferring ${result.fsPath}${fromEditor ? " based on the active editor" : ""}`,
  );
  return result;
}

export async function getVenvInFolder(
  folder: vscode.Uri,
): Promise<{ id: string; path: string } | undefined> {
  const api = await getPythonEnvsApi();
  if (!api) {
    return undefined;
  }
  const env = await getEnvInFolder(api, folder);
  if (!env || !isEnvInFolder(env, folder)) {
    return undefined;
  }
  return { id: env.envId.id, path: env.environmentPath.fsPath };
}

export async function getExistingQuantumVenv(): Promise<
  vscode.Uri | undefined
> {
  const root = getActiveWorkspaceRoot();
  if (!root) {
    return undefined;
  }
  const info = await getVenvInFolder(root);
  return info ? vscode.Uri.file(info.path) : undefined;
}

export async function createQuantumVenv(): Promise<{ action: string }> {
  const api = await getPythonEnvsApi();
  if (!api) {
    throw new CopilotToolError(pythonEnvsNotInstalledMsg);
  }

  // There's no obvious way to propagate the path from getExistingQuantumVenv to here
  // so just parallel the logic.
  const root = getActiveWorkspaceRoot();
  if (!root) {
    throw new CopilotToolError("No workspace folder is open.");
  }

  // Don't interrupt the chat by showing a picker - just use the defaults
  const selectedPackages = packagePickItems.filter((item) => item.picked);

  const packagesToInstall = toRequirements(selectedPackages);

  const existingEnv = await getEnvInFolder(api, root);
  if (existingEnv) {
    // Copilot should already have confirmed that the user is willing to update
    // the existing workspace
    try {
      await api.managePackages(existingEnv, {
        install: packagesToInstall,
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
    env = await api.createEnvironment(root, {
      quickCreate: true,
      additionalPackages: packagesToInstall,
    });
  } catch (e: any) {
    throw new CopilotToolError(
      `Failed to create environment: ${e.message ?? e}`,
    );
  }
  if (!env) {
    throw new CopilotToolError("Environment creation was cancelled or failed.");
  }
  return { action: "created" };
}

// Command palette handler — includes interactive prompt for existing venvs.
export async function createQuantumVenvForCommand(): Promise<void> {
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
    if (!picked) {
      return;
    }
    root = picked.uri;
  }

  const existingEnv = await getEnvInFolder(api, root);
  if (existingEnv) {
    const choice = await vscode.window.showQuickPick(
      ["Update existing environment", "Cancel"],
      {
        placeHolder: "A virtual environment already exists at this workspace.",
      },
    );
    if (choice !== "Update existing environment") {
      return;
    }
  }

  const selectedPackages = await vscode.window.showQuickPick(
    packagePickItems.map((item) => ({ ...item })),
    {
      canPickMany: true,
      placeHolder: "Select packages to install",
    },
  );
  if (!selectedPackages || selectedPackages.length === 0) {
    return;
  }

  const packagesToInstall = toRequirements(selectedPackages);

  try {
    if (existingEnv) {
      await api.managePackages(existingEnv, {
        install: packagesToInstall,
        upgrade: true,
      });
      vscode.window.showInformationMessage(
        "Quantum notebook packages updated in existing environment.",
      );
      return;
    }

    // Quick create so the user isn't prompted
    const env = await api.createEnvironment(root, {
      quickCreate: true,
      additionalPackages: packagesToInstall,
    });
    if (!env) {
      log.warn(
        `Failed to create a Python environment in ${root.fsPath}. ` +
          `Ensure the Python Environments extension has a registered environment manager.`,
      );
      return;
    }
    vscode.window.showInformationMessage(
      "Quantum notebook virtual environment created.",
    );
  } catch (e: any) {
    vscode.window.showErrorMessage(
      `Failed to set up environment: ${e.message ?? e}`,
    );
  }
}
