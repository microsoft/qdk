// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// TODO (acasey): consider merging into catalog.ts

import { loadKatasCourse } from "./catalog.js";
import { KATAS_COURSE_ID } from "./constants.js";
import type { CatalogCourse, CourseDescriptor } from "./types.js";

/**
 * A source of learning courses. Implementations know how to enumerate the
 * courses they provide and how to fully load a course by id.
 *
 * Loading is intentionally split from enumeration so the UI can list
 * available courses cheaply without materializing every course.
 */
export interface CourseProvider {
  /** Stable identifier for this provider (for diagnostics/telemetry). */
  readonly id: string;
  /** Enumerate the descriptors for all courses this provider offers. */
  listCourses(): Promise<CourseDescriptor[]>;
  /** Fully load a course by id. Returns `undefined` if not provided here. */
  loadCourse(id: string): Promise<CatalogCourse | undefined>;
}

/**
 * Aggregates multiple {@link CourseProvider}s into a single catalog of
 * courses. The registry is the single entry point the service uses to
 * discover and load courses regardless of where they come from.
 */
export class CourseRegistry {
  constructor(private readonly providers: CourseProvider[]) {}

  /** Enumerate descriptors across all providers, in provider order. */
  async listCourses(): Promise<CourseDescriptor[]> {
    const all: CourseDescriptor[] = [];
    const seen = new Set<string>();
    for (const provider of this.providers) {
      let descriptors: CourseDescriptor[];
      try {
        descriptors = await provider.listCourses();
      } catch {
        // A misbehaving provider should not break the whole catalog.
        continue;
      }
      for (const descriptor of descriptors) {
        if (seen.has(descriptor.id)) {
          continue;
        }
        seen.add(descriptor.id);
        all.push(descriptor);
      }
    }
    return all;
  }

  /** Look up a single descriptor by id, or `undefined` if not found. */
  async getDescriptor(id: string): Promise<CourseDescriptor | undefined> {
    const all = await this.listCourses();
    return all.find((d) => d.id === id);
  }

  /**
   * Fully load a course by id. Tries each provider in order and returns
   * the first match. Throws if no provider can load the course.
   */
  async loadCourse(id: string): Promise<CatalogCourse> {
    for (const provider of this.providers) {
      const course = await provider.loadCourse(id);
      if (course) {
        return course;
      }
    }
    throw new Error(`No provider could load course "${id}".`);
  }
}

/** Provider for the built-in Quantum Katas course. */
export class KatasProvider implements CourseProvider {
  readonly id = "katas-provider";

  async listCourses(): Promise<CourseDescriptor[]> {
    return [
      {
        id: KATAS_COURSE_ID,
        title: "Quantum Katas",
        shortDescription:
          "Hands-on quantum computing tutorials and exercises in Q#.",
        kind: "qsharp",
      },
    ];
  }

  async loadCourse(id: string): Promise<CatalogCourse | undefined> {
    if (id !== KATAS_COURSE_ID) {
      return undefined;
    }
    return await loadKatasCourse();
  }
}
