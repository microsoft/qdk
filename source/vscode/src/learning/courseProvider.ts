// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { NotebookCourseProvider } from "./notebookCourseProvider.js";
import { KatasProvider } from "./katasProvider.js";
import type { CatalogCourse, CourseDescriptor } from "./types.js";

/**
 * A source of learning courses. Implementations know how to find the courses
 * they provide and parse them into memory.
 *
 * Loading a course only reads and parses it. Creating the learner's editable
 * files is a separate step — see `materializeCourseWorkbooks`.
 */
export interface CourseProvider {
  /** Stable identifier for this provider (for diagnostics/telemetry). */
  readonly id: string;
  /** Find and parse every course this provider offers. */
  listCourses(): Promise<CatalogCourse[]>;
}

/**
 * Aggregates multiple {@link CourseProvider}s so the service has a single
 * place to ask for courses regardless of where they come from.
 */
export class CompositeCourseProvider implements CourseProvider {
  readonly id = "composite-provider";

  constructor(private readonly providers: CourseProvider[]) {}

  /**
   * Parse the courses from every provider, in provider order. When two
   * providers offer the same course id, the earlier provider wins.
   */
  async listCourses(): Promise<CatalogCourse[]> {
    const all: CatalogCourse[] = [];
    // Course id -> id of the provider that claimed it.
    const claimedBy = new Map<string, string>();

    for (const provider of this.providers) {
      let courses: CatalogCourse[];
      try {
        courses = await provider.listCourses();
      } catch (e) {
        // A misbehaving provider should not break the whole catalog.
        log.warn(
          `Course provider "${provider.id}" failed to list courses: ${String(e)}`,
        );
        continue;
      }

      for (const course of courses) {
        const winner = claimedBy.get(course.id);
        if (winner !== undefined) {
          const from = course.sourceDir
            ? ` at ${vscode.Uri.parse(course.sourceDir).fsPath}`
            : "";
          log.warn(
            `Ignoring course "${course.id}" from "${provider.id}"${from}: ` +
              `that id is already provided by "${winner}".`,
          );
          continue;
        }
        claimedBy.set(course.id, provider.id);
        all.push(course);
      }
    }
    return all;
  }
}

/**
 * Create the {@link CompositeCourseProvider} with every available source of
 * courses: the built-in Quantum Katas plus any courses authored on disk
 * under `qdk-learning/courses/*`.
 */
export function createCourseProvider(
  workspaceRoot: vscode.Uri,
): CompositeCourseProvider {
  return new CompositeCourseProvider([
    new KatasProvider(),
    new NotebookCourseProvider(workspaceRoot),
  ]);
}

/** Project a loaded course down to the summary used by UI surfaces. */
export function toDescriptor(course: CatalogCourse): CourseDescriptor {
  return {
    id: course.id,
    title: course.title,
    shortDescription: course.shortDescription,
    kind: course.kind,
    environment: course.environment,
  };
}
