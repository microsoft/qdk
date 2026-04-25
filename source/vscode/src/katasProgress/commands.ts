// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { EventType, sendTelemetryEvent } from "../telemetry.js";
import { NAVIGATE_FILE } from "./detector.js";
import type { ProgressWatcher } from "./progressReader.js";
import type { KatasTreeProvider } from "./treeProvider.js";
import type { SectionKind, OverallProgress } from "./types.js";

export interface OpenSectionArgs {
  kataId: string;
  sectionIndex: number;
  kind: SectionKind;
}

function findSection(
  snapshot: OverallProgress | undefined,
  kataId: string,
  sectionIndex: number,
) {
  if (!snapshot) return undefined;
  const kata = snapshot.katas.find((k) => k.id === kataId);
  if (!kata) return undefined;
  const section = kata.sections[sectionIndex];
  if (!section) return undefined;
  return { kata, section };
}

/**
 * Open a kata section. Try to navigate the live MCP widget via
 * `.navigate.json`. If no widget picks it up within 2 seconds, fall
 * back to chat.
 */
async function openSection(
  watcher: ProgressWatcher,
  treeProvider: KatasTreeProvider,
  args: OpenSectionArgs,
): Promise<void> {
  const info = watcher.workspaceInfo;

  // Try in-place navigation via .navigate.json when the katas workspace
  // already exists (implying the MCP server may be active with a live widget).
  if (info?.katasDirExists) {
    const navigated = await tryNavigateSignal(
      info.katasRoot,
      args,
      treeProvider,
    );
    if (navigated) {
      sendTelemetryEvent(
        EventType.KatasPanelAction,
        { action: "navigateWidget" },
        {},
      );
      return;
    }
  }

  await askInChat(watcher, args);
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "openLesson" }, {});
}

function buildChatPrompt(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): string {
  const found = findSection(
    watcher.lastSnapshot,
    args.kataId,
    args.sectionIndex,
  );
  const kataTitle = found?.kata.title ?? args.kataId;
  const sectionTitle = found?.section.title;

  // Use the /quantum-katas slash command so the model loads the skill
  // directly, and include #goto with precise IDs so it can call the
  // tool without fuzzy matching.
  const location = sectionTitle
    ? `the "${sectionTitle}" ${args.kind} in "${kataTitle}"`
    : `"${kataTitle}"`;

  return `/quantum-katas #goto ${args.kataId} ${args.sectionIndex} — Go to ${location}`;
}

async function askInChat(
  watcher: ProgressWatcher,
  args: OpenSectionArgs,
): Promise<void> {
  const prompt = buildChatPrompt(watcher, args);
  try {
    await vscode.commands.executeCommand("workbench.action.chat.open", {
      query: prompt,
      isPartialQuery: false,
      mode: "agent",
    });
  } catch {
    // Older VS Code builds may not accept the options object — fall back
    // to passing a plain string.
    await vscode.commands.executeCommand("workbench.action.chat.open", prompt);
  }
  sendTelemetryEvent(EventType.KatasPanelAction, { action: "askInChat" }, {});
}

// ─── Navigate signal (.navigate.json) ─────────────────────────────────
//
// Write a transient `.navigate.json` file into the katas workspace to
// request in-place navigation of the live MCP widget — avoiding a new
// chat message / LLM round-trip. The server `fs.watch`es for this file
// and the widget polls `check_navigate` to pick it up.

/** Cancel any in-flight navigation attempt before starting a new one. */
let cancelInflight: (() => void) | null = null;

const NAVIGATE_TIMEOUT_MS = 2000;

/**
 * Try to navigate the live widget by writing `.navigate.json`.
 * Resolves `true` if the server consumed the file within the timeout,
 * `false` otherwise (caller should fall back to chat).
 */
