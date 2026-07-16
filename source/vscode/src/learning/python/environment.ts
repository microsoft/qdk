// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { LEARNING_VENV_DIR } from "../constants.js";

/**
 * Manages per-course Python environments for `python-notebook` courses.
 *
 * Every operating-system interaction (creating a venv, installing
 * packages, registering a Jupyter kernel) is encapsulated behind a method
 * here and routed through {@link runShell} so the underlying mechanism can
 * later be swapped for the Python extension's API without touching callers.
 *
 * All file access uses `vscode.workspace.fs` and shell work uses the
 * `vscode.tasks` API, keeping this module free of Node built-ins so the
 * extension still bundles for VS Code for the Web (where these desktop-only
 * operations are short-circuited).
 */
export class EnvironmentManager {
  private readonly controllers = new Map<string, vscode.NotebookController>();
  /** Cached result of probing for `uv` on the PATH. */
  private _uvAvailable: boolean | undefined;

  dispose(): void {
    for (const controller of this.controllers.values()) {
      controller.dispose();
    }
    this.controllers.clear();
  }

  /** The course's virtual environment folder. */
  venvUri(courseRoot: vscode.Uri): vscode.Uri {
    return vscode.Uri.joinPath(courseRoot, LEARNING_VENV_DIR);
  }

  /** True on a host where environment management can run (desktop only). */
  get supported(): boolean {
    return vscode.env.uiKind !== vscode.UIKind.Web;
  }

  /**
   * Locate a Python interpreter to bootstrap a venv. Prefers the Python
   * extension's active interpreter, falling back to `python3`/`python`.
   * Returns `undefined` when none can be determined.
   */
  async ensureInterpreter(): Promise<string | undefined> {
    if (!this.supported) {
      return undefined;
    }
    const fromExtension = await this.activeInterpreterPath();
    return fromExtension ?? "python3";
  }

  /** Whether the venv already exists on disk. */
  async venvExists(courseRoot: vscode.Uri): Promise<boolean> {
    return uriExists(this.venvUri(courseRoot));
  }

  /**
   * Create the course venv if it does not yet exist.
   *
   * @param pythonSpec Optional Python version specifier from `course.json`
   *   (e.g. `">=3.11"`, `"3.12"`). When `uv` is available this is passed
   *   directly to `uv venv --python <spec>` which lets `uv` discover or
   *   download a matching interpreter. When `uv` is unavailable, the spec
   *   is ignored and the system interpreter is used.
   */
  async createVenv(courseRoot: vscode.Uri, pythonSpec?: string): Promise<void> {
    if (!this.supported || (await this.venvExists(courseRoot))) {
      return;
    }
    const cwd = courseRoot;
    const venvPath = this.venvUri(courseRoot).fsPath;

    // Prefer `uv` when it's available — it's faster and is the modern
    // default tooling. Fall back to the standard library `venv` module.
    if (await this.uvAvailable()) {
      // With `uv`, pass the version spec (e.g. ">=3.11") or fall back to
      // the system default. `uv` will discover or download a matching
      // interpreter automatically.
      const args = ["venv"];
      if (pythonSpec) {
        args.push("--python", pythonSpec);
      }
      args.push(venvPath);

      const code = await this.runShell(
        "Create course environment",
        "uv",
        args,
        cwd,
      );
      if (code === 0) {
        return;
      }
      log.warn(`\`uv venv\` failed (exit ${code}); falling back to venv.`);
    }

    // For the stdlib fallback we need an actual interpreter path.
    const python = await this.ensureInterpreter();
    if (!python) {
      throw new Error("No Python interpreter was found.");
    }

    // Preflight: on some distros the `venv`/`ensurepip` modules are a
    // separate OS package (e.g. Debian's `python3-venv`). Detect that here
    // so we can surface an actionable message instead of an opaque failure.
    const preflight = await this.runShell(
      "Check Python venv support",
      python,
      ["-c", "import venv, ensurepip"],
      cwd,
    );
    if (preflight !== 0) {
      // TODO (acasey): use their python version number
      throw new Error(
        "This Python installation can't create virtual environments " +
          "(the `venv`/`ensurepip` modules are missing). On Debian/Ubuntu " +
          "install them with `sudo apt install python3-venv` (matching your " +
          "Python version, e.g. `python3.12-venv`), then try again.",
      );
    }

    const code = await this.runShell(
      "Create course environment",
      python,
      ["-m", "venv", venvPath],
      cwd,
    );
    if (code !== 0) {
      throw new Error(
        `Creating the virtual environment failed (exit ${code}).`,
      );
    }
  }

