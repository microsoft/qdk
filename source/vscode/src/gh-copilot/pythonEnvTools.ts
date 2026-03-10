// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";

// Minimal type definitions for the Python Environments extension API.
// We only define the subset we consume to avoid a hard dependency.

interface PythonCommandRunConfiguration {
  executable: string;
  args?: string[];
}

interface PythonEnvironmentExecutionInfo {
  run: PythonCommandRunConfiguration;
}

interface PythonEnvironmentId {
  id: string;
  managerId: string;
}

interface PythonEnvironment {
  envId: PythonEnvironmentId;
  name: string;
  displayName: string;
  version: string;
  environmentPath: vscode.Uri;
  execInfo: PythonEnvironmentExecutionInfo;
}

interface PythonPackage {
  name: string;
  displayName: string;
  version?: string;
}

interface PythonEnvironmentApi {
  getEnvironment(scope: vscode.Uri | undefined): Promise<PythonEnvironment | undefined>;
  getPackages(environment: PythonEnvironment): Promise<PythonPackage[] | undefined>;
}

const PYTHON_ENVS_EXTENSION_ID = "ms-python.vscode-python-envs";
const REQUIRED_PACKAGES = ["qdk", "qdk-mcp"];

type InitPythonResult =
  | {
      status: "ready";
      pythonVersion: string;
      environmentName: string;
      environmentPath: string;
      packages: { name: string; version: string | undefined }[];
      message: string;
    }
  | {
      status: "missing-packages";
      pythonVersion?: string;
      environmentName?: string;
      installCommand: string;
      message: string;
    }
  | {
      status: "no-python-extension";
      pythonFound: boolean;
      pythonVersion?: string;
      installCommand: string;
      recommendations: string[];
      message: string;
    };

/**
 * Implements the `qdk-init-python-environment` tool call.
 *
 * Checks for the Python Environments extension, gets the active environment
 * and installed packages, and returns actionable guidance for setting up
 * QDK Python tooling.
 */
export async function initPythonQdkEnvironment(): Promise<InitPythonResult> {
  const pyEnvsExtension =
    vscode.extensions.getExtension<PythonEnvironmentApi>(
      PYTHON_ENVS_EXTENSION_ID,
    );

  if (pyEnvsExtension) {
    return await initWithPythonEnvsExtension(pyEnvsExtension);
  } else {
    return await initWithoutPythonEnvsExtension();
  }
}

async function initWithPythonEnvsExtension(
  extension: vscode.Extension<PythonEnvironmentApi>,
): Promise<InitPythonResult> {
  const api = extension.isActive
    ? extension.exports
    : await extension.activate();

  const environment = await api.getEnvironment(undefined);
  if (!environment) {
    return {
      status: "missing-packages",
      installCommand: `pip install ${REQUIRED_PACKAGES.join(" ")}`,
      message:
        "The Python Environments extension is installed but no Python environment is currently selected for this workspace. " +
        "Please select or create a Python environment, then install the QDK packages.",
    };
  }

  const packages = await api.getPackages(environment);
  const qdkPackages = REQUIRED_PACKAGES.map((name) => {
    const found = packages?.find(
      (p) => p.name.toLowerCase() === name.toLowerCase(),
    );
    return { name, installed: !!found, version: found?.version };
  });

  const allInstalled = qdkPackages.every((p) => p.installed);

  if (allInstalled) {
    // TODO: enable qdk python mcp server passthrough
    // (the node.js mcp server "circuit" would call the python mcp server "circuit"
    //  using the resolved venv)
    return {
      status: "ready",
      pythonVersion: environment.version,
      environmentName: environment.displayName,
      environmentPath: environment.environmentPath.fsPath,
      packages: qdkPackages.map((p) => ({
        name: p.name,
        version: p.version,
      })),
      message:
        "Python QDK environment is ready. " +
        `All required packages (${REQUIRED_PACKAGES.join(", ")}) are installed.`,
    };
  }

  const missing = qdkPackages.filter((p) => !p.installed).map((p) => p.name);
  const pythonExe = environment.execInfo.run.executable;

  return {
    status: "missing-packages",
    pythonVersion: environment.version,
    environmentName: environment.displayName,
    installCommand: `${pythonExe} -m pip install ${REQUIRED_PACKAGES.join(" ")}`,
    message:
      `The following QDK packages are missing: ${missing.join(", ")}. ` +
      `Run the install command to set them up.`,
  };
}

async function initWithoutPythonEnvsExtension(): Promise<InitPythonResult> {
  const pythonInfo = await findPythonOnPath();

  if (pythonInfo) {
    return {
      status: "no-python-extension",
      pythonFound: true,
      pythonVersion: pythonInfo.version,
      installCommand: `${pythonInfo.executable} -m pip install ${REQUIRED_PACKAGES.join(" ")}`,
      recommendations: ["ms-python.python"],
      message:
        `Python ${pythonInfo.version} was found on PATH, ` +
        `but the Python VS Code extension is not installed. ` +
        `Install the recommended extension for a better experience, ` +
        `and run the install command to set up QDK packages.`,
    };
  }

  return {
    status: "no-python-extension",
    pythonFound: false,
    installCommand: `pip install ${REQUIRED_PACKAGES.join(" ")}`,
    recommendations: ["ms-python.python"],
    message:
      "Python was not found on PATH. " +
      "Please install Python from https://www.python.org/downloads/, " +
      "install the recommended VS Code extension, " +
      "and run the install command to set up QDK packages.",
  };
}

async function findPythonOnPath(): Promise<{
  executable: string;
  version: string;
} | null> {
  // child_process is only available in the desktop (Node.js) extension host.
  // In the web extension host this will fail and we'll report Python not found.
  let execFile: typeof import("child_process").execFile;
  try {
    // Use dynamic require to avoid a compile-time dependency on Node types.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    execFile = require("child_process").execFile;
  } catch {
    return null;
  }

  for (const candidate of ["python3", "python"]) {
    try {
      const version = await execVersion(execFile, candidate);
      if (version) {
        return { executable: candidate, version };
      }
    } catch {
      // candidate not found, try next
    }
  }
  return null;
}

function execVersion(
  execFile: typeof import("child_process").execFile,
  executable: string,
): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(
      executable,
      ["--version"],
      { timeout: 5000 },
      (error, stdout, stderr) => {
        if (error) {
          resolve(null);
          return;
        }
        // `python --version` outputs "Python 3.x.y" to stdout (or stderr on older versions)
        const output = (stdout || stderr).toString().trim();
        const match = output.match(/Python\s+(\S+)/i);
        resolve(match ? match[1] : null);
      },
    );
  });
}
