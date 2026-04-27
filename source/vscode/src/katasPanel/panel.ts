// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Singleton webview panel manager for the Quantum Katas full-view experience.
 *
 * Creates a WebviewPanel, bridges postMessage ↔ LearningService, delegates run /
 * circuit to VS Code commands, uses loadCompilerWorker for exercise checking,
 * listens to LearningService.onDidChangeState for state sync, and watches
 * .navigate.json for tree-view navigation signals.
 */

import * as vscode from "vscode";
import { QscEventTarget } from "qsharp-lang";
import { getExerciseSources } from "qsharp-lang/katas-md";
import type { Exercise } from "qsharp-lang/katas-md";
import { loadCompilerWorker, qsharpExtensionId } from "../common.js";
import { runExerciseInTerminal } from "../run.js";
import type { LearningService } from "../learningService/index.js";
import type { ProgressWatcher } from "../katasProgress/progressReader.js";
import {
  detectKatasWorkspace,
  NAVIGATE_FILE,
} from "../katasProgress/detector.js";
import type { SolutionCheckResult } from "../learningService/types.js";

let instance: KatasPanelManager | undefined;

export class KatasPanelManager {
  private panel: vscode.WebviewPanel | undefined;
  private ready = false;
  private queuedMessages: unknown[] = [];
  private disposables: vscode.Disposable[] = [];
  private navigateWatcher: vscode.FileSystemWatcher | undefined;

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

    this.attachPanel(info);
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

