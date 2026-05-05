// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { KATAS_COURSE_ID } from "./constants.js";
import type { ActivityLocation, ProgressFileData } from "./types.js";

/** Stable string key for an activity location, used in completion maps. */
export function activityLocationKey(loc: ActivityLocation): string {
  return `${loc.courseId}__${loc.unitId}__${loc.activityId}`;
}

/** Structural equality for two activity locations. */
export function activityLocationsEqual(
  a: ActivityLocation,
  b: ActivityLocation,
): boolean {
  return (
    a.courseId === b.courseId &&
    a.unitId === b.unitId &&
    a.activityId === b.activityId
  );
}

/**
 * Look up a completion entry, handling backward-compatible keys that omit
 * the courseId prefix (written before courses were introduced).
 */
export function findCompletion(
  completions: ProgressFileData["completions"],
  loc: ActivityLocation,
): { completedAt: string } | undefined {
  const key = activityLocationKey(loc);
  if (key in completions) {
    return completions[key];
  }
  // Backward compat: old keys used `unitId__activityId` without courseId
  if (loc.courseId === KATAS_COURSE_ID) {
    const legacyKey = `${loc.unitId}__${loc.activityId}`;
    if (legacyKey in completions) {
      return completions[legacyKey];
    }
  }
  return undefined;
}
