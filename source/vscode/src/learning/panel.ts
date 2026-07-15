// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Webview panel manager for the lesson panel.
 * This panel displays the lesson content and the action buttons to interact with
 * the learning feature.
 */

import * as vscode from "vscode";
import { qsharpExtensionId } from "../common.js";
import { LEARNING_FILE, LEARNING_TREE_VIEW_ID } from "./constants.js";
import type { LearningService } from "./service.js";
import type { TelemetrySource } from "./types.js";
import type {
  HostToWebviewMessage,
  ResultAction,
  ResultPayload,
  WebviewToHostMessage,
} from "./types.js";

/**
 * Register the WebviewPanelSerializer so the Lesson panel persists across
 * VS Code restarts.
 */
export function registerLessonPanelSerializer(
  context: vscode.ExtensionContext,
  manager: LessonPanelManager,
): void {
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer("qsharp-lesson", {
      async deserializeWebviewPanel(panel: vscode.WebviewPanel) {
        await manager.restore(panel);
      },
    }),
  );
}

export class LessonPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly service: LearningService,
  ) {}

  /** True when the active course is a python-notebook course. */
  private get isPythonNotebook(): boolean {
    return (
      this.service.initialized &&
      this.service.getActiveCourseInfo().kind === "python-notebook"
    );
  }

  /**
   * Show or create the Lesson panel.
   */
  async show(): Promise<void> {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
      return;
    }

    const ok = await this.service.tryInitialize();
    if (!ok) {
      vscode.window.showWarningMessage(
        `No QDK Learning workspace detected. Open a folder containing ${LEARNING_FILE} first.`,
      );
      return;
    }

    this.panel = vscode.window.createWebviewPanel(
      "qsharp-lesson",
      "Lesson",
      { viewColumn: vscode.ViewColumn.One, preserveFocus: false },
      this.getWebviewOptions(),
    );

    this.panel.iconPath = {
      light: vscode.Uri.joinPath(
        this.extensionUri,
        "resources",
        "mobius-light.svg",
      ),
      dark: vscode.Uri.joinPath(
        this.extensionUri,
        "resources",
        "mobius-dark.svg",
      ),
    };

    // Generate and set HTML
    this.panel.webview.html = this.getWebviewContent(this.panel.webview);

    this.attachPanel();
  }

  /**
   * Restore a serialized Lesson panel after VS Code restarts.
   * Re-initializes the service from disk before reconnecting the webview.
   */
  async restore(panel: vscode.WebviewPanel): Promise<void> {
    const ok = await this.service.tryInitialize();
    if (!ok) {
      // Workspace no longer available — dispose the stale panel.
      panel.dispose();
      return;
    }

    this.panel = panel;

    // Restored panels predate any webview-option changes, so re-apply the
    // current options (e.g. allowlisted command URIs) before re-rendering.
    this.panel.webview.options = this.getWebviewOptions();

    // Re-set HTML — webview resource URIs change across sessions.
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

    // Listen for state changes from the service.
    this.disposables.push(
      this.service.onDidChangeState(() => {
        if (this.panel) {
          this.sendState();
          this.openCurrentCodeEditor().catch(() => {});
        }
      }),
    );
  }

  dispose(): void {
    this.panel?.dispose();
    // Close any lingering code editor tabs.
    if (this.service.initialized) {
      this.closeStaleEditorTabs(undefined).catch(() => {});
    }
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }

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
    this.sendMessage({
      command: "state",
      state: this.service.getStateForPanel(),
    });
  }

  /**
   * Show the panel and execute the "check solution" action, sending the
   * result to the webview so it renders the same output as clicking the
   * panel's own Check button. Returns whether the solution passed.
   */
  async checkAndShowResult(): Promise<boolean> {
    await this.show();
    return this.checkSolutionAndSendResult();
  }

  /**
   * If the current position is an exercise or example, open the
   * corresponding .qs file in the secondary editor column.
   * Closes any previously-opened code editor tabs that are no longer current.
   */
  private async openCurrentCodeEditor(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const fileUri = this.service.getCurrentCodeFileUri();

    // Close stale editor tabs that don't match the current file.
    await this.closeStaleEditorTabs(fileUri);

    if (fileUri) {
      // Set a left/right two-column layout so the lesson panel stays in the
      // first editor group and the code file opens beside it in the second.
      await vscode.commands.executeCommand("vscode.setEditorLayout", {
        orientation: 0,
        groups: [{ size: 0.4 }, { size: 0.6 }],
      });
      await vscode.commands.executeCommand("vscode.open", fileUri, {
        viewColumn: vscode.ViewColumn.Two,
        preview: false,
      } satisfies vscode.TextDocumentShowOptions);
    }
  }

  /**
   * Close any open editor tabs whose URI falls under the QDK Learning root
   * that don't match {@link keepUri}.
   * When {@link keepUri} is undefined, all code editor tabs are closed.
   */
  private async closeStaleEditorTabs(
    keepUri: vscode.Uri | undefined,
  ): Promise<void> {
    const learningRoot = this.service.learningContentRoot.toString();
    const keepStr = keepUri?.toString();

    const staleTabs: vscode.Tab[] = [];
    for (const group of vscode.window.tabGroups.all) {
      for (const tab of group.tabs) {
        if (tab.input instanceof vscode.TabInputText) {
          const tabUriStr = tab.input.uri.toString();
          if (tabUriStr.startsWith(learningRoot) && tabUriStr !== keepStr) {
            staleTabs.push(tab);
          }
        }
      }
    }
    if (staleTabs.length > 0) {
      await vscode.window.tabGroups.close(staleTabs);
    }
  }

  private sendResult<Action extends ResultAction>(
    action: Action,
    result: ResultPayload<Action>,
  ): void {
    if (!this.service.initialized) {
      return;
    }
    this.sendMessage({
      command: "result",
      action,
      result,
      state: this.service.getStateForPanel(),
    } as Extract<HostToWebviewMessage, { command: "result" }>);
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
      await this.openCurrentCodeEditor();
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
      await vscode.commands.executeCommand(`${LEARNING_TREE_VIEW_ID}.focus`);
      return;
    }

    if (msg.command === "switchCourse") {
      await this.service.switchCourse(msg.courseId, "panel");
      this.sendState();
      return;
    }

    if (msg.command === "courseInfo") {
      await vscode.commands.executeCommand(
        "qsharp-vscode.learningCourseInfo",
        msg.courseId
          ? { kind: "course", descriptor: { id: msg.courseId } }
          : undefined,
      );
      return;
    }

    if (msg.command === "browseCourses") {
      await vscode.commands.executeCommand(
        "qsharp-vscode.learningSwitchCourse",
      );
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
          const result = this.isPythonNotebook
            ? await this.service.nextUnit("panel")
            : await this.service.next("panel");
          this.sendResult("next", result);
          break;
        }
        case "back": {
          const result = this.isPythonNotebook
            ? await this.service.previousUnit("panel")
            : await this.service.previous("panel");
          this.sendResult("back", result);
          break;
        }
        case "run": {
          await this.executeRun();
          this.sendResult("run", {});
          this.service.sendActivityActionTelemetry("run", "panel");
          break;
        }
        case "check": {
          await this.checkSolutionAndSendResult("panel");
          break;
        }
        case "reset": {
          // TODO (acasey): is this text appropriate for all course flavors?
          const confirmed = await vscode.window.showWarningMessage(
            "Reset this unit to the original notebook? Your current work will be lost.",
            { modal: true },
            "Reset",
          );
          if (confirmed === "Reset") {
            await this.service.resetExercise("panel");
          }
          this.sendState();
          break;
        }
        case "open-notebook": {
          await this.openCourseNotebook();
          break;
        }
        default:
          this.sendError(`Unknown action: ${action}`);
      }
    } catch (err: unknown) {
      this.sendError(err instanceof Error ? err.message : String(err));
    }
  }

  private async executeRun(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }

    const pos = this.service.getCurrentActivity();
    if (pos.content.type !== "lesson-example") {
      throw new Error("Current item cannot be run.");
    }

    const fileUri = this.service.getExampleFileUri();
    await this.service.markExampleRun();

    await this.openCurrentCodeEditor();
    await vscode.commands.executeCommand(
      `${qsharpExtensionId}.runProgram`,
      fileUri,
    );
  }

  /**
   * Open the current unit's notebook in the Jupyter editor (column 2).
   */
  private async openCourseNotebook(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const notebookUri = this.service.getCurrentCodeFileUri();
    if (!notebookUri) {
      return;
    }
    // Set a two-column layout: lesson panel left, notebook right.
    await vscode.commands.executeCommand("vscode.setEditorLayout", {
      orientation: 0,
      groups: [{ size: 0.35 }, { size: 0.65 }],
    });
    await vscode.commands.executeCommand(
      "vscode.openWith",
      notebookUri,
      "jupyter-notebook",
      { viewColumn: vscode.ViewColumn.Two, preview: false },
    );
  }

  /**
   * Webview options for the lesson panel.
   *
   * `enableCommandUris` is restricted to an allowlist so author-supplied
   * markdown (drop-in courses) can link to specific learning commands — e.g.
   * a "Check my environment" button in a unit overview that runs the
   * environment check — without granting the ability to invoke arbitrary VS
   * Code commands.
   */
  private getWebviewOptions(): vscode.WebviewPanelOptions &
    vscode.WebviewOptions {
    return {
      enableScripts: true,
      enableFindWidget: true,
      retainContextWhenHidden: true,
      enableCommandUris: ["qsharp-vscode.learningCheckEnvironment"],
      localResourceRoots: [
        vscode.Uri.joinPath(this.extensionUri, "out"),
        vscode.Uri.joinPath(this.extensionUri, "resources"),
        this.service.learningContentRoot,
      ],
    };
  }

  private getWebviewContent(webview: vscode.Webview): string {
    const extensionUri = this.extensionUri;
    const cspSource = webview.cspSource;

    function getUri(...parts: string[]): vscode.Uri {
      return webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, ...parts));
    }

    const webviewClientJsUri = getUri(
      "out",
      "learning",
      "webview",
      "webview-client.js",
    );
    const cssUri = getUri("out", "learning", "webview", "webview-client.css");
    const katexCssUri = getUri("out", "katex", "katex.min.css");
    const codiconCssUri = getUri("out", "katex", "codicon.css");
    const mobiusUri = getUri("resources", "mobius.svg");

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; img-src ${cspSource}; style-src ${cspSource} 'unsafe-inline'; font-src ${cspSource}; script-src ${cspSource};" />
    <title>Lesson</title>
    <link rel="stylesheet" href="${katexCssUri}" />
    <link rel="stylesheet" href="${codiconCssUri}" />
    <link rel="stylesheet" href="${cssUri}" />
  </head>
  <body data-mobius-uri="${mobiusUri}">
    <script src="${webviewClientJsUri}"></script>
  </body>
</html>`;
  }

  private async checkSolutionAndSendResult(
    source?: TelemetrySource,
  ): Promise<boolean> {
    const { result } = await this.service.checkSolution(source);
    this.sendMessage({
      command: "result",
      action: "check",
      result,
      state: this.service.getStateForPanel(),
    });
    return result.passed;
  }
}
