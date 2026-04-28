// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Shared types for the Quantum Katas activity-bar panel.
 *
 * `ProgressFileData` is defined in `../types.ts` (the canonical learning
 * types module) and re-exported here for convenience.
 */

export type { ProgressFileData } from "../types.js";

export type SectionKind = "lesson" | "exercise";

export interface CatalogSection {
  id: string;
  title: string;
  kind: SectionKind;
  /** True for lessons that contain at least one code example. Always false for exercises. */
  hasExample?: boolean;
  /** For lessons with examples, the id of the first example item (used to resolve the .qs file path). */
  exampleId?: string;
}

export interface CatalogKata {
  id: string;
  title: string;
  sections: CatalogSection[];
}

export interface SectionProgress extends CatalogSection {
  isComplete: boolean;
  completedAt?: string;
}

export interface KataProgress {
  id: string;
  title: string;
  total: number;
  completed: number;
  sections: SectionProgress[];
}

export interface OverallProgress {
  katas: KataProgress[];
  /** May point at a kata that is not in the catalog (stale data) — callers should handle. */
  currentPosition: { kataId: string; sectionId: string; itemIndex: number };
  stats: { totalSections: number; completedSections: number };
}
