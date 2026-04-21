// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { loadCatalog } from "./catalog.js";
import {
  detectKatasWorkspace,
  KatasWorkspaceInfo,
  KATAS_SUBFOLDER,
  PROGRESS_FILE,
} from "./detector.js";
import type {
  CatalogKata,
  KataProgress,
  OverallProgress,
  ProgressFileData,
  SectionProgress,
} from "./types.js";

const KATAS_DETECTED_CONTEXT = "qsharp-vscode.katasDetected";

function completionKey(kataId: string, sectionIndex: number): string {
  return `${kataId}__${sectionIndex}`;
}

function emptyProgressFile(): ProgressFileData {
  return {
    version: 1,
    position: { kataId: "", sectionIndex: 0, itemIndex: 0 },
    completions: {},
    startedAt: new Date().toISOString(),
  };
}

/**
 * Compute `OverallProgress` by joining the catalog with a progress-file snapshot.
 * Mirrors `ProgressManager.getOverallProgress` in the learning server.
 */
function computeOverallProgress(
  catalog: CatalogKata[],
  data: ProgressFileData,
): OverallProgress {
  let totalSections = 0;
  let completedSections = 0;

  const katas: KataProgress[] = catalog.map((kata) => {
    const sections: SectionProgress[] = kata.sections.map((s, i) => {
      const key = completionKey(kata.id, i);
      const completion = data.completions[key];
      return {
        ...s,
        index: i,
        isComplete: completion != null,
        completedAt: completion?.completedAt,
      };
    });
    const completed = sections.filter((s) => s.isComplete).length;
    totalSections += sections.length;
    completedSections += completed;
    return {
      id: kata.id,
      title: kata.title,
      total: sections.length,
      completed,
      sections,
    };
  });

  return {
    katas,
    currentPosition: data.position,
    stats: { totalSections, completedSections },
  };
}

/**
 * Watches the detected katas workspace for progress changes and publishes
 * `OverallProgress` snapshots. Re-runs detection when workspace folders
 * or the `Q#.learning.workspaceRoot` setting change.
 */
export class ProgressWatcher implements vscode.Disposable {
  private readonly changeEmitter = new vscode.EventEmitter<OverallProgress>();
  readonly onDidChange = this.changeEmitter.event;

  private disposables: vscode.Disposable[] = [];
  private fileWatcher: vscode.FileSystemWatcher | undefined;
  private currentInfo: KatasWorkspaceInfo | undefined;
  private latest: OverallProgress | undefined;
  private reloadScheduled = false;

  constructor() {
    this.disposables.push(
      vscode.workspace.onDidChangeWorkspaceFolders(() => this.scheduleReload()),
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration("Q#.learning.workspaceRoot")) {
          this.scheduleReload();
        }
      }),
    );
  }

  get workspaceInfo(): KatasWorkspaceInfo | undefined {
    return this.currentInfo;
  }

  get lastSnapshot(): OverallProgress | undefined {
    return this.latest;
  }

  /** Force a fresh detection + reload. Safe to call many times. */
  async refresh(): Promise<void> {
    await this.reload();
  }

  /**
   * Schedule a reload on the next microtask, coalescing rapid config/workspace
   * change events into a single reload.
   */
  private scheduleReload(): void {
    if (this.reloadScheduled) return;
    this.reloadScheduled = true;
    queueMicrotask(() => {
      this.reloadScheduled = false;
      void this.reload();
    });
  }

  private async reload(): Promise<void> {
    const info = await detectKatasWorkspace();
    const detected = info !== undefined;

    // Update the context key whenever detection state changes.
    await vscode.commands.executeCommand(
      "setContext",
      KATAS_DETECTED_CONTEXT,
      detected,
    );

    // Rewire the file watcher if the path changed.
    const newPath = info?.progressFile.fsPath;
    const oldPath = this.currentInfo?.progressFile.fsPath;
    if (newPath !== oldPath) {
      this.fileWatcher?.dispose();
      this.fileWatcher = undefined;
      if (info) {
        // Pattern is relative to the containing workspace folder when possible,
        // but an absolute RelativePattern works for any path.
        const pattern = new vscode.RelativePattern(
          info.katasRoot,
          PROGRESS_FILE,
        );
        const watcher = vscode.workspace.createFileSystemWatcher(pattern);
        const onEvent = () => void this.readAndEmit();
        watcher.onDidChange(onEvent);
        watcher.onDidCreate(onEvent);
        watcher.onDidDelete(onEvent);
        this.fileWatcher = watcher;
      }
    }
    this.currentInfo = info;

    await this.readAndEmit();
  }

  private async readAndEmit(): Promise<void> {
    const info = this.currentInfo;
    let catalog: CatalogKata[];
    try {
      catalog = await loadCatalog();
    } catch (err) {
      log.warn(`[katasProgress] failed to load catalog: ${err}`);
      catalog = [];
    }

    let data = emptyProgressFile();
    if (info) {
      try {
        const bytes = await vscode.workspace.fs.readFile(info.progressFile);
        const raw = new TextDecoder("utf-8").decode(bytes);
        const parsed = JSON.parse(raw) as ProgressFileData;
        if (parsed && parsed.version === 1) data = parsed;
      } catch {
        // File missing or corrupt — use an empty snapshot.
      }
    }

    this.latest = computeOverallProgress(catalog, data);
    this.changeEmitter.fire(this.latest);
  }

  /**
   * Kick off the initial detect + load. Call once after construction.
   * Emits an update on completion.
   */
  async start(): Promise<void> {
    await this.reload();
  }

  dispose(): void {
    this.fileWatcher?.dispose();
    this.changeEmitter.dispose();
    for (const d of this.disposables) d.dispose();
    this.disposables = [];
  }
}

// Re-export these for callers that want to locate files without importing
// detector directly.
export { KATAS_SUBFOLDER, PROGRESS_FILE };
