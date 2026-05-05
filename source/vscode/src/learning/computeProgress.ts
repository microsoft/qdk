// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  UnitProgress,
  OverallProgress,
  ProgressFileData,
  ActivityKind,
  ActivityProgress,
} from "./types.js";

/**
 * Minimal catalog shape needed for progress computation.
 */
export interface ProgressCatalogEntry {
  id: string;
  title: string;
  activities: { id: string; title: string; type: ActivityKind }[];
}

function completionKey(unitId: string, activityId: string): string {
  return `${unitId}__${activityId}`;
}

/**
 * Compute `OverallProgress` by joining a unit catalog with a progress-file
 * snapshot. Shared between `LearningService` and the progress tree view to avoid
 * duplicating the join logic.
 */
export function computeOverallProgress(
  catalog: ProgressCatalogEntry[],
  data: ProgressFileData,
): OverallProgress {
  let totalActivities = 0;
  let completedActivities = 0;

  const units: UnitProgress[] = catalog.map((unit) => {
    const activities: ActivityProgress[] = unit.activities.map((a) => {
      const key = completionKey(unit.id, a.id);
      const completion = data.completions[key];
      return {
        id: a.id,
        title: a.title,
        type: a.type,
        isComplete: completion != null,
        completedAt: completion?.completedAt,
      };
    });
    const completed = activities.filter((a) => a.isComplete).length;
    totalActivities += activities.length;
    completedActivities += completed;
    return {
      id: unit.id,
      title: unit.title,
      total: activities.length,
      completed,
      activities,
    };
  });

  const currentUnitId = data.position.unitId;
  const currentUnit = currentUnitId
    ? catalog.find((u) => u.id === currentUnitId)
    : undefined;

  return {
    units,
    currentPosition: {
      unitId: data.position.unitId,
      activityId: data.position.activityId,
      unitTitle: (currentUnit?.title ?? currentUnitId) || undefined,
    },
    stats: { totalActivities, completedActivities },
  };
}