async function tryNavigateSignal(
  katasRoot: vscode.Uri,
  args: OpenSectionArgs,
  treeProvider: KatasTreeProvider,
): Promise<boolean> {
  // Cancel any previous in-flight attempt.
  cancelInflight?.();

  const navFileUri = vscode.Uri.joinPath(katasRoot, NAVIGATE_FILE);
  const payload = JSON.stringify({
    kataId: args.kataId,
    sectionIndex: args.sectionIndex,
    itemIndex: 0,
  });

  let settled = false;
  let resolvePromise: (consumed: boolean) => void;
  const promise = new Promise<boolean>((r) => {
    resolvePromise = r;
  });

  let backupTimer: ReturnType<typeof setTimeout> | undefined;
  let timeoutTimer: ReturnType<typeof setTimeout> | undefined;

  function settle(consumed: boolean) {
    if (settled) return;
    settled = true;
    cancelInflight = null;
    fsWatcher?.dispose();
    clearTimeout(backupTimer);
    clearTimeout(timeoutTimer);
    treeProvider.clearNavigating();
    resolvePromise(consumed);
  }

  // 1. Set up the FileSystemWatcher BEFORE writing the file to avoid the
  //    race where the server deletes it before we start listening.
  const pattern = new vscode.RelativePattern(katasRoot, NAVIGATE_FILE);
  const fsWatcher = vscode.workspace.createFileSystemWatcher(pattern);
  fsWatcher.onDidDelete(() => settle(true));

  cancelInflight = () => settle(false);

  // 2. Write the navigate signal file.
  try {
    await vscode.workspace.fs.writeFile(
      navFileUri,
      new TextEncoder().encode(payload),
    );
  } catch (err) {
    log.warn(`[katasProgress] failed to write navigate signal: ${err}`);
    settle(false);
    return promise;
  }

  // 3. Show spinner on the tree view.
  treeProvider.setNavigating(args.kataId, args.sectionIndex);

  // 4. Backup stat check — FileSystemWatcher on Windows can miss delete
  //    events. A single check partway through the timeout catches this.
  backupTimer = setTimeout(async () => {
    if (settled) return;
    try {
      await vscode.workspace.fs.stat(navFileUri);
      // File still exists — keep waiting for watcher / timeout.
    } catch {
      // File is gone — the server consumed it.
      settle(true);
    }
  }, 500);

  // 5. Timeout fallback — delete the file and fall back to chat.
  timeoutTimer = setTimeout(async () => {
    if (settled) return;
    try {
      await vscode.workspace.fs.delete(navFileUri);
    } catch {
      // File may already be gone.
    }
    settle(false);
  }, NAVIGATE_TIMEOUT_MS);

  return promise;
}

export function registerKatasCommands(
  context: vscode.ExtensionContext,
  watcher: ProgressWatcher,
  treeProvider: KatasTreeProvider,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.katasRefresh", async () => {
      sendTelemetryEvent(EventType.KatasPanelAction, { action: "refresh" }, {});
      await watcher.refresh();
    }),

    vscode.commands.registerCommand("qsharp-vscode.katasContinue", async () => {
      sendTelemetryEvent(
        EventType.KatasPanelAction,
        { action: "continue" },
        {},
      );
      const snap = watcher.lastSnapshot;
      const pos = snap?.currentPosition;
      if (snap && pos && pos.kataId) {
        const found = findSection(snap, pos.kataId, pos.sectionIndex);
        if (found) {
          await openSection(watcher, treeProvider, {
            kataId: pos.kataId,
            sectionIndex: pos.sectionIndex,
            kind: found.section.kind,
          });
          return;
        }
      }
      // No position recorded yet — open chat with a generic start prompt.
      await vscode.commands.executeCommand("workbench.action.chat.open", {
        query: "/quantum-katas Let's start the Quantum Katas.",
        isPartialQuery: false,
        mode: "agent",
      });
    }),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasOpenSection",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) return;
        await openSection(watcher, treeProvider, args);
      },
    ),

    vscode.commands.registerCommand(
      "qsharp-vscode.katasAskInChat",
      async (input: unknown) => {
        const args = normalizeSectionArgs(input);
        if (!args) return;
        await askInChat(watcher, args);
      },
    ),
  );
}

function normalizeSectionArgs(input: unknown): OpenSectionArgs | undefined {
  if (!input || typeof input !== "object") return undefined;
  const obj = input as Record<string, unknown>;

  // Tree node shape — see treeProvider.ts `KatasNode`.
  if (obj.kind === "section") {
    const kataId = obj.kataId as string | undefined;
    const section = obj.section as
      | { index?: number; kind?: SectionKind }
      | undefined;
    if (
      kataId &&
      section &&
      typeof section.index === "number" &&
      section.kind
    ) {
      return { kataId, sectionIndex: section.index, kind: section.kind };
    }
  }
  if (obj.kind === "kata") {
    const kata = obj.kata as
      | { id?: string; sections?: Array<{ kind?: SectionKind }> }
      | undefined;
    if (kata?.id && kata.sections && kata.sections.length > 0) {
      const firstKind = kata.sections[0].kind ?? "lesson";
      return { kataId: kata.id, sectionIndex: 0, kind: firstKind };
    }
  }

  // Already in `OpenSectionArgs` shape.
  const kataId = obj.kataId as string | undefined;
  const sectionIndex = obj.sectionIndex as number | undefined;
  const kind = obj.kind as SectionKind | undefined;
  if (
    kataId &&
    typeof sectionIndex === "number" &&
    (kind === "lesson" || kind === "exercise")
  ) {
    return { kataId, sectionIndex, kind };
  }
  return undefined;
}
