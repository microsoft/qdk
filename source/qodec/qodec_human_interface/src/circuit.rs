// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A boundary's calls drawn as a circuit: one row per block id, one column
//! per moment. A wire is drawn only while its block is live, so the picture
//! is also the state: where a wire starts a block was given, where it stops
//! the block was taken.
//!
//! Each call goes in the earliest column after every earlier call on the rows
//! it spans, so the calls on one wire keep their order, and calls on separate
//! wires that could run together share a column. Batches are chunks of time:
//! a batch starts after every call of the batch before it, so the picture
//! reads left to right in the order things happened.
//!
//! ```text
//!  3       R──CX──M→0
//!  0 ──────────CX───────
//! ```

use crate::state::Wiring;
use std::ops::Range;

/// Where each call of a boundary goes.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// The block ids, one per row, in increasing order.
    pub rows: Vec<u64>,
    /// Per column, its calls, in time order.
    pub columns: Vec<Vec<usize>>,
    /// Per column, its width in characters, without the gap after it.
    pub widths: Vec<usize>,
    /// Per call: its column.
    pub column_of: Vec<usize>,
    /// Per call: the rows it spans, from its first operand's row to its last.
    pub spans: Vec<Option<Range<usize>>>,
    /// Per call: the rows of its operands.
    pub operand_rows: Vec<Vec<usize>>,
    /// Per row: its calls, as (column, call), in time order.
    pub row_calls: Vec<Vec<(usize, usize)>>,
}

impl Layout {
    /// Lays out calls with `wiring`, in batches `batch`, whose labels are
    /// `label_width` wide.
    #[must_use]
    pub fn new(
        wiring: &[Wiring],
        batch: impl Fn(usize) -> usize,
        label_width: impl Fn(usize) -> usize,
    ) -> Self {
        let mut rows: Vec<u64> = wiring.iter().flat_map(Wiring::blocks).collect();
        rows.sort_unstable();
        rows.dedup();
        let row = |label: u64| rows.binary_search(&label).expect("every label has a row");

        let mut layout = Layout {
            row_calls: vec![Vec::new(); rows.len()],
            ..Layout::default()
        };
        // The first free column of each row.
        let mut free = vec![0usize; rows.len()];
        let mut last_column = 0;
        for (e, w) in wiring.iter().enumerate() {
            if e > 0 && batch(e) != batch(e - 1) {
                let start = free.iter().copied().max().unwrap_or(0);
                free.fill(start);
            }
            let operand_rows: Vec<usize> = w.blocks().map(row).collect();
            let span = operand_rows
                .iter()
                .min()
                .zip(operand_rows.iter().max())
                .map(|(&lo, &hi)| lo..hi + 1);
            let column = match &span {
                Some(span) => free[span.clone()].iter().copied().max().unwrap_or(0),
                // A call on no block keeps its place in time.
                None => free.iter().copied().max().unwrap_or(last_column),
            };
            match &span {
                Some(span) => free[span.clone()].fill(column + 1),
                None => free.fill(column + 1),
            }
            last_column = column;
            if layout.columns.len() <= column {
                layout.columns.resize(column + 1, Vec::new());
                layout.widths.resize(column + 1, 1);
            }
            layout.columns[column].push(e);
            layout.widths[column] = layout.widths[column].max(label_width(e));
            layout.column_of.push(column);
            for &r in &operand_rows {
                layout.row_calls[r].push((column, e));
            }
            layout.operand_rows.push(operand_rows);
            layout.spans.push(span);
        }
        layout.rows = rows;
        layout
    }

    /// The call on `row` in `column`, if any.
    fn call_at(&self, row: usize, column: usize) -> Option<usize> {
        let calls = &self.row_calls[row];
        let k = calls.partition_point(|&(c, _)| c < column);
        calls.get(k).filter(|&&(c, _)| c == column).map(|&(_, e)| e)
    }

    /// Whether the block on `row` is live on entering `column`: the last call
    /// on the row before it gave the block.
    fn live_into(&self, wiring: &[Wiring], row: usize, column: usize) -> bool {
        let calls = &self.row_calls[row];
        let k = calls.partition_point(|&(c, _)| c < column);
        k.checked_sub(1)
            .is_some_and(|k| wiring[calls[k].1].outs.contains(&self.rows[row]))
    }

    /// The call whose vertical link crosses `row` in `column`, if any.
    fn link_at(&self, row: usize, column: usize) -> Option<usize> {
        self.columns[column].iter().copied().find(|&e| {
            self.spans[e]
                .as_ref()
                .is_some_and(|s| s.start < row && row < s.end - 1)
                && !self.operand_rows[e].contains(&row)
        })
    }
}

/// What a piece of a drawn row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// A live wire.
    Wire,
    /// No live block.
    Gap,
    /// A call's label, on one of its operands' rows.
    Gate(usize),
    /// A call's vertical link, across a row between its operands.
    Link(usize),
}

/// A piece of a drawn row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// What it is.
    pub ink: Ink,
    /// Its text.
    pub text: String,
}

/// Draws `row` over `columns`, with calls labelled by `label`.
pub fn draw_row(
    layout: &Layout,
    wiring: &[Wiring],
    row: usize,
    columns: Range<usize>,
    label: impl Fn(usize) -> String,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut push = |ink: Ink, text: String| match segments.last_mut() {
        Some(last) if last.ink == ink && matches!(ink, Ink::Wire | Ink::Gap) => {
            last.text.push_str(&text);
        }
        _ => segments.push(Segment { ink, text }),
    };
    let label_of_row = layout.rows[row];
    let mut live = layout.live_into(wiring, row, columns.start);
    for column in columns {
        let width = layout.widths[column] + 1;
        let fill = |live: bool, n: usize| {
            if live {
                (Ink::Wire, "─".repeat(n))
            } else {
                (Ink::Gap, " ".repeat(n))
            }
        };
        if let Some(e) = layout.call_at(row, column) {
            let text = label(e);
            let n = text.chars().count();
            push(Ink::Gate(e), text);
            live = wiring[e].outs.contains(&label_of_row);
            let (ink, text) = fill(live, width.saturating_sub(n));
            push(ink, text);
        } else if let Some(e) = layout.link_at(row, column) {
            push(Ink::Link(e), if live { "┼" } else { "│" }.to_string());
            let (ink, text) = fill(live, width - 1);
            push(ink, text);
        } else {
            let (ink, text) = fill(live, width);
            push(ink, text);
        }
    }
    segments
}
