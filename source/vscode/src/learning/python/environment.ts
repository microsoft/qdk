// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import type {
  PythonEnvironment,
  PythonEnvironmentApi,
  PythonProcess,
} from "@vscode/python-environments";
import { PythonEnvironments } from "@vscode/python-environments";
import * as vscode from "vscode";

/**
 * Manages per-course Python environments for `python-notebook` courses.
 *
 * All environment lifecycle operations (creation, package installation,
 * import verification) are routed through the `@vscode/python-environments`
 * API. This module is free of Node built-ins so the extension still bundles
 * for VS Code for the Web (where these desktop-only operations are
 * short-circuited).
 */
export class EnvironmentManager {
  /** Cached Python Environments extension API (only set on success). */
  private _pythonEnvApi: PythonEnvironmentApi | undefined;
  /** Cached environments keyed by courseRoot.toString(). */
  private readonly _projectEnvironmentMap = new Map<
    string,
    PythonEnvironment
  >();

  dispose(): void {
    this._projectEnvironmentMap.clear();
  }

  /** True on a host where environment management can run (desktop only). */
  get supported(): boolean {
    return vscode.env.uiKind !== vscode.UIKind.Web;
  }

  /**
   * Ensure a Python environment exists for the given course and install the
   * packages listed in requirements.txt. If an environment already exists in the target
   * directory it is reused and packages are installed into it; otherwise a
   * new environment is created.
   *
   * @param courseRoot The course's source folder (where `pyproject.toml`
   *   may live and where the environment is created).
   */
  async ensureEnvironment(courseRoot: vscode.Uri): Promise<void> {
    if (!this.supported) {
      return;
    }
    const api = await this.pythonEnvironmentsApi();
    if (!api) {
      log.warn(
        "The Python Environments extension is required for Python courses. " +
          "Install it from the VS Code Marketplace.",
      );
      return;
    }

    // Check for an existing environment in this directory.
    let env = await this.findEnvironment(api, courseRoot);

    if (env) {
      log.info(
        `Updating existing environment for ${courseRoot.fsPath}: ${env.name}`,
      );
    } else {
      // Create a new environment. The API picks up requirements.txt, if present.
      // As of July 2026, it will not parse pyproject.toml.
      log.info(`Creating new environment for ${courseRoot.fsPath}`);
      env = await api.createEnvironment(courseRoot, { quickCreate: true });
      if (!env) {
        log.warn(
          `Failed to create a Python environment in ${courseRoot.fsPath}. ` +
            `Ensure the Python Environments extension has a registered environment manager.`,
        );
        return;
      }

      // Cache the resolved environment.
      this._projectEnvironmentMap.set(courseRoot.toString(), env);
    }
  }

  /**
   * Whether an environment exists for the given course root.
   */
  async environmentExists(courseRoot: vscode.Uri): Promise<boolean> {
    if (!this.supported) {
      return false;
    }
    const api = await this.pythonEnvironmentsApi();
    if (!api) {
      return false;
    }
    const env = await this.findEnvironment(api, courseRoot);
    return env !== undefined;
  }

  /**
   * Return the `{ id, path }` for the course's Python environment, suitable
   * for passing to the Jupyter extension's `openNotebook` API.
   * Returns `undefined` when no environment has been resolved.
   */
  async getJupyterEnvironmentPath(
    courseRoot: vscode.Uri,
  ): Promise<{ id: string; path: string } | undefined> {
    if (!this.supported) {
      return undefined;
    }
    const api = await this.pythonEnvironmentsApi();
    if (!api) {
      return undefined;
    }
    const env = await this.findEnvironment(api, courseRoot);
    if (!env) {
      return undefined;
    }
    return {
      id: env.envId.id,
      path: env.environmentPath.fsPath,
    };
  }

