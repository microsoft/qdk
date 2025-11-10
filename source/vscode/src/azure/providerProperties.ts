// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { TargetProfile } from "qsharp-lang";

const excludeTargets: string[] = [
  "ionq.qpu",
  "rigetti.qpu.aspen-m-2",
  "rigetti.qpu.ankaa-2",
];

const excludeProviders: string[] = [];

export function targetSupportQir(target: string) {
  // Note: Most of these should be dynamic at some point, with configuration coming
  // from the service, and able to be overridden by settings.
  return (
    !(target == "microsoft.estimator") &&
    !(target.startsWith("microsoft") && target.endsWith("cpu"))
  );
}

export function shouldExcludeTarget(target: string) {
  return excludeTargets.includes(target);
}

export function shouldExcludeProvider(provider: string) {
  return excludeProviders.includes(provider);
}

export function getPreferredTargetProfile(target: string): TargetProfile {
  if (target.startsWith("ionq") || target.startsWith("rigetti")) {
    return "base";
  } else if (target.startsWith("quantinuum") && target.includes(".h2")) {
    return "adaptive_ri";
  } else {
    return "adaptive_rif";
  }
}