    this.attachPanel(info);
  }

  /**
   * Wire up shared listeners on an already-created panel.
   * Called by both show() (new panel) and restore() (deserialized panel).
   */
  private attachPanel(info: {
    workspaceRoot: vscode.Uri;
    katasRoot: vscode.Uri;
  }): void {
    if (!this.panel) return;

    this.panel.onDidDispose(
      () => {
        this.panel = undefined;
        this.ready = false;
        this.queuedMessages = [];
        this.disposeWatchers();
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

    // Watch .navigate.json for tree-view navigation signals
    this.setupNavigateWatcher(info);
  }

  dispose(): void {
    this.panel?.dispose();
    this.disposeWatchers();
    for (const d of this.disposables) d.dispose();
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
    if (!this.panel) return;
    if (this.ready) {
      this.panel.webview.postMessage(msg);
    } else {
      this.queuedMessages.push(msg);
    }
  }

  private sendState(): void {
    if (!this.service.initialized) return;
    this.sendMessage({ command: "state", state: this.service.getState() });
  }

  /**
   * If the current position is an exercise or example, open the
   * corresponding .qs file in the secondary editor column.
   */
  private async openCurrentFile(): Promise<void> {
    if (!this.service.initialized) return;
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
    if (!this.service.initialized) return;
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

    if (msg.command === "action") {
      await this.handleAction(msg.action);
    }
  }

  private async handleAction(action: string): Promise<void> {
    if (!this.service.initialized) return;

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
        case "hint": {
          const { result } = this.service.getNextHint();
          this.sendResult("hint", result);
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
        case "progress": {
          const progress = this.service.getProgress();
          this.sendResult("progress", progress);
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
    if (!this.service.initialized) return;
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
    if (!this.service.initialized) return;
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
    if (!this.service.initialized)
      return { passed: false, events: [], error: "Service not initialized" };

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

  // ─── .navigate.json watcher (tree-view → panel) ───

  private setupNavigateWatcher(info: {
    workspaceRoot: vscode.Uri;
    katasRoot: vscode.Uri;
  }): void {
    this.disposeWatchers();

    // Handler for the tree-view signal file: read JSON, navigate service,
    // update webview, open the .qs file beside the panel, and delete the
    // signal file to indicate consumption.
    const handleTreeNavigate = async () => {
      if (!this.service.initialized) return;
      const navUri = vscode.Uri.joinPath(info.katasRoot, NAVIGATE_FILE);
      try {
        const bytes = await vscode.workspace.fs.readFile(navUri);
        const data = JSON.parse(new TextDecoder("utf-8").decode(bytes));
        if (data.kataId) {
          this.service.goTo(data.kataId, data.sectionId, data.itemIndex ?? 0);
          this.sendState();
          await this.openCurrentFile();
          this.panel?.reveal(vscode.ViewColumn.One);
        }
        // Delete the file to signal we consumed it
        await vscode.workspace.fs.delete(navUri);
      } catch {
        // File already consumed or missing
      }
    };

    // Tree-view → panel (written by katasProgress tree provider)
    const treePattern = new vscode.RelativePattern(
      info.katasRoot,
      NAVIGATE_FILE,
    );
    this.navigateWatcher =
      vscode.workspace.createFileSystemWatcher(treePattern);
    this.navigateWatcher.onDidCreate(handleTreeNavigate);
    this.navigateWatcher.onDidChange(handleTreeNavigate);

    // DEAD CODE: The .panel-navigate.json MCP server → panel watcher has been
    // removed. Navigation from LM tools now goes through
    // LearningService.onDidChangeState events instead.
  }

  private disposeWatchers(): void {
    this.navigateWatcher?.dispose();
    this.navigateWatcher = undefined;
  }

  // ─── HTML generation ───

  private getWebviewContent(webview: vscode.Webview): string {
    const extensionUri = this.extensionUri;

    function getUri(...parts: string[]): vscode.Uri {
      return webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, ...parts));
    }

    const renderJsUri = getUri(
      "out",
      "learning",
      "web",
      "public",
      "shared",
      "render.js",
    );
    const cssUri = getUri("out", "katasPanel", "katas-webview.css");
    const katexCssUri = getUri("out", "katex", "katex.min.css");

    return /*html*/ `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Quantum Katas</title>
    <link rel="stylesheet" href="${katexCssUri}" />
    <link rel="stylesheet" href="${cssUri}" />
  </head>
  <body>
    <header id="header" class="header">
      <span class="crumb"></span>
      <span class="badge"></span>
    </header>
    <section id="content" class="content"></section>
    <div id="output" class="output" hidden></div>
    <nav id="actions" class="action-bar"></nav>
    <footer id="progress-bar" class="progress-bar"></footer>

    <script src="${renderJsUri}"></script>
    <script>
      (() => {
        "use strict";
        const vscodeApi = acquireVsCodeApi();
        const R = globalThis.KatasRender;

        // ─── DOM slots ───
        const headerEl = document.getElementById("header");
        const crumbEl = headerEl.querySelector(".crumb");
        const badgeEl = headerEl.querySelector(".badge");
        const contentEl = document.getElementById("content");
        const outputEl = document.getElementById("output");
        const actionsEl = document.getElementById("actions");
        const progressEl = document.getElementById("progress-bar");

        let busy = false;
        let lastPositionKey = null;

        // ─── Rendering ───

        function renderHeader(position) {
          const item = position.item;
          const sectionTitle =
            item.type === "exercise"
              ? item.title
              : item.sectionTitle || "";
          crumbEl.textContent = position.kataId + " · " + position.sectionId + " · #" + (position.itemIndex + 1) + (sectionTitle ? " — " + sectionTitle : "");

          badgeEl.className = "badge";
          if (item.type === "exercise") {
            badgeEl.textContent = item.isComplete ? "✔ done" : "exercise";
            badgeEl.classList.add(item.isComplete ? "complete" : "exercise");
          } else if (item.type === "lesson-text") {
            badgeEl.textContent = "lesson";
          } else if (item.type === "lesson-example") {
            badgeEl.textContent = "example";
          } else if (item.type === "lesson-question") {
            badgeEl.textContent = "question";
          } else {
            badgeEl.textContent = item.type;
          }
        }

        function renderContent(position) {
          contentEl.innerHTML = R.renderContentBody(position.item);
          contentEl.scrollTop = 0;
        }

        function showOutput(html, variant) {
          outputEl.className = "output" + (variant ? " " + variant : "");
          outputEl.innerHTML =
            '<button class="out-dismiss" aria-label="Dismiss" title="Dismiss">×</button>' +
            '<div class="out-label">' + labelFor(variant) + "</div>" +
            '<div class="out-body">' + html + "</div>";
          outputEl.hidden = false;
          outputEl
            .querySelector(".out-dismiss")
            .addEventListener("click", clearOutput);
        }

        function labelFor(variant) {
          if (variant === "pass") return "Result";
          if (variant === "fail") return "Result";
          return "Output";
        }

        function clearOutput() {
          outputEl.hidden = true;
          outputEl.innerHTML = "";
        }

        function renderActions(groups) {
          actionsEl.innerHTML = "";
          for (const group of groups) {
            const div = document.createElement("div");
            div.className = "action-group";
            for (const binding of group) {
              if (binding.action === "quit" || binding.action === "menu")
                continue;
              const btn = document.createElement("button");
              btn.textContent = binding.label;
              if (binding.primary) btn.classList.add("primary");
              btn.dataset.action = binding.action;
              btn.disabled = busy;
              btn.addEventListener("click", () =>
                executeAction(binding.action),
              );
              div.appendChild(btn);
            }
            if (div.children.length > 0) actionsEl.appendChild(div);
          }
        }

        function setBusy(b) {
          busy = b;
          for (const btn of actionsEl.querySelectorAll("button"))
            btn.disabled = b;
        }

        function renderProgressBar(progress) {
          try {
            progressEl.innerHTML = R.renderProgressBar(progress);
          } catch {
            progressEl.innerHTML = "";
          }
        }

        function applyState(state) {
          if (!state) return;
          const pos = state.position;
          const key = pos.kataId + ":" + pos.sectionId + ":" + pos.itemIndex;
          if (key !== lastPositionKey) {
            renderHeader(pos);
            renderContent(pos);
            clearOutput();
            lastPositionKey = key;
          } else {
            renderHeader(pos);
          }
          renderActions(state.actions);
          renderProgressBar(state.progress);
          vscodeApi.setState(state);
        }

        function invalidateContent() {
          lastPositionKey = null;
        }

        // ─── Action dispatch ───

        function executeAction(action) {
          if (busy) return;
          setBusy(true);

          var slow = ["run", "circuit", "check"].indexOf(action) >= 0;
          if (slow) showOutput('<div class="loading">Working…</div>');

          vscodeApi.postMessage({ command: "action", action: action });
        }

        // ─── Messages from extension host ───

        window.addEventListener("message", function(event) {
          var msg = event.data;
          switch (msg.command) {
            case "state":
              applyState(msg.state);
              setBusy(false);
              break;
            case "result": {
              var action = msg.action;
              var result = msg.result;
              switch (action) {
                case "next":
                case "back":
                  if (result && !result.moved) {
                    showOutput(
                      action === "next"
                        ? '<div class="success">🎉 You have completed all content!</div>'
                        : '<div class="message">Already at the beginning.</div>',
                    );
                  }
                  break;
                case "check":
                  showOutput(
                    R.renderSolutionCheck(result),
                    result.passed ? "pass" : "fail",
                  );
                  if (result.passed) invalidateContent();
                  break;
                case "hint":
                  showOutput(
                    result
                      ? R.renderHint(result)
                      : '<div class="message">No more hints available.</div>',
                  );
                  break;
                case "reveal-answer":
                  showOutput("<div>" + result + "</div>");
                  break;
                case "solution":
                  showOutput(
                    '<div style="margin-bottom:0.3rem"><strong>Reference Solution</strong></div><pre>' +
                      R.escapeHtml(result) +
                      "</pre>",
                  );
                  break;
                case "progress":
                  showOutput(R.renderProgress(result));
                  break;
                case "run":
                case "circuit":
                  clearOutput();
                  break;
              }
              if (msg.state) applyState(msg.state);
              setBusy(false);
              break;
            }
            case "error":
              showOutput(
                '<div class="fail">Error: ' +
                  R.escapeHtml(msg.message) +
                  "</div>",
                "fail",
              );
              setBusy(false);
              break;
          }
        });

        // Copy-to-clipboard for file paths.
        contentEl.addEventListener("click", function(e) {
          var btn = e.target.closest(".copy-btn");
          if (btn && navigator.clipboard) {
            navigator.clipboard.writeText(btn.dataset.copy);
          }
        });

        // File-path link clicks → open via extension host.
        contentEl.addEventListener("click", function(e) {
          var a = e.target.closest("a.file-path-link");
          if (!a) return;
          e.preventDefault();
          var url = a.getAttribute("href");
          if (url) {
            vscodeApi.postMessage({ command: "openFile", uri: url });
          }
        });

        // Restore cached state immediately for instant render on restart
        const cachedState = vscodeApi.getState();
        if (cachedState) applyState(cachedState);

        // Signal ready
        vscodeApi.postMessage({ command: "ready" });
      })();
    </script>
  </body>
</html>`;
  }
}
