// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useState } from "preact/hooks";
import { type ReData, SingleEstimateResult } from "./data.js";
import { EstimatesOverview } from "./estimatesOverview.js";
import { ReTable } from "./reTable.js";
import { SpaceChart } from "./spaceChart.js";
import { Spinner } from "./spinner.js";

export function EstimatesPanel(props: {
  estimatesData: ReData[];
  colors: string[];
  runNames: string[];
  calculating: boolean;
  onRowDeleted: (rowId: string) => void;
  allowSaveImage?: boolean;
}) {
  const [estimate, setEstimate] = useState<SingleEstimateResult | null>(null);

  return (
    <>
      <div style="display:flex; height:64px; align-items: center; position: relative">
        <svg
          width="48"
          height="48"
          viewBox="96 96 828 828"
          xmlns="http://www.w3.org/2000/svg"
        >
          <g fill="none" stroke="gray" stroke-width="8">
            <path d="M 512 135 L 819 313 L 819 667 L 512 845 L 205 667 L 205 313 L 512 135 Z" />
            <path d="M 205 580 L 742 890 L 819 845 L 818 756 L 205 402" />
            <path d="M 204 579 L 743 268" />
            <path d="M 664 224 L 207 489" />
            <path d="M 206 400 L 588 180" />
            <path d="M 205 667 L 818 313" />
            <path d="M 205 490 L 820 845" />
            <path d="M 207 314 L 818 667" />
            <path d="M 282 269 L 820 580" />
            <path d="M 820 490 L 357 223" />
            <path d="M 435 180 L 818 400" />
            <path d="M 281 710 L 281 271" />
            <path d="M 357 755 L 357 223" />
            <path d="M 283 711 L 820 400" />
            <path d="M 434 797 L 434 181" />
            <path d="M 511 136 L 511 844" />
            <path d="M 588 799 L 588 182" />
            <path d="M 665 223 L 665 845" />
            <path d="M 742 887 L 742 267" />
            <path d="M 665 845 L 816 758" />
            <path d="M 433 801 L 820 577" />
            <path d="M 820 489 L 360 755" />
          </g>
        </svg>
        {props.calculating ? (
          <Spinner style="position: absolute; top: 11px; left: 4px;" />
        ) : null}
        <h1>Microsoft Quantum Resource Estimator</h1>
      </div>
      <EstimatesOverview
        estimatesData={props.estimatesData}
        isSimplifiedView={false}
        onRowDeleted={props.onRowDeleted}
        setEstimate={setEstimate}
        runNames={props.runNames}
        colors={props.colors}
        allowSaveImage={props.allowSaveImage || false}
      ></EstimatesOverview>
      {!estimate ? null : (
        <>
          <details open>
            <summary style="font-size: 1.5em; font-weight: bold; margin: 24px 8px;">
              Space diagram
            </summary>
            <SpaceChart estimatesData={estimate} />
          </details>
          <details open>
            <summary style="font-size: 1.5em; font-weight: bold; margin: 24px 8px;">
              Resource Estimates
            </summary>
            <ReTable estimatesData={estimate} />
          </details>
        </>
      )}
    </>
  );
}
