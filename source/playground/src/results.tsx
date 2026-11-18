// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { QscEventTarget, ShotResult, VSDiagnostic } from "qsharp-lang";
import { useEffect, useState } from "preact/hooks";

import { Histogram, Markdown } from "qsharp-lang/ux";
import { StateTable } from "./state.js";
import { ActiveTab } from "./main.js";

function resultToLabel(result: string | VSDiagnostic): string {
  if (typeof result !== "string") return "ERROR";
  return result;
}

type ResultsState = {
  shotCount: number; // How many shots have started
  resultCount: number; // How many have completed (may be one less than above)
  currIndex: number; // Which is currently being displayed
  currResult: ShotResult | undefined; // The shot data to display
  buckets: Map<string, number>; // Histogram buckets
  filterValue: string; // Any filter that is in effect (or "")
  filterIndex: number; // The index into the filtered set
  currArray: ShotResult[]; // Used to detect a new run
};

function newRunState() {
  return {
    shotCount: 0,
    resultCount: 0,
    currIndex: 0,
    currResult: undefined,
    buckets: new Map(),
    filterValue: "",
    filterIndex: 0,
    currArray: [],
  };
}

function resultIsSame(a: ShotResult, b: ShotResult): boolean {
  // If the length changed, any entries are different objects, or the final result has changed.
  if (
    a.success !== b.success ||
    a.result !== b.result ||
    a.events.length !== b.events.length
  )
    return false;

  for (let i = 0; i < a.events.length; ++i) {
    if (a.events[i] !== b.events[i]) return false;
  }

  return true;
}

