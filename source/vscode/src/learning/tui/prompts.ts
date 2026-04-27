// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { createInterface } from "node:readline";
import type { KataSummary, Action, ActionGroup } from "../server/index.js";

export type { Action } from "../server/index.js";

// ─── Low-level helpers ───

/** Wait for a single keypress and return it. */
function waitForKey(): Promise<string> {
  return new Promise((resolve) => {
    const wasRaw = process.stdin.isRaw;
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.once("data", (data) => {
      process.stdin.setRawMode(wasRaw);
      process.stdin.pause();
      const key = data.toString();
      // Ctrl-C
      if (key === "\x03") {
        process.exit(0);
      }
      resolve(key);
    });
  });
}

/** Read a line of text from stdin (for free-form input). */
function readLine(prompt: string): Promise<string> {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => {
    rl.question(prompt, (answer) => {
      rl.close();
      resolve(answer);
    });
  });
}

/** Print a key legend, one group per line. */
function printKeyLegend(groups: ActionGroup[]): void {
  for (const group of groups) {
    if (group.length > 0) {
      console.log(
        group.map((b) => `\x1b[1m[${b.key}]\x1b[0m ${b.label}`).join("  "),
      );
    }
  }
}

// ─── Public prompts ───

export async function promptAction(groups: ActionGroup[]): Promise<Action> {
  const keyMap = new Map<string, Action>();
  for (const group of groups) {
    for (const b of group) {
      keyMap.set(b.key === "space" ? " " : b.key, b.action);
    }
  }

  printKeyLegend(groups);

  while (true) {
    const key = await waitForKey();
    const action = keyMap.get(key.toLowerCase());
    if (action) return action;
  }
}

export async function promptKataJump(
  katas: KataSummary[],
): Promise<{ kataId: string } | null> {
  console.log("  Jump to kata (enter number, or 0 to cancel):");
  for (let i = 0; i < katas.length; i++) {
    const k = katas[i];
    console.log(
      `  ${i + 1}. ${k.title} (${k.completedCount}/${k.sectionCount})`,
    );
  }

  const answer = await readLine("  > ");
  const idx = parseInt(answer, 10);
  if (isNaN(idx) || idx < 1 || idx > katas.length) return null;
  return { kataId: katas[idx - 1].id };
}

export async function promptQuestion(): Promise<string> {
  return readLine("  Question: ");
}

export async function promptShots(): Promise<number> {
  const answer = await readLine("  Shots [100]: ");
  if (!answer.trim()) return 100;
  const n = parseInt(answer, 10);
  return isNaN(n) || n < 1 ? 1 : n;
}