  /**
   * Per-module import report for the course environment. Each entry is
   * `true` when that module imports successfully. Missing environment yields
   * all `false`.
   */
  async importsReport(
    courseRoot: vscode.Uri,
    modules: string[],
  ): Promise<{ module: string; ok: boolean }[]> {
    if (!this.supported || modules.length === 0) {
      return modules.map((module) => ({ module, ok: false }));
    }
    const api = await this.pythonEnvironmentsApi();
    if (!api) {
      return modules.map((module) => ({ module, ok: false }));
    }
    const env = await this.findEnvironment(api, courseRoot);
    if (!env) {
      return modules.map((module) => ({ module, ok: false }));
    }

    const results: { module: string; ok: boolean }[] = [];
    for (const module of modules) {
      const code = await runPython(api, env, ["-c", `import ${module}`]);
      results.push({ module, ok: code === 0 });
    }
    return results;
  }

  // ─── Private helpers ───

  /**
   * The Python Environments extension API, or `undefined` when the
   * extension is unavailable. A successful lookup is cached; failures
   * are retried so the extension can be installed mid-session.
   */
  private async pythonEnvironmentsApi(): Promise<
    PythonEnvironmentApi | undefined
  > {
    if (this._pythonEnvApi) {
      return this._pythonEnvApi;
    }
    try {
      this._pythonEnvApi = await PythonEnvironments.api();
    } catch (e) {
      log.warn(`Python Environments extension is not available: ${String(e)}`);
    }
    return this._pythonEnvApi;
  }

  /**
   * Find an existing environment in the given directory.
   */
  private async findEnvironment(
    api: PythonEnvironmentApi,
    courseRoot: vscode.Uri,
  ): Promise<PythonEnvironment | undefined> {
    // Check cache first.
    const cached = this._projectEnvironmentMap.get(courseRoot.toString());
    if (cached) {
      return cached;
    }

    // TODO (acasey): pick an approach
    // This version creates a workspace setting, which could be noise for the user
    // but seems to cause Jupyter to pick up the venv and might make other
    // python environment operations easier in the future
    const courseName = courseRoot.path.split("/").pop();
    void api.addPythonProject({
      name: `QDK Course: ${courseName}`, // This doesn't seem to persist across sessions
      uri: courseRoot,
    }); // Can drop result - just want side effect
    await api.refreshEnvironments(courseRoot); // As now
    const envs = await api.getEnvironments(courseRoot); // React somehow if there are multiple

    switch (envs.length) {
      case 0:
        return undefined;
      case 1:
        return envs[0]; // TODO (acasey): need to enforce location?
      default:
        log.warn(
          `Found multiple virtual environments, using first: ${envs.join(", ")}`,
        );
        return envs[0];
    }

    // // Without a refresh, getEnvironment seems to pick up the global install
    // await api.refreshEnvironments(courseRoot);
    // const env = await api.getEnvironment(courseRoot);
    // if (env) {
    //   // If there's no local venv, getEnvironment will return the global install
    //   const envPath = env.environmentPath.toString();
    //   const rootPath = courseRoot.toString().replace(/\/?$/, "/");
    //   if (!envPath.startsWith(rootPath)) {
    //     log.debug(
    //       `Ignoring environment "${env.name}" at ${envPath} ` +
    //         `because it is not under ${rootPath}`,
    //     );
    //     return undefined;
    //   }
    //   this._projectEnvironmentMap.set(courseRoot.toString(), env);
    // }
    // return env;
  }
}

// ─── Helpers ───

/**
 * Run Python with the given args in the background and return the exit code.
 */
function runPython(
  api: PythonEnvironmentApi,
  env: PythonEnvironment,
  args: string[],
): Promise<number> {
  return new Promise<number>((resolve) => {
    api
      .runInBackground(env, { args })
      .then((proc: PythonProcess) => {
        proc.stdout.on("data", (data) => {
          log.info(`python stdout: ${String(data)}`);
        });
        proc.stderr.on("data", (data) => {
          log.warn(`python stderr: ${String(data)}`);
        });
        proc.onExit((code) => {
          resolve(code ?? -1);
        });
      })
      .catch((e) => {
        log.warn(`Failed to run Python: ${String(e)}`);
        resolve(-1);
      });
  });
}