  /**
   * Sync the course environment from its `pyproject.toml`. Prefers `uv sync`
   * when available; falls back to creating a venv with the stdlib `venv`
   * module and installing with `pip install .`.
   *
   * @param courseRoot The course's source folder (where `pyproject.toml`
   *   lives and where the `.venv` is created).
   * @param pythonSpec Optional Python version specifier from course metadata
   *   (e.g. `">=3.11"`). Passed to {@link createVenv} in the fallback path.
   */
  async syncEnvironment(
    courseRoot: vscode.Uri,
    pythonSpec?: string,
  ): Promise<void> {
    if (!this.supported) {
      return;
    }

    if (await this.uvAvailable()) {
      const code = await this.runShell(
        "Sync course environment",
        "uv",
        ["sync", "--project", courseRoot.fsPath],
        courseRoot,
      );
      if (code === 0) {
        return;
      }
      log.warn(
        `\`uv sync\` failed (exit ${code}); falling back to venv + pip.`,
      );
    }

    // Fallback: create a venv and install from pyproject.toml using pip.
    await this.createVenv(courseRoot, pythonSpec);
    const python = await this.venvPython(courseRoot);
    if (!python) {
      throw new Error("Failed to create the virtual environment.");
    }

    const code = await this.runShell(
      "Install from pyproject.toml",
      python,
      ["-m", "pip", "install", "--disable-pip-version-check", "."],
      courseRoot,
    );
    if (code !== 0) {
      throw new Error(
        `\`pip install .\` failed (exit ${code}). Check the terminal output for details.`,
      );
    }
  }

  /**
   * Install the course's pinned requirements into its venv. Always installs
   * `ipykernel` as well so the Jupyter extension can discover and run the
   * venv as a notebook kernel without a globally-registered kernelspec.
   */
  async installRequirements(
    courseRoot: vscode.Uri,
    requirements: string[],
  ): Promise<void> {
    if (!this.supported) {
      return;
    }
    const python = await this.venvPython(courseRoot);
    if (!python) {
      throw new Error("The course environment is missing its interpreter.");
    }
    // De-duplicate while preserving order; ipykernel is required for the
    // venv to act as a Jupyter kernel.
    const packages = [...new Set(["ipykernel", ...requirements])];
    const cwd = courseRoot;

    if (await this.uvAvailable()) {
      const code = await this.runShell(
        "Install course requirements",
        "uv",
        ["pip", "install", "--python", python, ...packages],
        cwd,
      );
      if (code === 0) {
        return;
      }
      log.warn(
        `\`uv pip install\` failed (exit ${code}); falling back to pip.`,
      );
    }

    const code = await this.runShell(
      "Install course requirements",
      python,
      ["-m", "pip", "install", "--disable-pip-version-check", ...packages],
      cwd,
    );
    if (code !== 0) {
      throw new Error(`Installing requirements failed (exit ${code}).`);
    }
  }

