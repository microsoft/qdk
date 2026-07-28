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

  /**
   * True when the active course is a python-notebook course. Those courses
   * use the notebook itself as the primary surface, so the lesson panel is
   * never shown for them.
   */
  private get isPythonNotebook(): boolean {
    return (
      this.service.initialized &&
      this.service.getActiveCourseInfo().kind === "python-notebook"
    );
  }

  /**
   * Show or create the Lesson panel.
   *
   * No-op for python-notebook courses — the notebook is the primary surface
   * there, so there is nothing for the panel to add.
   */
  async show(): Promise<void> {
    const ok = await this.service.tryInitialize();
    if (!ok) {
      vscode.window.showWarningMessage(
        `No QDK Learning workspace detected. Open a folder containing ${LEARNING_FILE} first.`,
      );
      return;
    }

    if (this.isPythonNotebook) {
      return;
    }

    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.One);
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

    if (this.isPythonNotebook) {
      // The active course no longer uses the panel — drop the serialized one.
      panel.dispose();
      return;
    }

    this.panel = panel;

    // Restored panels predate any webview-option changes, so re-apply the
    // current options before re-rendering.
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
        if (!this.panel) {
          return;
        }
        if (this.isPythonNotebook) {
          // Switched into a course that doesn't use the panel.
          this.panel.dispose();
          return;
        }
        this.sendState();
        this.openCurrentCodeEditor().catch(() => {});
      }),
    );
  }

  dispose(): void {
    this.panel?.dispose();
    // Close any lingering code editor tabs.
    if (this.service.initialized) {
      this.service.closeStaleEditorTabs(undefined).catch(() => {});
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
   */
  private async openCurrentCodeEditor(): Promise<void> {
    if (!this.service.initialized) {
      return;
    }
    const fileUri = this.service.getCurrentCodeFileUri();

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
      // TODO (acasey): we might want to rename some of the commands and tools for consistency
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
          const result = await this.service.next("panel");
          this.sendResult("next", result);
          break;
        }
        case "back": {
          const result = await this.service.previous("panel");
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
          const confirmed = await vscode.window.showWarningMessage(
            "Reset this exercise to the original placeholder code? Your current code will be lost.",
            { modal: true },
            "Reset",
          );
          if (confirmed === "Reset") {
            await this.service.resetExercise("panel");
          }
          this.sendState();
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
   * Webview options for the lesson panel.
   *
   * Command URIs are deliberately not enabled: the panel only renders
   * built-in course content, so nothing needs to invoke VS Code commands
   * from inside the webview.
   */
  private getWebviewOptions(): vscode.WebviewPanelOptions &
    vscode.WebviewOptions {
    return {
      enableScripts: true,
      enableFindWidget: true,
      retainContextWhenHidden: true,
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
