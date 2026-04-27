// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname } from "node:path";
import type {
  ProgressFileData,
  OverallProgress,
  KataProgress,
  SectionProgress,
  Kata,
} from "./types.js";

export class ProgressManager {
  private filePath: string;
  private data: ProgressFileData;
  private katas: Kata[] = [];

  /**
   * @param learningFilePath Absolute path to the `qdk-learning.json` file.
   * @param katasRoot The relative `katasRoot` value to write into the file.
   */
  constructor(
    learningFilePath: string,
    private katasRoot: string,
  ) {
    this.filePath = learningFilePath;
    this.data = this.freshData();
  }

  private freshData(): ProgressFileData {
    return {
      version: 1,
      katasRoot: this.katasRoot,
      position: { kataId: "", sectionId: "", itemIndex: 0 },
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
        // Preserve the katasRoot value from the constructor (authoritative)
        // but read everything else from disk.
        this.data = { ...parsed, katasRoot: this.katasRoot };
        // Ensure position references a valid kata
        if (
          katas.length > 0 &&
          !katas.find((k) => k.id === this.data.position.kataId)
        ) {
          this.data.position = {
            kataId: katas[0].id,
            sectionId: katas[0].sections[0]?.id ?? "",
            itemIndex: 0,
          };
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
  private completionKey(kataId: string, sectionId: string): string {
    return `${kataId}__${sectionId}`;
  }

  isComplete(kataId: string, sectionId: string): boolean {
    return this.completionKey(kataId, sectionId) in this.data.completions;
  }

  markComplete(kataId: string, sectionId: string): void {
    const key = this.completionKey(kataId, sectionId);
    if (!(key in this.data.completions)) {
      this.data.completions[key] = { completedAt: new Date().toISOString() };
    }
  }

  getPosition(): { kataId: string; sectionId: string; itemIndex: number } {
    return { ...this.data.position };
  }

  setPosition(kataId: string, sectionId: string, itemIndex: number): void {
    this.data.position = { kataId, sectionId, itemIndex };
  }

  reset(kataId?: string): void {
    if (kataId) {
      // Reset only the given kata
      const kata = this.katas.find((k) => k.id === kataId);
      if (kata) {
        for (const s of kata.sections) {
          delete this.data.completions[this.completionKey(kataId, s.id)];
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
      const sections: SectionProgress[] = kata.sections.map((s) => {
        const isComplete = this.isComplete(kata.id, s.id);
        const key = this.completionKey(kata.id, s.id);
        return {
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