  /**
   * Select this course's venv as the kernel/interpreter for a notebook.
   *
   * Uses the **stable** `ms-python.python` `environments` API
   * (`updateActiveEnvironmentPath`) to set the interpreter for the notebook
   * resource — the mechanism the Jupyter extension honors when picking a
   * kernel — and additionally nudges the picker with a core
   * {@link vscode.NotebookController} affinity hint.
   *
   * We deliberately do NOT register a global kernelspec
   * (`ipykernel install --user`): that pollutes the user's kernel list and
   * competes with the Jupyter extension's own environment discovery.
   * Instead {@link installRequirements} puts `ipykernel` in the venv so
   * Jupyter can discover and run it directly.
   */
  async selectKernelForNotebook(
    notebook: vscode.NotebookDocument,
    courseRoot: vscode.Uri,
    courseId: string,
    displayName: string,
  ): Promise<void> {
    if (!this.supported) {
      return;
    }

    // Primary, stable path: point the Python extension at the venv
    // interpreter for this notebook resource.
    const python = await this.venvPython(courseRoot);
    if (python) {
      await this.setActiveInterpreter(notebook.uri, python);
    }

    // Secondary nudge: a notebook controller affinity hint. This is a core
    // VS Code API (not Python-specific) and is safe to keep as a fallback.
    let controller = this.controllers.get(courseId);
    if (!controller) {
      controller = vscode.notebooks.createNotebookController(
        `qdk-learning-${courseId}`,
        "jupyter-notebook",
        `QDK: ${displayName}`,
      );
      controller.supportedLanguages = ["python"];
      controller.description = "QDK course environment";
      this.controllers.set(courseId, controller);
    }
    controller.updateNotebookAffinity(
      notebook,
      vscode.NotebookControllerAffinity.Preferred,
    );
  }

  /** Path to the venv's Python interpreter, or `undefined` if not present. */
  async venvPython(courseRoot: vscode.Uri): Promise<string | undefined> {
    const venv = this.venvUri(courseRoot);
    const candidates = [
      vscode.Uri.joinPath(venv, "bin", "python"),
      vscode.Uri.joinPath(venv, "bin", "python3"),
      vscode.Uri.joinPath(venv, "Scripts", "python.exe"),
    ];
    for (const candidate of candidates) {
      if (await uriExists(candidate)) {
        return candidate.fsPath;
      }
    }
    return undefined;
  }

  /**
   * Verify the given modules import in the course venv (e.g. `qdk`,
   * `qsharp_widgets`). Returns `false` if the venv or interpreter is
   * missing or the import fails.
   */
  async checkImports(
    courseRoot: vscode.Uri,
    modules: string[],
  ): Promise<boolean> {
    if (!this.supported || modules.length === 0) {
      return false;
    }
    const python = await this.venvPython(courseRoot);
    if (!python) {
      return false;
    }
    const code = await this.runShell(
      "Verify course packages",
      python,
      ["-c", `import ${modules.join(", ")}`],
      courseRoot,
    );
    return code === 0;
  }

  /**
   * Per-module import report for the course venv. Each entry is `true` when
   * that module imports successfully. Missing venv/interpreter yields all
   * `false`. Used by the environment check to pinpoint which package is
   * missing.
   */
  async importsReport(
    courseRoot: vscode.Uri,
    modules: string[],
  ): Promise<{ module: string; ok: boolean }[]> {
    if (!this.supported || modules.length === 0) {
      return modules.map((module) => ({ module, ok: false }));
    }
    const python = await this.venvPython(courseRoot);
    if (!python) {
      return modules.map((module) => ({ module, ok: false }));
    }
    const results: { module: string; ok: boolean }[] = [];
    for (const module of modules) {
      const code = await this.runShell(
        `Check import: ${module}`,
        python,
        ["-c", `import ${module}`],
        courseRoot,
      );
      results.push({ module, ok: code === 0 });
    }
    return results;
  }

  /** Whether `uv` is available on the PATH (public diagnostics accessor). */
  async hasUv(): Promise<boolean> {
    return this.uvAvailable();
  }

