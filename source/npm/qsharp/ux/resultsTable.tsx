// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useRef, useState, useEffect } from "preact/hooks";

export type CellValue = string | number | { value: string; sortBy: number };
export type Row = {
  color: string;
  cells: CellValue[];
};

// Note: column 0 is expected to be unique amongst all rows
export function ResultsTable(props: {
  columnNames: string[];
  rows: Row[];
  initialColumns: number[];
  onRowDeleted(rowId: string): void;
  selectedRow: string | null;
  onRowSelected(rowId: string): void;
}) {
  const [showColumns, setShowColumns] = useState(props.initialColumns);
  const [sortColumn, setSortColumn] = useState<{
    columnId: number;
    ascending: boolean;
  } | null>(null);

  const [showColumnMenu, setShowColumnMenu] = useState(false);
  const [showRowMenu, setShowRowMenu] = useState("");

  // Use to track the column being dragged
  const draggingCol = useRef("");
  const columnMenu = useRef<HTMLDivElement>(null);

  /*
  Note: Drag and drop events can occur faster than preact reconciles state.
  This causes challenges where one event will set state, and the next event
  that needs to use the latest state will still see old state.

  So don't apply state changes in the drag and drop handlers. Instead, set
  styles directly, and just update state if the 'drop' event changes column
  order.
  */

  function onDragStart(ev: DragEvent) {
    if (!(ev.target instanceof HTMLElement)) return;
    const colid = ev.target.closest("th")?.dataset["colid"];
    draggingCol.current = colid!;
    ev.dataTransfer!.dropEffect = "move";
  }

  function onDragEnter(ev: DragEvent) {
    // Get the column id of the column being entered
    if (!(ev.target instanceof HTMLElement)) return;
    const thisColId = ev.target.closest("th")?.dataset["colid"];
    if (!thisColId || !draggingCol.current) return;

    // If this column is different to the column being dragged, add the CSS class
    if (draggingCol.current !== thisColId) {
      ev.preventDefault();
      ev.dataTransfer!.dropEffect = "move";
      ev.target
        .closest("table")!
        .querySelectorAll(`[data-colid="${thisColId}"]`)
        .forEach((elem) => elem.classList.add("qs-resultsTable-dragEnter"));
    }
  }

  function onRowSelected(rowId: string) {
    props.onRowSelected(rowId);
  }

  function onDragOver(ev: DragEvent) {
    if (!(ev.target instanceof HTMLElement)) return;
    const thisColId = ev.target.closest("th")?.dataset["colid"];
    if (!thisColId || !draggingCol.current) return;

    // If dragging something over a different column, allow the drop
    if (draggingCol.current !== thisColId) {
      ev.dataTransfer!.dropEffect = "move";
      ev.preventDefault();
    }
  }

  function onDragLeave(ev: DragEvent) {
    // Remove the CSS class from the column being left
    if (!(ev.target instanceof HTMLElement)) return;
    const thisColId = ev.target.closest("th")?.dataset["colid"];
    if (!thisColId) return;

    ev.target
      .closest("table")!
      .querySelectorAll(`[data-colid="${thisColId}"]`)
      .forEach((elem) => elem.classList.remove("qs-resultsTable-dragEnter"));
    ev.preventDefault();
  }

  function onDrop(ev: DragEvent) {
    if (!(ev.target instanceof HTMLElement)) return;
    const thisColId = ev.target.closest("th")?.dataset["colid"];
    if (!thisColId) return;

    if (draggingCol.current) {
      moveColumn(parseInt(draggingCol.current), parseInt(thisColId));
      ev.preventDefault();
    }
  }

  function onDragEnd(ev: DragEvent) {
    // Called regardless of how dragging ends
    // ev.target is the source element
    // Just remove any dragEnter classes from cells that may remain
    (ev.target as HTMLElement)
      .closest("table")!
      .querySelectorAll(`th, td`)
      .forEach((elem) => elem.classList.remove("qs-resultsTable-dragEnter"));
    draggingCol.current = "";
  }

  function moveColumn(oldIdx: number, newIdx: number) {
    // Locate the indexes in the array where the column idx is
    const arrIdxOld = showColumns.indexOf(oldIdx);
    const arrIdxNew = showColumns.indexOf(newIdx);

    const newColumns = [...showColumns];
    const removed = newColumns.splice(arrIdxOld, 1);
    newColumns.splice(arrIdxNew, 0, ...removed);
    setShowColumns(newColumns);
  }

  function onSort(ev: MouseEvent) {
    if (!(ev.currentTarget instanceof HTMLTableCellElement)) return;
    const thisCol = ev.currentTarget.dataset["colid"];

    if (sortColumn && thisCol === sortColumn.columnId.toString()) {
      // Toggle the sort order
      setSortColumn({
        columnId: sortColumn.columnId,
        ascending: !sortColumn.ascending,
      });
    } else {
      // Set the sort column
      setSortColumn({
        columnId: parseInt(thisCol!),
        ascending: true,
      });
    }
  }

  function getSortedRows(rows: Row[]) {
    if (!sortColumn) return rows;

    const colIdx = sortColumn.columnId;
    const ascending = sortColumn.ascending;

    const sortedRows = [...rows];
    sortedRows.sort((a, b) => {
      const aVal = a.cells[colIdx];
      const bVal = b.cells[colIdx];
      if (typeof aVal === "string" && typeof bVal === "string") {
        return ascending ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal);
      } else if (typeof aVal === "number" && typeof bVal === "number") {
        return ascending ? aVal - bVal : bVal - aVal;
      } else if (typeof aVal === "object" && typeof bVal === "object") {
        return ascending
          ? aVal.sortBy - bVal.sortBy
          : bVal.sortBy - aVal.sortBy;
      } else {
        return 0;
      }
    });

    return sortedRows;
  }

  function getCellStr(col: CellValue) {
    if (typeof col === "object") {
      return col.value;
    } else if (typeof col === "number") {
      return col.toLocaleString();
    } else {
      return col || "";
    }
  }

  function onRowClicked(rowId: string) {
    const newSelectedRow = props.selectedRow === rowId ? "" : rowId;
    onRowSelected(newSelectedRow);
  }

  function onClickRowMenu(ev: MouseEvent, rowid: string) {
    ev.stopPropagation();
    if (showRowMenu === rowid) {
      setShowRowMenu("");
    } else {
      setShowRowMenu(rowid!);
    }
  }

  function onClickColumnMenu(ev: MouseEvent) {
    ev.stopPropagation();
    setShowRowMenu("");
    setShowColumnMenu(!showColumnMenu);
  }

  function getColumnList() {
    return props.columnNames.map((name, idx) => ({
      name,
      idx,
      show: showColumns.includes(idx),
    }));
  }

  function toggleColumn(idx: number) {
    const newColumns = [...showColumns];
    const arrIdx = newColumns.indexOf(idx);
    if (arrIdx === -1) {
      // Not currently showing, need to add it
      if (idx > newColumns.length) {
        // The column position is greater than the number of columns currently showing
        // So just add to the end
        newColumns.push(idx);
      } else {
        // Insert at the correct position
        newColumns.splice(idx, 0, idx);
      }
    } else {
      newColumns.splice(arrIdx, 1);
    }
    setShowColumns(newColumns);
  }

  function deleteRow(e: MouseEvent, rowId: string) {
    e.stopPropagation();
    // Clear out any menus or selections for the row if needed
    setShowRowMenu("");
    if (props.selectedRow === rowId) {
      onRowSelected("");
    }
    props.onRowDeleted(rowId);
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (!props.selectedRow) return;
    const sortedRowNames = getSortedRows(props.rows).map((row) =>
      row.cells[0].toString(),
    );
    const currIndex = sortedRowNames.indexOf(props.selectedRow);

    switch (ev.code) {
      case "ArrowDown":
        if (currIndex < sortedRowNames.length - 1) {
          ev.preventDefault();
          props.onRowSelected(sortedRowNames[currIndex + 1]);
        }
        break;
      case "ArrowUp":
        if (currIndex > 0) {
          ev.preventDefault();
          props.onRowSelected(sortedRowNames[currIndex - 1]);
        }
        break;
      default:
      // Not of interest
    }
  }

  useEffect(() => {
    // Post rendering, if the column menu is displayed, then ensure it
    // has focus so that clicking anywhere outside of it caused the blur
    // event that closes it.
    if (showColumnMenu && columnMenu.current) {
      columnMenu.current.focus();
    }
  });

  return (
    <table
      class="qs-resultsTable-sortedTable"
      tabIndex={0}
      onKeyDown={onKeyDown}
    >
      <thead>
        <tr>
          <th>
            <div
              style="position: relative"
              tabIndex={0}
              onBlur={() => setShowColumnMenu(false)}
            >
              <svg
                width="16"
                height="16"
                style="position: relative;"
                onClick={onClickColumnMenu}
              >
                <rect x="1" y="3.5" width="14" height="2" fill="black" />
                <rect
                  x="1"
                  y="3"
                  width="14"
                  height="12"
                  stroke="gray"
                  stroke-width="1"
                  fill="none"
                  rx="2"
                />
                <path
                  stroke="gray"
                  stroke-width="1"
                  d="M4.5,3 V15 M8,3 V15 M11.5,3 V15"
                />
              </svg>
              <div
                ref={columnMenu}
                class={
                  showColumnMenu
                    ? "qs-resultsTable-columnMenu qs-resultsTable-showColumnMenu"
                    : "qs-resultsTable-columnMenu"
                }
                style="position: absolute; top: 16; left: 0;"
              >
                {getColumnList().map((elem) => (
                  <div
                    style="width: 100px; height: 20px;"
                    class={
                      elem.show
                        ? "qs-resultsTable-columnSelected"
                        : "qs-resultsTable-menuItem"
                    }
                    onClick={() => toggleColumn(elem.idx)}
                  >
                    {elem.name}
                  </div>
                ))}
              </div>
            </div>
          </th>
          {showColumns.map((idx) => {
            const isSortColumn = sortColumn?.columnId === idx;
            return (
              <th onClick={onSort} data-colid={idx.toString()}>
                <span
                  class={
                    isSortColumn
                      ? "qs-resultsTable-sortHeaderCell"
                      : "qs-resultsTable-headerCell"
                  }
                  draggable
                  onDragStart={onDragStart}
                  onDragEnter={onDragEnter}
                  onDragOver={onDragOver}
                  onDragLeave={onDragLeave}
                  onDragEnd={onDragEnd}
                  onDrop={onDrop}
                >
                  {props.columnNames[idx]}
                </span>
                {isSortColumn ? (
                  <svg
                    width="16"
                    height="16"
                    style={`transform: rotate(${
                      sortColumn!.ascending ? "0" : "180"
                    }deg)`}
                  >
                    <polygon fill="gray" points="2,10 8,4 14,10" />
                  </svg>
                ) : null}
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {getSortedRows(props.rows).map((row) => {
          const rowId = row.cells[0].toString();
          return (
            <tr
              onClick={() => onRowClicked(rowId)}
              data-rowid={rowId}
              class={
                rowId === props.selectedRow
                  ? "qs-resultsTable-sortedTableSelectedRow"
                  : undefined
              }
            >
              <td>
                <div
                  style="position: relative"
                  onClick={(e) => onClickRowMenu(e, rowId)}
                >
                  <svg width="16" height="16" style="position: relative;">
                    <path
                      stroke-width="1.5"
                      stroke={row.color}
                      stroke-linecap="round"
                      d="M4,5 h8 M4,8 h8 M4,11 h8"
                    />
                  </svg>
                  {showRowMenu === rowId ? (
                    <div
                      class="qs-resultsTable-showColumnMenu"
                      style="top: 16px; left: 0px;"
                    >
                      <div
                        class="qs-resultsTable-menuItem"
                        onClick={(e) => deleteRow(e, rowId)}
                      >
                        Delete
                      </div>
                    </div>
                  ) : null}
                </div>
              </td>
              {showColumns.map((idx) => {
                return (
                  <td data-colid={idx.toString()}>
                    {getCellStr(row.cells[idx])}
                  </td>
                );
              })}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
