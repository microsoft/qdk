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
  // TODO (acasey): do we need a cache?
  private readonly _envCache = new Map<string, PythonEnvironment>();

  dispose(): void {
    this._envCache.clear();
  }

  /** True on a host where environment management can run (desktop only). */
  get supported(): boolean {
    return vscode.env.uiKind !== vscode.UIKind.Web;
  }

  /**
   * Ensure a Python environment exists for the given course and install the
   * specified packages. If an environment already exists in the target
   * directory it is reused and packages are installed into it; otherwise a
   * new environment is created.
   *
   * @param courseRoot The course's source folder (where `pyproject.toml`
   *   may live and where the environment is created).
   * @param packages Packages to install (e.g. `["ipykernel", "qdk"]`).
   */
  async ensureEnvironment(
    courseRoot: vscode.Uri,
    packages: string[],
  ): Promise<void> {
    if (!this.supported) {
      return;
    }
    const api = await this.pythonEnvironmentsApi();
    if (!api) {
      // TODO (acasey): this goes to the extension host output window, which isn't useful
      throw new Error(
        "The Python Environments extension is required for Python courses. " +
          "Install it from the VS Code Marketplace.",
      );
    }

    // Check for an existing environment in this directory.
    let env = await this.findEnvironment(api, courseRoot);

    if (env) {
      log.info(
        `Updating existing environment for ${courseRoot.fsPath}: ${env.name}`,
      );
    } else {
      // Create a new environment. The API picks up pyproject.toml if present.
      log.info(`Creating new environment for ${courseRoot.fsPath}`);
      env = await api.createEnvironment(courseRoot, { quickCreate: true });
      if (!env) {
        // TODO (acasey): where does this go?
        throw new Error(
          `Failed to create a Python environment in ${courseRoot.fsPath}. ` +
            `Ensure the Python Environments extension has a registered environment manager.`,
        );
      }

      // Cache the resolved environment.
      this._envCache.set(courseRoot.toString(), env);
    }

    // Install packages.
    if (packages.length > 0) {
      log.info(`Installing packages: ${packages.join(", ")}`);
      await api.managePackages(env, { install: packages });
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
    const cached = this._envCache.get(courseRoot.toString());
    if (cached) {
      return cached;
    }

    // Without a refresh, getEnvironment seems to pick up the global install
    await api.refreshEnvironments(courseRoot);
    const env = await api.getEnvironment(courseRoot);
    if (env) {
      // If there's no local venv, getEnvironment will return the global install
      const envPath = env.environmentPath.toString();
      const rootPath = courseRoot.toString().replace(/\/?$/, "/");
      if (!envPath.startsWith(rootPath)) {
        log.debug(
          `Ignoring environment "${env.name}" at ${envPath} ` +
            `because it is not under ${rootPath}`,
        );
        return undefined;
      }
      this._envCache.set(courseRoot.toString(), env);
    }
    return env;
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