  /**
   * Whether the given interpreter can create virtual environments (the
   * `venv` and `ensurepip` modules are importable). On some Linux distros
   * these are a separate OS package. Defaults to the bootstrap interpreter.
   */
  async venvModuleSupported(python?: string): Promise<boolean> {
    if (!this.supported) {
      return false;
    }
    const interpreter = python ?? (await this.ensureInterpreter());
    if (!interpreter) {
      return false;
    }
    const code = await this.runShell("Check Python venv support", interpreter, [
      "-c",
      "import venv, ensurepip",
    ]);
    return code === 0;
  }

  // ─── Private: swappable OS interaction ───

  /**
   * The Python extension's stable `environments` API, or `undefined` when
   * the extension is unavailable. Only the documented, non-proposed members
   * are typed here.
   */
  private async pythonEnvironmentsApi(): Promise<
    // TODO (acasey): consider naming this type
    | {
        getActiveEnvironmentPath?: (resource?: vscode.Uri) => {
          path?: string;
        };
        updateActiveEnvironmentPath?: (
          environment: string,
          resource?: vscode.Uri,
        ) => Thenable<void>;
      }
    | undefined
  > {
    const ext = vscode.extensions.getExtension("ms-python.python");
    if (!ext) {
      return undefined;
    }
    try {
      const api = (await ext.activate()) as {
        environments?: {
          getActiveEnvironmentPath?: (resource?: vscode.Uri) => {
            path?: string;
          };
          updateActiveEnvironmentPath?: (
            environment: string,
            resource?: vscode.Uri,
          ) => Thenable<void>;
        };
      };
      return api.environments;
    } catch (e) {
      log.warn(`Could not query the Python extension: ${String(e)}`);
      return undefined;
    }
  }

  /** The Python extension's active interpreter path, if available. */
  private async activeInterpreterPath(): Promise<string | undefined> {
    const environments = await this.pythonEnvironmentsApi();
    return environments?.getActiveEnvironmentPath?.()?.path;
  }

  /**
   * Set the active interpreter for a resource via the stable Python
   * extension API. The Jupyter extension uses this association to pick the
   * kernel for the notebook.
   */
  private async setActiveInterpreter(
    resource: vscode.Uri,
    pythonPath: string,
  ): Promise<void> {
    const environments = await this.pythonEnvironmentsApi();
    if (!environments?.updateActiveEnvironmentPath) {
      return;
    }
    try {
      await environments.updateActiveEnvironmentPath(pythonPath, resource);
    } catch (e) {
      log.warn(`Could not set the active interpreter: ${String(e)}`);
    }
  }

  /** Whether `uv` is available on the PATH. Cached after the first probe. */
  private async uvAvailable(): Promise<boolean> {
    if (this._uvAvailable === undefined) {
      const code = await this.runShell("Check for uv", "uv", ["--version"]);
      this._uvAvailable = code === 0;
    }
    return this._uvAvailable;
  }

  /**
   * Run a shell command as a one-shot task and resolve with its exit code.
   * Centralized so the execution mechanism stays swappable.
   */
  private runShell(
    name: string,
    command: string,
    args: string[],
    cwd?: vscode.Uri,
  ): Promise<number> {
    // Course commands pass the course root; course-independent probes
    // (`uv --version`, `python -c "import venv"`) don't depend on the cwd,
    // so they fall back to the workspace folder, which is guaranteed to exist.
    const cwdPath = (cwd ?? vscode.workspace.workspaceFolders?.[0]?.uri)
      ?.fsPath;
    const task = new vscode.Task(
      { type: "qdk-learning" },
      vscode.TaskScope.Workspace,
      name,
      "qdk-learning",
      new vscode.ShellExecution(command, args, { cwd: cwdPath }),
    );
    task.presentationOptions = {
      reveal: vscode.TaskRevealKind.Silent,
      focus: false,
      clear: false,
    };
    return new Promise<number>((resolve) => {
      const sub = vscode.tasks.onDidEndTaskProcess((e) => {
        if (e.execution.task === task) {
          sub.dispose();
          resolve(e.exitCode ?? -1);
        }
      });
      void vscode.tasks.executeTask(task);
    });
  }
}

// ─── Helpers ───

async function uriExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}
