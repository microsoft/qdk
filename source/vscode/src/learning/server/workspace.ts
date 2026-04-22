// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { join, dirname } from "node:path";
import type { Kata, Exercise, Lesson } from "./types.js";

export class WorkspaceManager {
  constructor(private workspacePath: string) {}

  /** Get the directory for a kata's exercise files */
  private exerciseDir(kataId: string): string {
    return join(this.workspacePath, "exercises", kataId);
  }

  /** Get the directory for a kata's example files */
  private exampleDir(kataId: string): string {
    return join(this.workspacePath, "examples", kataId);
  }

  /** Get the file path for an exercise's solution file */
  getExerciseFilePath(kataId: string, exerciseId: string): string {
    return join(this.exerciseDir(kataId), `${exerciseId}.qs`);
  }

  /** Get the file path for an example's standalone .qs file */
  getExampleFilePath(kataId: string, exampleId: string): string {
    return join(this.exampleDir(kataId), `${exampleId}.qs`);
  }

  /**
   * Scaffold exercise .qs files for the given katas.
   * Skips files that already exist to preserve user work.
   */
  async scaffoldExercises(katas: Kata[]): Promise<void> {
    for (const kata of katas) {
      for (const section of kata.sections) {
        if (section.type !== "exercise") continue;
        const exercise = section as Exercise;
        const filePath = this.getExerciseFilePath(kata.id, exercise.id);
        if (await this.fileExists(filePath)) continue;

        await mkdir(dirname(filePath), { recursive: true });
        await writeFile(filePath, exercise.placeholderCode, "utf-8");
      }
    }
  }

  /**
   * Scaffold example .qs files for the given katas.
   * Examples are read-only reference material — overwrite unconditionally
   * so corpus updates propagate.
   */
  async scaffoldExamples(katas: Kata[]): Promise<void> {
    for (const kata of katas) {
      for (const section of kata.sections) {
        if (section.type !== "lesson") continue;
        const lesson = section as Lesson;
        for (const item of lesson.items) {
          if (item.type !== "example") continue;
          const filePath = this.getExampleFilePath(kata.id, item.id);
          await mkdir(dirname(filePath), { recursive: true });
          await writeFile(filePath, item.code, "utf-8");
        }
      }
    }
  }

  /** Read the user's current code for an exercise */
  async readUserCode(kataId: string, exerciseId: string): Promise<string> {
    const filePath = this.getExerciseFilePath(kataId, exerciseId);
    return readFile(filePath, "utf-8");
  }

  /** Write code to an exercise file (used for testing / resetting) */
  async writeUserCode(
    kataId: string,
    exerciseId: string,
    code: string,
  ): Promise<void> {
    const filePath = this.getExerciseFilePath(kataId, exerciseId);
    await mkdir(dirname(filePath), { recursive: true });
    await writeFile(filePath, code, "utf-8");
  }

  private async fileExists(path: string): Promise<boolean> {
    try {
      await access(path);
      return true;
    } catch {
      return false;
    }
  }
}
