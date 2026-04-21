// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join, dirname } from "node:path";
import type { ProgressFileData, OverallProgress, KataProgress, SectionProgress, Kata } from "./types.js";

const PROGRESS_FILE = ".katas-progress.json";

export class ProgressManager {
  private filePath: string;
  private data: ProgressFileData;
  private katas: Kata[] = [];

  constructor(workspacePath: string) {
    this.filePath = join(workspacePath, PROGRESS_FILE);
    this.data = this.freshData();
  }

  private freshData(): ProgressFileData {
    return {
      version: 1,
      position: { kataId: "", sectionIndex: 0, itemIndex: 0 },
      completions: {},
      startedAt: new Date().toISOString(),
    };
  }

  /** Load progress from disk. Handles missing/corrupt file gracefully. */
  async load(katas: Kata[]): Promise<void> {
    this.katas = katas;
    try {
      const raw = await readFile(this.filePath, "utf-8");
      const parsed = JSON.parse(raw) as ProgressFileData;
      if (parsed.version === 1) {
        this.data = parsed;
        // Ensure position references a valid kata
        if (katas.length > 0 && !katas.find((k) => k.id === this.data.position.kataId)) {
          this.data.position = { kataId: katas[0].id, sectionIndex: 0, itemIndex: 0 };
        }
      }
    } catch {
      // File missing or corrupt — start fresh
      this.data = this.freshData();
      if (katas.length > 0) {
        this.data.position.kataId = katas[0].id;
      }
    }
  }

  /** Persist progress to disk */
  async save(): Promise<void> {
    await mkdir(dirname(this.filePath), { recursive: true });
    await writeFile(this.filePath, JSON.stringify(this.data, null, 2), "utf-8");
  }

  /** Key for the completions map */
  private completionKey(kataId: string, sectionIndex: number): string {
    return `${kataId}__${sectionIndex}`;
  }

  isComplete(kataId: string, sectionIndex: number): boolean {
    return this.completionKey(kataId, sectionIndex) in this.data.completions;
  }

  markComplete(kataId: string, sectionIndex: number): void {
    const key = this.completionKey(kataId, sectionIndex);
    if (!(key in this.data.completions)) {
      this.data.completions[key] = { completedAt: new Date().toISOString() };
    }
  }

  getPosition(): { kataId: string; sectionIndex: number; itemIndex: number } {
    return { ...this.data.position };
  }

  setPosition(kataId: string, sectionIndex: number, itemIndex: number): void {
    this.data.position = { kataId, sectionIndex, itemIndex };
  }

  reset(kataId?: string): void {
    if (kataId) {
      // Reset only the given kata
      const kata = this.katas.find((k) => k.id === kataId);
      if (kata) {
        for (let i = 0; i < kata.sections.length; i++) {
          delete this.data.completions[this.completionKey(kataId, i)];
        }
      }
    } else {
      // Reset everything
      this.data = this.freshData();
      if (this.katas.length > 0) {
        this.data.position.kataId = this.katas[0].id;
      }
    }
  }

  getOverallProgress(): OverallProgress {
    const katasMap = new Map<string, KataProgress>();
    let totalSections = 0;
    let completedSections = 0;

    for (const kata of this.katas) {
      const sections: SectionProgress[] = kata.sections.map((s, i) => {
        const isComplete = this.isComplete(kata.id, i);
        const key = this.completionKey(kata.id, i);
        return {
          index: i,
          id: s.id,
          title: s.title,
          type: s.type,
          isComplete,
          completedAt: this.data.completions[key]?.completedAt,
        };
      });
      const completed = sections.filter((s) => s.isComplete).length;
      katasMap.set(kata.id, {
        total: sections.length,
        completed,
        sections,
      });
      totalSections += sections.length;
      completedSections += completed;
    }

    return {
      katas: katasMap,
      currentPosition: this.getPosition(),
      stats: { totalSections, completedSections },
    };
  }
}
