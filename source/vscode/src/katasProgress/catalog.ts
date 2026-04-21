// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { getAllKatas } from "qsharp-lang/katas-md";
import type { CatalogKata, CatalogSection } from "./types.js";

/**
 * Recommended pedagogical order for katas. Kept in sync with
 * `RECOMMENDED_ORDER` in source/vscode/src/learning/server/server.ts.
 */
const RECOMMENDED_ORDER = [
  "getting_started",
  "complex_arithmetic",
  "linear_algebra",
  "qubit",
  "single_qubit_gates",
  "multi_qubit_systems",
  "multi_qubit_gates",
  "preparing_states",
  "distinguishing_states",
  "measurements",
  "random_numbers",
  "deutsch_jozsa",
  "grover",
  "key_distribution",
  "graphs",
];

let cached: CatalogKata[] | undefined;

/**
 * Load the full kata catalog (ids, titles, section shapes) from
 * `qsharp-lang/katas-md`. Cached for the lifetime of the extension host.
 *
 * Note: `qsharp-lang/katas-md` pulls in the full generated-markdown module
 * (~2 MB), which esbuild statically bundles into the extension entry.
 */
export async function loadCatalog(): Promise<CatalogKata[]> {
  if (cached) return cached;

  const allKatas = await getAllKatas();

  const trimmed: CatalogKata[] = allKatas.map((k) => ({
    id: k.id,
    title: k.title,
    sections: k.sections.map<CatalogSection>((s) => ({
      id: s.id,
      title: s.title,
      kind: s.type,
    })),
  }));

  trimmed.sort((a, b) => {
    const ai = RECOMMENDED_ORDER.indexOf(a.id);
    const bi = RECOMMENDED_ORDER.indexOf(b.id);
    return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
  });

  cached = trimmed;
  return cached;
}
