// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Shared types for the Quantum Katas activity-bar panel.
 *
 * Progress schema is mirrored from the learning-server's
 * `ProgressFileData` (see source/vscode/src/learning/server/progress.ts).
 * We deliberately re-declare it here so this module does not depend on
 * anything inside `src/learning/` — the learning subtree is bundled as
 * a separate Node CLI and pulls in the full ~2 MB kata markdown catalog.
 */

export interface ProgressFileData {
  version: 1;
  position: { kataId: string; sectionId: string; itemIndex: number };
  completions: Record<string, { completedAt: string }>;
  startedAt: string;
}

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
