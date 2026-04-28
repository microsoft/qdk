// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Singleton webview panel manager for the Quantum Katas full-view experience.
 *
 * Creates a WebviewPanel, bridges postMessage ↔ LearningService, delegates run /
 * circuit to VS Code commands, uses loadCompilerWorker for exercise checking,
 * and listens to LearningService.onDidChangeState for state sync.
 */

import * as vscode from "vscode";
import { QscEventTarget } from "qsharp-lang";
import { getExerciseSources } from "qsharp-lang/katas-md";
import type { Exercise } from "qsharp-lang/katas-md";
import { loadCompilerWorker, qsharpExtensionId } from "../../common.js";
import { runExerciseInTerminal } from "../../run.js";
import type { LearningService } from "../index.js";
import type { ProgressWatcher } from "../progress/progressReader.js";
import { detectKatasWorkspace } from "../progress/detector.js";
import type { SolutionCheckResult } from "../types.js";

let instance: KatasPanelManager | undefined;

export class KatasPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly progressWatcher: ProgressWatcher,
    private readonly service: LearningService,
  ) {}

  /**
   * Show the panel and execute the "check solution" action, sending the
   * result to the webview so it renders the same output as clicking the
   * panel's own Check button. Returns whether the solution passed.
   */
  async checkAndShowResult(): Promise<boolean> {
    await this.show();
    const result = await this.executeCheck();
    this.sendResult("check", result);
    return result.passed;
  }

  /**
   * Show (or create) the Katas panel.
   * If the panel already exists, it's revealed; otherwise a new one is created.
   */
  async show(): Promise<void> {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
      return;
    }

    // Detect the katas workspace
    const info = await detectKatasWorkspace();
    if (!info) {
      vscode.window.showWarningMessage(
        "No Quantum Katas workspace detected. Open a folder containing qdk-learning.json first.",
      );
      return;
    }

    // Initialize the shared service if needed
    if (!this.service.initialized) {
      await this.service.initialize(info.workspaceRoot, info.katasRoot);
    }

    // Create webview panel
    this.panel = vscode.window.createWebviewPanel(
      "qsharp-katas",
      "Quantum Katas",
      { viewColumn: vscode.ViewColumn.One, preserveFocus: false },
      {
        enableScripts: true,
        enableFindWidget: true,
        retainContextWhenHidden: true,
        localResourceRoots: [
          vscode.Uri.joinPath(this.extensionUri, "out"),
          info.katasRoot,
        ],
      },
    );

    this.panel.iconPath = vscode.Uri.joinPath(
      this.extensionUri,
      "resources",
      "qdk.svg",
    );

    // Generate and set HTML
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    this.attachPanel();
  }

  /**
   * Restore a serialized Katas panel after VS Code restarts.
   * Re-initializes the service from disk and sends fresh state to the webview.
   */
  async restore(panel: vscode.WebviewPanel): Promise<void> {
    // Detect the katas workspace
    const info = await detectKatasWorkspace();
    if (!info) {
      // Workspace no longer available — dispose the stale panel
      panel.dispose();
      return;
    }

    // Initialize or refresh the shared service from disk
    if (!this.service.initialized) {
      await this.service.initialize(info.workspaceRoot, info.katasRoot);
    } else {
      await this.service.reloadProgress();
    }

    this.panel = panel;

    // Re-set HTML — webview resource URIs change across sessions
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    this.attachPanel();
  }

  /**
   * Wire up shared listeners on an already-created panel.
   * Called by both show() (new panel) and restore() (deserialized panel).
   */
  private attachPanel(): void {
    if (!this.panel) {
      return;
    }

    this.panel.onDidDispose(
      () => {
        this.panel = undefined;
        this.ready = false;
        this.queuedMessages = [];
      },
      undefined,
      this.disposables,
    );

    // Listen for webview messages
    this.panel.webview.onDidReceiveMessage(
      (msg) => this.handleMessage(msg),
      undefined,
      this.disposables,
    );

    // Listen for progress changes (external edits to qdk-learning.json)
    this.disposables.push(
      this.progressWatcher.onDidChange(async () => {
        if (this.service.initialized && this.panel) {
          await this.service.reloadProgress();
          this.sendState();
        }
      }),
    );

    // Listen for state changes from LM tools (navigation, check, etc.)
    this.disposables.push(
      this.service.onDidChangeState(() => {
        if (this.panel) {
          this.sendState();
          this.openCurrentFile().catch(() => {});
        }
      }),
    );
  }

  dispose(): void {
    this.panel?.dispose();
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
    instance = undefined;
  }

  // ─── Singleton accessor ───

  static getInstance(
    extensionUri: vscode.Uri,
    progressWatcher: ProgressWatcher,
    learningService: LearningService,
  ): KatasPanelManager {
    if (!instance) {
      instance = new KatasPanelManager(
        extensionUri,
        progressWatcher,
        learningService,
      );
    }
    return instance;
  }

  // ─── Message bridge ───

  private sendMessage(msg: unknown): void {
    if (!this.panel) {
      return;
    }
    if (this.ready) {
      this.panel.webview.postMessage(msg);
    } else {
      this.queuedMessages.push(msg);
    }
  }

  private sendState(): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({ command: "state", state: this.service.getState() });
  }

  /**
   * If the current position is an exercise or example, open the
   * corresponding .qs file in the secondary editor column.
   */
  private async openCurrentFile(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const pos = this.service.getPosition();
    let fileUri: vscode.Uri | undefined;
    if (pos.item.type === "exercise") {
      fileUri = this.service.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      fileUri = this.service.getExampleFileUri();
    }
    if (fileUri) {
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
    }
  }

  private sendResult(action: string, result: unknown): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({
      command: "result",
      action,
      result,
      state: this.service.getState(),
    });
  }

  private sendError(message: string): void {
    this.sendMessage({ command: "error", message });
  }

  private async handleMessage(msg: any): Promise<void> {
    if (msg.command === "ready") {
      this.ready = true;
      for (const queued of this.queuedMessages) {
        this.panel?.webview.postMessage(queued);
      }
      this.queuedMessages = [];
      this.sendState();
      await this.openCurrentFile();
      return;
    }

    if (msg.command === "openFile") {
      const uri = vscode.Uri.parse(msg.uri);
      await vscode.commands.executeCommand("vscode.open", uri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
      return;
    }

    if (msg.command === "openChat") {
      const text = msg.text || "Give me a hint";
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: `/qdk-learning ${text}`,
      });
      return;
    }

    if (msg.command === "focusProgress") {
      await vscode.commands.executeCommand("qsharp-vscode.katasTree.focus");
      return;
    }

    if (msg.command === "action") {
      await this.handleAction(msg.action);
    }
  }

  private async handleAction(action: string): Promise<void> {
    if (!this.service.initialized) {
      return;
    }

    try {
      switch (action) {
        case "next": {
          const result = this.service.next();
          this.sendResult("next", result);
          break;
        }
        case "back": {
          const result = this.service.previous();
          this.sendResult("back", result);
          break;
        }
        case "run": {
          await this.executeRun();
          this.sendResult("run", {});
          break;
        }
        case "circuit": {
          await this.executeCircuit();
          this.sendResult("circuit", {});
          break;
        }
        case "check": {
          const result = await this.executeCheck();
          this.sendResult("check", result);
          break;
        }
        case "solution": {
          const code = this.service.getFullSolution();
          this.sendResult("solution", code);
          break;
        }
        case "reveal-answer": {
          const { result } = this.service.revealAnswer();
          this.sendResult("reveal-answer", result);
          break;
        }
        default:
          this.sendError(`Unknown action: ${action}`);
      }
    } catch (err: unknown) {
      this.sendError(err instanceof Error ? err.message : String(err));
    }
  }

  // ─── Command delegation ───

  private async executeRun(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const pos = this.service.getPosition();

    if (pos.item.type === "exercise") {
      const exercise = this.service.resolveExercise() as Exercise;
      const userCode = await this.service.readUserCode();
      const sources = await getExerciseSources(exercise);
      const fileUri = this.service.getExerciseFileUri();

      // Open the file first
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);

      runExerciseInTerminal(
        this.extensionUri,
        userCode,
        sources,
        "QDK: Run Program",
      );
    } else if (pos.item.type === "lesson-example") {
      const fileUri = this.service.getExampleFileUri();
      this.service.markExampleRun(pos.item.id);

      // Open the file first, then run via normal command
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
      await vscode.commands.executeCommand(
        `${qsharpExtensionId}.runProgram`,
        fileUri,
      );
    } else {
      throw new Error("Current item cannot be run.");
    }
  }

  private async executeCircuit(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const pos = this.service.getPosition();

    let fileUri: vscode.Uri;
    if (pos.item.type === "exercise") {
      fileUri = this.service.getExerciseFileUri();
    } else if (pos.item.type === "lesson-example") {
      fileUri = this.service.getExampleFileUri();
    } else {
      throw new Error("Current item cannot show a circuit.");
    }

    await vscode.commands.executeCommand("vscode.open", fileUri, {
      viewColumn: vscode.ViewColumn.Two,
      preview: false,
    } satisfies vscode.TextDocumentShowOptions);
    await vscode.commands.executeCommand(
      `${qsharpExtensionId}.showCircuit`,
      fileUri,
    );
  }

  private async executeCheck(): Promise<SolutionCheckResult> {
    if (!this.service.initialized) {
      return { passed: false, events: [], error: "Service not initialized" };
    }

    const pos = this.service.getPosition();
    if (pos.item.type !== "exercise") {
      throw new Error("Current item is not an exercise.");
    }

    // Get the exercise object for exercise sources
    const exercise = this.service.resolveExercise() as Exercise;
    const userCode = await this.service.readUserCode();
    const sources = await getExerciseSources(exercise);

    // Use the extension's compiler worker
    const worker = loadCompilerWorker(this.extensionUri);
    const eventTarget = new QscEventTarget(true);

    try {
      const passed = await worker.checkExerciseSolution(
        userCode,
        sources,
        eventTarget,
      );

      const events: { type: string; message?: string }[] = [];
      const resultCount = eventTarget.resultCount();
      const results = eventTarget.getResults();
      for (let i = 0; i < resultCount; i++) {
        const r = results[i];
        for (const evt of r.events) {
          if (evt.type === "Message") {
            events.push({
              type: "Message",
              message: (evt as { message: string }).message,
            });
          } else {
            events.push({ type: evt.type });
          }
        }
      }

      if (passed) {
        await this.service.markExerciseComplete(pos.kataId, pos.sectionId);
      }

      return {
        passed,
        events,
        error: passed
          ? undefined
          : events.length > 0
            ? events
                .filter((e) => e.message)
                .map((e) => e.message)
                .join("\n") || undefined
            : "Solution check failed.",
      };
    } catch (err: unknown) {
      return {
        passed: false,
        events: [],
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      worker.terminate();
    }
  }

  // ─── HTML generation ───

  private getWebviewContent(webview: vscode.Webview): string {
    const extensionUri = this.extensionUri;

    function getUri(...parts: string[]): vscode.Uri {
      return webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, ...parts));
    }

    const renderJsUri = getUri("out", "learning", "panel", "render.js");
    const webviewClientJsUri = getUri(
      "out",
      "learning",
      "panel",
      "webview-client.js",
    );
    const cssUri = getUri("out", "learning", "panel", "webview.css");
    const katexCssUri = getUri("out", "katex", "katex.min.css");
    const codiconCssUri = getUri("out", "katex", "codicon.css");

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Quantum Katas</title>
    <link rel="stylesheet" href="${katexCssUri}" />
    <link rel="stylesheet" href="${codiconCssUri}" />
    <link rel="stylesheet" href="${cssUri}" />
  </head>
  <body>
    <div class="branding">
      <svg class="branding-icon" width="18" height="18" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M7.9062 12.88C8.8625 12.6325 9.9444 11.9581 11.1425 10.8631L11.5994 10.445L11.8781 10.5119C12.0312 10.5488 12.4656 10.5819 12.8438 10.5862C13.7888 10.5962 14.2263 10.4475 14.7856 9.9244C15.3362 9.4094 15.3569 9.3225 15.225 8.0819C15.1519 7.3925 15.0819 7.015 14.9938 6.8319C14.7538 6.3338 14.1244 5.9056 13.3881 5.7406C12.9881 5.6506 12.53 5.7213 11.915 5.9675L11.5013 6.1331L10.7975 5.605C8.4731 3.8594 7.1956 3.2437 5.3969 3.0019C4.725 2.9112 4.1688 3.0238 3.3969 3.4044C2.7794 3.7094 2.5675 3.9131 2.065 4.6875C1.7006 5.2481 1.1706 6.3631 0.9644 7C0.8256 7.4306 0.7925 7.65 0.7906 8.1562C0.7887 8.7131 0.8106 8.8406 0.9931 9.3269C1.1062 9.6269 1.3119 10.0488 1.4506 10.2644C1.9275 11.0069 3.12 12.0569 3.9688 12.4825C4.2806 12.6387 5.1162 12.9131 5.5312 12.995C6.0244 13.0925 7.3288 13.0294 7.9062 12.88ZM5.4375 11.9069C5.2138 11.8619 4.89 11.7713 4.7169 11.705C4.0088 11.4344 2.8013 10.4431 2.325 9.7413C1.9619 9.205 1.8513 8.845 1.8494 8.1875C1.8481 7.5744 1.9319 7.1856 2.195 6.5912L2.3306 6.2844L2.405 6.5325C2.7588 7.7138 3.7144 8.95 4.8338 9.6738C5.7206 10.2481 7.3781 10.9044 8.3438 11.0638C8.5844 11.1037 8.8506 11.15 8.9362 11.1662L9.0906 11.1969L8.9362 11.3175C8.2063 11.8875 6.6475 12.15 5.4375 11.9069ZM9.9038 10.43C9.8469 10.2494 9.5538 10.1463 8.9525 10.0944C8.6687 10.0694 8.4225 10.0262 8.405 9.9975C8.3875 9.9694 8.4325 9.9044 8.505 9.8537C8.5775 9.8031 8.9225 9.4944 9.2713 9.1675C10.3356 8.1725 10.825 7.7531 11.2581 7.4644C12.4038 6.7006 13.2844 6.5587 13.8631 7.045C14.0687 7.2175 14.0706 7.2244 14.1662 8.1137L14.2631 9.0094L14.1156 9.1669C14.035 9.2538 13.8894 9.3656 13.7919 9.4162C13.6062 9.5131 12.75 9.6119 12.75 9.5369C12.75 9.4669 13.4294 9.175 13.4962 9.2163C13.53 9.2369 13.6531 9.2137 13.77 9.165C14.2169 8.9781 14.1744 8.4206 13.6981 8.2306C13.4612 8.1356 13.4238 8.1363 13.11 8.2444C12.33 8.5125 11.9325 8.7794 10.8006 9.7925C10.0463 10.4681 9.94 10.5431 9.9038 10.43ZM6.5613 9.4475C5.9169 9.165 5.37 8.8387 4.9 8.4556C4.5125 8.14 3.7244 7.185 3.7975 7.1206C3.8675 7.0587 4.3594 6.9506 4.7812 6.9044C5.2319 6.855 5.9938 6.9525 6.475 7.1219C6.8481 7.2531 7.4494 7.6106 8.1456 8.115L8.5719 8.4244L7.87 9.0556C7.4844 9.4031 7.1513 9.6862 7.1312 9.6844C7.1106 9.6825 6.8544 9.5756 6.5613 9.4475ZM8.5938 7.1844C7.7844 6.6006 6.9756 6.1837 6.2812 5.9912C5.5613 5.7912 4.5969 5.8 3.8919 6.0131L3.3769 6.1681L3.3394 5.9681C3.3188 5.8587 3.3175 5.505 3.3356 5.1825C3.3731 4.5362 3.3687 4.5431 3.8788 4.2812C4.3044 4.0625 4.83 3.9856 5.3812 4.0613C6.4112 4.2019 7.1875 4.4588 8.02 4.935C8.5281 5.2256 10.4556 6.5862 10.485 6.6744C10.4956 6.7056 10.3556 6.8463 10.1738 6.9869C9.9925 7.1269 9.7469 7.3425 9.6281 7.465C9.51 7.5875 9.3831 7.6856 9.3469 7.6837C9.3106 7.6819 8.9719 7.4569 8.5938 7.1844Z"/></svg>
      <span class="branding-text">Microsoft Quantum Katas</span>
    </div>
    <header id="header" class="header">
      <span class="crumb"></span>
      <span class="badge"></span>
    </header>
    <section id="content" class="content"></section>
    <div id="output" class="output" hidden></div>
    <nav id="actions" class="action-bar"></nav>
    <footer id="progress-bar" class="progress-bar" title="View progress"></footer>

    <script src="${renderJsUri}"></script>
    <script>globalThis.__vscodeApi = acquireVsCodeApi();</script>
    <script src="${webviewClientJsUri}"></script>
  </body>
</html>`;
  }
}
