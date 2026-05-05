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
import { loadCompilerWorker, qsharpExtensionId } from "../common.js";
import { getExerciseSourceFiles } from "./catalog.js";
import { runExerciseInTerminal } from "../run.js";
import { LEARNING_FILE, LEARNING_CONTENT_FOLDER } from "./constants.js";
import type { LearningService } from "./index.js";
import type {
  SolutionCheckResult,
  OutputEvent,
  HostToWebviewMessage,
  WebviewToHostMessage,
} from "./types.js";

let instance: KatasPanelManager | undefined;

/**
 * Register the WebviewPanelSerializer so the Katas panel persists across
 * VS Code restarts.
 */
export function registerKatasPanelSerializer(
  context: vscode.ExtensionContext,
  learningService: LearningService,
): void {
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer("qsharp-katas", {
      async deserializeWebviewPanel(panel: vscode.WebviewPanel) {
        const manager = KatasPanelManager.getInstance(
          context.extensionUri,
          learningService,
        );
        await manager.restore(panel);
      },
    }),
  );
}

export class KatasPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly extensionUri: vscode.Uri,
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
    // Reveal existing panel immediately if present and not on an example.
    if (this.panel && this.service.initialized) {
      const pos = this.service.getPosition();
      if (pos.content.type !== "example") {
        this.panel.reveal(vscode.ViewColumn.One);
        return;
      }
    }

    // Initialize the shared service (detects workspace from disk).
    const ok = await this.service.ensureInitialized();
    if (!ok) {
      vscode.window.showWarningMessage(
        `No Quantum Katas workspace detected. Open a folder containing ${LEARNING_FILE} first.`,
      );
      return;
    }

    // Example activities open the file directly — no panel needed.
    const pos = this.service.getPosition();
    if (pos.content.type === "example") {
      this.panel?.dispose();
      this.closeStaleLearningTabs(undefined).catch(() => {});
      const fileUri = vscode.Uri.file(pos.content.filePath);
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.One,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
      return;
    }

    // Reveal existing panel if we already have one.
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
      return;
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
          vscode.Uri.joinPath(this.extensionUri, "resources"),
          this.service.getKatasRoot(),
        ],
      },
    );

    this.panel.iconPath = vscode.Uri.joinPath(
      this.extensionUri,
      "resources",
      "mobius.svg",
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
    // If already initialized, just refresh progress from disk.
    // Otherwise, perform full initialization.
    const wasInitialized = this.service.initialized;
    const ok = await this.service.ensureInitialized();
    if (!ok) {
      // Workspace no longer available — dispose the stale panel
      panel.dispose();
      return;
    }
    if (wasInitialized) {
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
      this.service.onDidChangeProgress(async () => {
        if (this.service.initialized && this.panel) {
          this.sendState();
        }
      }),
    );

    // Listen for state changes from LM tools (navigation, check, etc.)
    this.disposables.push(
      this.service.onDidChangeState(() => {
        if (!this.service.initialized) {
          return;
        }

        const pos = this.service.getPosition();
        if (pos.content.type === "example") {
          // Example activities don't use the lesson panel — close it and
          // open the file directly in the primary editor column.
          this.panel?.dispose();
          this.closeStaleLearningTabs(undefined).catch(() => {});
          const fileUri = vscode.Uri.file(pos.content.filePath);
          vscode.commands.executeCommand("vscode.open", fileUri, {
            viewColumn: vscode.ViewColumn.One,
            preview: false,
          } satisfies vscode.TextDocumentShowOptions);
          return;
        }

        if (this.panel) {
          this.sendState();
          this.openCurrentFile().catch(() => {});
        }
      }),
    );
  }

  dispose(): void {
    this.panel?.dispose();
    // Close any lingering learning editor tabs
    if (this.service.initialized) {
      this.closeStaleLearningTabs(undefined).catch(() => {});
    }
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
    instance = undefined;
  }

  // ─── Singleton accessor ───

  static getInstance(
    extensionUri: vscode.Uri,
    learningService: LearningService,
  ): KatasPanelManager {
    if (!instance) {
      instance = new KatasPanelManager(extensionUri, learningService);
    }
    return instance;
  }

  // ─── Message bridge ───

  private sendMessage(msg: HostToWebviewMessage): void {
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
   * Closes any previously-opened kata editor tabs that are no longer current.
   */
  private async openCurrentFile(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const pos = this.service.getPosition();
    let fileUri: vscode.Uri | undefined;
    if (pos.content.type === "exercise") {
      fileUri = this.service.getExerciseFileUri();
    } else if (pos.content.type === "lesson-example") {
      fileUri = this.service.getExampleFileUri();
    }

    // Close stale learning editor tabs that don't match the current file
    await this.closeStaleLearningTabs(fileUri);

    if (fileUri) {
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
    }
  }

  /**
   * Close any open editor tabs managed by the learning feature (under the
   * katas workspace root or any `qdk-learning-content` folder) that don't
   * match {@link keepUri}. When {@link keepUri} is undefined (e.g. on
   * lesson-text or question), all managed tabs are closed.
   */
  private async closeStaleLearningTabs(
    keepUri: vscode.Uri | undefined,
  ): Promise<void> {
    const managedRoots: string[] = [this.service.getKatasRoot().toString()];

    // Also treat files under qdk-learning-content folders as managed tabs.
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
      managedRoots.push(
        vscode.Uri.joinPath(folder.uri, LEARNING_CONTENT_FOLDER).toString(),
      );
    }

    const keepStr = keepUri?.toString();

    const staleTabs: vscode.Tab[] = [];
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        if (tab.input instanceof vscode.TabInputText) {
          const tabUriStr = tab.input.uri.toString();
          const isManaged = managedRoots.some((root) =>
            tabUriStr.startsWith(root),
          );
          if (isManaged && tabUriStr !== keepStr) {
            staleTabs.push(tab);
          }
        }
      }
    }
    if (staleTabs.length > 0) {
      await vscode.window.tabGroups.close(staleTabs);
    }
  }

  private sendResult<
    M extends Extract<HostToWebviewMessage, { command: "result" }>,
  >(action: M["action"], result: M["result"]): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({
      command: "result",
      action,
      result,
      state: this.service.getState(),
    } as HostToWebviewMessage);
  }

  private sendError(message: string): void {
    this.sendMessage({ command: "error", message });
  }

  private async handleMessage(msg: WebviewToHostMessage): Promise<void> {
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
      await vscode.commands.executeCommand("qsharp-vscode.learningTree.focus");
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

    if (pos.content.type === "exercise") {
      const exercise = this.service.resolveExercise();
      const userCode = await this.service.readUserCode();
      const sources = await getExerciseSourceFiles(exercise);
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
    } else if (pos.content.type === "lesson-example") {
      const fileUri = this.service.getExampleFileUri();
      this.service.markExampleRun(pos.content.id);

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
    if (pos.content.type === "exercise") {
      fileUri = this.service.getExerciseFileUri();
    } else if (pos.content.type === "lesson-example") {
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
    if (pos.content.type !== "exercise") {
      throw new Error("Current item is not an exercise.");
    }

    // Get the exercise object for exercise sources
    const exercise = this.service.resolveExercise();
    const userCode = await this.service.readUserCode();
    const sources = await getExerciseSourceFiles(exercise);

    // Use the extension's compiler worker
    const worker = loadCompilerWorker(this.extensionUri);
    const eventTarget = new QscEventTarget(true);

    try {
      const passed = await worker.checkExerciseSolution(
        userCode,
        sources,
        eventTarget,
      );

      const events: OutputEvent[] = [];
      const results = eventTarget.getResults();
      for (const r of results) {
        for (const evt of r.events) {
          switch (evt.type) {
            case "Message":
              events.push({ type: "message", message: evt.message });
              break;
            case "DumpMachine":
              events.push({ type: "dump", dump: { state: evt.state } });
              break;
            case "Matrix":
              events.push({ type: "matrix", matrix: { matrix: evt.matrix } });
              break;
          }
        }
        // Extract compiler/runtime errors from the shot result
        if (!r.success && typeof r.result !== "string") {
          const errors = r.result?.errors ?? [];
          for (const e of errors) {
            const msg = e.diagnostic?.message ?? String(e);
            if (msg) {
              events.push({ type: "message", message: msg });
            }
          }
        }
      }

      if (passed) {
        await this.service.markExerciseComplete(pos);
      }

      return {
        passed,
        events,
        error: passed
          ? undefined
          : events.length === 0
            ? "Solution check failed."
            : undefined,
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

    const webviewClientJsUri = getUri(
      "out",
      "learning",
      "webview",
      "webview-client.js",
    );
    const cssUri = getUri("out", "learning", "webview", "webview.css");
    const katexCssUri = getUri("out", "katex", "katex.min.css");
    const codiconCssUri = getUri("out", "katex", "codicon.css");
    const mobiusUri = getUri("resources", "mobius.svg");

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Quantum Katas</title>
    <link rel="stylesheet" href="${katexCssUri}" />
    <link rel="stylesheet" href="${codiconCssUri}" />
    <link rel="stylesheet" href="${cssUri}" />
  </head>
  <body data-mobius-uri="${mobiusUri}">
    <script src="${webviewClientJsUri}"></script>
  </body>
</html>`;
  }
}
