// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { Operation } from "./data-structures/circuit.js";
import { Register } from "./data-structures/register.js";

export type Tick = {
  value: number;
  label: string;
};

// Should be redesigned in case of localization
type TickTimeDefinition = { tick: Tick; plural: string };

export function CreateIntegerTicks(min: number, max: number): Tick[] {
  if (max < min || min <= 0) {
    return [];
  }

  const numberFormat = new Intl.NumberFormat();

  const l = Math.round(10 ** Math.floor(Math.log10(min)));
  const r = Math.round(10 ** Math.ceil(Math.log10(max)));

  const result: Tick[] = [];

  for (let i = l; i <= r; i *= 10) {
    if (i >= min && i <= max) {
      result.push({ value: i, label: numberFormat.format(i) });
    }
  }
  if (result.length == 0) {
    let step = l;
    while (step >= 1) {
      let i = Math.ceil(min / step) * step;
      while (i <= max) {
        result.push({ value: i, label: numberFormat.format(i) });
        i += step;
      }
      if (result.length > 0) {
        break;
      }
      step /= 10;
    }
  }

  return result;
}

const predefinedTimeTicks: TickTimeDefinition[] = [
  { tick: { value: 1, label: "1 nanosecond" }, plural: "nanoseconds" },
  { tick: { value: 1e3, label: "1 microsecond" }, plural: "microseconds" },
  { tick: { value: 1e6, label: "1 millisecond" }, plural: "milliseconds" },
  { tick: { value: 1e9, label: "1 second" }, plural: "seconds" },
  { tick: { value: 6e10, label: "1 minute" }, plural: "minutes" },
  { tick: { value: 3.6e12, label: "1 hour" }, plural: "hours" },
  { tick: { value: 8.64e13, label: "1 day" }, plural: "days" },
  { tick: { value: 6.048e14, label: "1 week" }, plural: "weeks" },
  { tick: { value: 2.592e15, label: "1 month" }, plural: "months" },
  { tick: { value: 3.1536e16, label: "1 year" }, plural: "years" },
  { tick: { value: 3.1536e17, label: "1 decade" }, plural: "decades" },
  { tick: { value: 3.1536e18, label: "1 century" }, plural: "centuries" },
];

export function CreateTimeTicks(min: number, max: number): Tick[] {
  if (max < min || min <= 0) {
    return [];
  }

  let l = 0;
  while (
    l < predefinedTimeTicks.length &&
    predefinedTimeTicks[l].tick.value <= min
  ) {
    l++;
  }

  let r = l;
  if (l > 0) {
    l--;
  }

  if (r >= predefinedTimeTicks.length) {
    r = predefinedTimeTicks.length - 1;
  }

  while (
    r < predefinedTimeTicks.length - 1 &&
    predefinedTimeTicks[r].tick.value <= max
  ) {
    r++;
  }

  const result: Tick[] = [];

  for (let i = l; i <= r; i++) {
    if (
      predefinedTimeTicks[i].tick.value >= min &&
      predefinedTimeTicks[i].tick.value <= max
    ) {
      result.push(predefinedTimeTicks[i].tick);
    }
  }

  if (result.length == 0) {
    if (l < predefinedTimeTicks.length - 1) {
      let coeff =
        10 **
        Math.floor(
          Math.log10(
            predefinedTimeTicks[l + 1].tick.value /
              predefinedTimeTicks[l].tick.value,
          ),
        );
      do {
        let i = 1;
        let val;
        do {
          val = predefinedTimeTicks[l].tick.value * i * coeff;
          if (val >= min && val <= max) {
            result.push({
              value: val,
              label:
                (i * coeff).toString() + " " + predefinedTimeTicks[l].plural,
            });
          }
          i++;
        } while (
          // just a single tick in case of 1, 10 or 100
          (i > 2 || result.length < 1) &&
          val < predefinedTimeTicks[l + 1].tick.value
        );
        coeff /= 10;
      } while (result.length < 1 && coeff >= 1);
    } else {
      // > century
      let l = predefinedTimeTicks.length - 1;
      do {
        let coeff =
          10 ** Math.floor(Math.log10(max / predefinedTimeTicks[l].tick.value));
        do {
          let i = 1;
          let val;
          do {
            val = predefinedTimeTicks[l].tick.value * i * coeff;
            if (val >= min && val <= max) {
              result.push({
                value: val,
                label:
                  (i * coeff).toString() + " " + predefinedTimeTicks[l].plural,
              });
            }
            i++;
          } while (
            // just a single tick in case of 1, 10 or 100
            (i > 2 || result.length < 1) &&
            val < max
          );
          coeff /= 10;
        } while (result.length < 1 && coeff >= 1);
        l--;
      } while (result.length < 1 && l >= 0);
    }
  }

  return result;
}

type SeriesOfPoints = Array<{ items: Array<{ x: number; y: number }> }>;

// Given an array of object, and each of those objects has an 'items' property
// that is another array of objects with number properties x & y, return a
// tuple of [minX, maxX, minY, maxY] covering all items given
export function getMinMaxXYForItems(
  data: SeriesOfPoints,
): [number, number, number, number] {
  return data.reduce(
    (priorSeriesResult, curr) =>
      curr.items.reduce(
        (priorItemResult, curr) => [
          Math.min(priorItemResult[0], curr.x),
          Math.max(priorItemResult[1], curr.x),
          Math.min(priorItemResult[2], curr.y),
          Math.max(priorItemResult[3], curr.y),
        ],
        priorSeriesResult,
      ),
    [Number.MAX_VALUE, Number.MIN_VALUE, Number.MAX_VALUE, Number.MIN_VALUE],
  );
}

export function getRanges(data: SeriesOfPoints, rangeCoefficient: number) {
  const [minX, maxX, minY, maxY] = getMinMaxXYForItems(data);

  const rangeX = {
    min: minX / rangeCoefficient,
    max: maxX * rangeCoefficient,
  };
  const rangeY = {
    min: minY / rangeCoefficient,
    max: maxY * rangeCoefficient,
  };
  return { rangeX, rangeY };
}

/**
 * Returns all registers (targets and controls) involved in an operation.
 *
 * @param operation The operation for which to get the registers.
 * @returns An array of Register objects involved in the operation.
 */
export function getOperationRegisters(operation: Operation): Register[] {
  switch (operation.kind) {
    case "unitary":
      return [...operation.targets, ...(operation.controls || [])];
    case "ket":
      return operation.targets;
    case "measurement":
      return [...operation.qubits, ...(operation.results || [])];
  }
}