export function ResultsTab(props: {
  evtTarget: QscEventTarget;
  onShotError?: (err?: VSDiagnostic) => void;
  activeTab: ActiveTab;
}) {
  const [resultState, setResultState] = useState<ResultsState>(newRunState());

  // This is more complex than ideal for performance reasons. During a run, results may be getting
  // updated thousands of times a second, but there is no point trying to render at more than 60fps.
  // Therefore this subscribes to an event that happens once a frame if changes to results occur.
  // As the results are mutated array, they don't make good props or state, so need to manually
  // check for changes that would impact rendering and update state by creating new objects.
  const evtTarget = props.evtTarget;
  useEffect(() => {
    const resultUpdateHandler = () => {
      const results = evtTarget.getResults();

      // If it's a new run, the entire results array will be a new object
      const isNewResults = results !== resultState.currArray;

      // If the results object has changed then reset the current index and filter.
      const newIndex = isNewResults ? 0 : resultState.currIndex;
      const newFilterValue = isNewResults ? "" : resultState.filterValue;
      const newFilterIndex = isNewResults ? 0 : resultState.filterIndex;

      const currentResult = resultState.currResult;
      const updatedResult =
        newIndex < results.length ? results[newIndex] : undefined;

      const replaceResult =
        isNewResults ||
        // One is defined but the other isn't
        !currentResult !== !updatedResult ||
        // Or they both exist but are different (e.g. may have new events of have completed)
        (currentResult &&
          updatedResult &&
          !resultIsSame(currentResult, updatedResult));

      // Keep the old object if no need to replace it, else construct a new one
      const newResult = !replaceResult
        ? currentResult
        : !updatedResult
          ? undefined
          : {
              success: updatedResult.success,
              result: updatedResult.result,
              events: [...updatedResult.events],
            };

      // Update the histogram if new results have come in.
      // For now, just completely recreate the bucket map
      const resultCount = evtTarget.resultCount();
      let buckets = resultState.buckets;
      // If there are entirely new results, or if new results have been added, recalculate.
      if (isNewResults || resultState.resultCount !== resultCount) {
        buckets = new Map();
        for (let i = 0; i < resultCount; ++i) {
          const key = results[i].result;
          const strKey = resultToLabel(key);
          const newValue = (buckets.get(strKey) || 0) + 1;
          buckets.set(strKey, newValue);
        }
      }

      // If anything needs updating, construct the new state object and store
      if (
        replaceResult ||
        resultState.shotCount !== results.length ||
        resultState.resultCount !== resultCount ||
        resultState.currIndex !== newIndex
      ) {
        setResultState({
          shotCount: results.length,
          resultCount: resultCount,
          currIndex: newIndex,
          currResult: newResult,
          filterValue: newFilterValue,
          filterIndex: newFilterIndex,
          buckets,
          currArray: results,
        });
        updateEditorError(newResult);
      }
    };

    evtTarget.addEventListener("uiResultsRefresh", resultUpdateHandler);

    // Remove the event listener when this component is destroyed
    return () =>
      evtTarget.removeEventListener("uiResultsRefresh", resultUpdateHandler);
  }, [evtTarget]);

  // If there's a filter set, there must have been at least one item for that result.
  // If there's no filter set, may well be no results at all yet.

  const filterValue = resultState.filterValue;
  const countForFilter = filterValue
    ? resultState.buckets.get(filterValue) || 0
    : resultState.shotCount;
  const currIndex = filterValue
    ? resultState.filterIndex
    : resultState.currIndex;
  const resultLabel =
    typeof resultState.currResult?.result === "string"
      ? resultToLabel(resultState.currResult?.result || "")
      : `ERROR: ${resultState.currResult?.result.message}`;

  function moveToIndex(idx: number, filter: string) {
    const results = evtTarget.getResults();

    // The non-filtered default case
    let currIndex = idx;
    let currResult = results[idx];

    // If a filter is in effect, need to find the filtered index
    if (filter !== "") {
      let found = 0;
      for (let i = 0; i < results.length; ++i) {
        // The buckets to filter on have been converted to kets where possible
        if (resultToLabel(results[i].result) !== filter) continue;
        if (found === idx) {
          currIndex = i;
          currResult = results[i];
          break;
        }
        ++found;
      }
    }
    setResultState({
      ...resultState,
      filterValue: filter,
      filterIndex: idx,
      currIndex,
      currResult,
    });
    updateEditorError(currResult);
  }

  function updateEditorError(result?: ShotResult) {
    if (!props.onShotError) return;
    if (!result || result.success || typeof result.result === "string") {
      props.onShotError();
    } else {
      props.onShotError(result.result);
    }
  }

  function onPrev() {
    if (currIndex > 0) moveToIndex(currIndex - 1, filterValue);
  }

  function onNext() {
    if (currIndex < countForFilter - 1) moveToIndex(currIndex + 1, filterValue);
  }

  return props.activeTab === "results-tab" ? (
    <div>
      {!resultState.shotCount ? null : (
        <>
          {resultState.buckets.size > 1 ? (
            <Histogram
              shotCount={resultState.shotCount}
              data={resultState.buckets}
              filter={filterValue}
              onFilter={(val: string) => moveToIndex(0, val)}
              shotsHeader={false}
            ></Histogram>
          ) : null}
          <div class="output-header">
            <div>
              Shot {currIndex + 1} of {countForFilter}
            </div>
            <div class="prev-next">
              <span onClick={onPrev}>Prev</span> |{" "}
              <span onClick={onNext}>Next</span>
            </div>
          </div>
          <div class="result-label">Result: {resultLabel}</div>
          <div>
            {resultState.currResult?.events.map((evt) => {
              return evt.type === "Message" ? (
                <div class="message-output">&gt; {evt.message}</div>
              ) : evt.type === "DumpMachine" ? (
                <div>
                  <StateTable
                    dump={evt.state}
                    latexDump={evt.stateLatex}
                    count={evt.qubitCount}
                  ></StateTable>
                </div>
              ) : (
                <Markdown markdown={evt.matrixLatex} />
              );
            })}
          </div>
        </>
      )}
    </div>
  ) : null;
}
