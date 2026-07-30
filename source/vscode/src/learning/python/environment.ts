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
 * Dotted Python module name. Import checks come from author-supplied
 * `course.json`, and reach a `python -c` command line.
 */
const MODULE_NAME = /^[A-Za-z_]\w*(\.[A-Za-z_]\w*)*$/;

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
  /** In-flight {@link ensureEnvironment} calls, keyed by courseRoot.toString(). */
  private readonly _pendingEnvironments = new Map<string, Promise<void>>();

  dispose(): void {
    this._projectEnvironmentMap.clear();
    this._pendingEnvironments.clear();
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
    // Finding an existing environment and creating one are separate awaits, so
    // concurrent callers must share a single attempt or each creates its own.
    const key = courseRoot.toString();
    let pending = this._pendingEnvironments.get(key);
    if (!pending) {
      pending = this.resolveEnvironment(courseRoot).finally(() => {
        this._pendingEnvironments.delete(key);
      });
      this._pendingEnvironments.set(key, pending);
    }
    return pending;
  }

  private async resolveEnvironment(courseRoot: vscode.Uri): Promise<void> {
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
        `Using existing environment for ${courseRoot.fsPath}: ${env.name}`,
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

      // Register the course folder as a Python project. This creates a workspace
      // setting, which causes Jupyter to pick up the venv.
      const courseName = courseRoot.path.split("/").pop();
      await api.addPythonProject({
        name: `QDK Course: ${courseName}`,
        uri: courseRoot,
      });
    }

    this._projectEnvironmentMap.set(courseRoot.toString(), env);
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
      if (!MODULE_NAME.test(module)) {
        log.warn(`Not a valid Python module name, skipping: ${module}`);
        results.push({ module, ok: false });
        continue;
      }
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

    await api.refreshEnvironments(courseRoot);
    const envs = await api.getEnvironments(courseRoot);

    switch (envs.length) {
      case 0:
        return undefined;
      case 1:
        return envs[0];
      default:
        log.warn(
          `Found multiple virtual environments, using first: ${envs
            .map((e) => e.name)
            .join(", ")}`,
        );
        return envs[0];
    }
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
