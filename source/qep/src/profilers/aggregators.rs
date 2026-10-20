// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{TemporalProfiles, TimeSeries};
use rustc_hash::FxHashMap;

/// Aggregates events into fixed, non-overlapping windows anchored at time zero.
/// Each metric includes zero-count windows through its last recorded event.
pub struct TumblingWindowAggregator {
    /// Aggregation window size in nanoseconds.
    window_size_nanos: usize,

    /// Counts per window for each event name.
    event_counts: FxHashMap<String, Vec<usize>>,
}

impl TumblingWindowAggregator {
    /// Creates an aggregator with a nonzero window size.
    ///
    /// # Panics
    ///
    /// Panics if `window_size_nanos` is zero.
    #[must_use]
    pub fn new(window_size_nanos: usize) -> Self {
        assert!(window_size_nanos > 0, "window size must be nonzero");
        Self {
            window_size_nanos,
            event_counts: FxHashMap::default(),
        }
    }

    /// Records an event; events on a window boundary belong to the next window.
    /// Events may be recorded out of timestamp order.
    pub fn record_event(&mut self, time_nanos: usize, event_name: &str) {
        let counts = match self.event_counts.get_mut(event_name) {
            Some(counts) => counts,
            None => self.event_counts.entry(event_name.to_owned()).or_default(),
        };
        let window_id = time_nanos / self.window_size_nanos;
        let window_count = window_id+1;
        if counts.len() < window_count {
            counts.resize(window_count, 0);
        }
        counts[window_id] += 1;
    }

    /// Returns event rates in events/second at each window's start time.
    /// Profiles are ordered by metric name.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn get_profiles(&self) -> TemporalProfiles {
        let window_size_sec = self.window_size_nanos as f64 / 1e9;
        let mut profiles: Vec<_> = self
            .event_counts
            .iter()
            .map(|(event_name, counts)| TimeSeries {
                metric_name: format!("{event_name} rate"),
                metric_units: "events/s".to_owned(),
                series: counts
                    .iter()
                    .enumerate()
                    .map(|(i, &count)| (i as f64 * window_size_sec, count as f64 / window_size_sec))
                    .collect(),
            })
            .collect();
        profiles.sort_by(|left, right| left.metric_name.cmp(&right.metric_name));

        TemporalProfiles {
            profiles,
            aggregation_window_sec: Some(window_size_sec),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks ten events across two metrics, including boundaries and empty windows.
    #[test]
    fn collects_two_event_metrics_across_windows() {
        let mut aggregator = TumblingWindowAggregator::new(500_000_000);
        assert_eq!(
            aggregator.get_profiles(),
            TemporalProfiles {
                profiles: vec![],
                aggregation_window_sec: Some(0.5),
            }
        );

        for time_nanos in [0, 1, 499_999_999, 500_000_000, 1_100_000_000, 1_000_000_000] {
            aggregator.record_event(time_nanos, "X gate");
        }
        for time_nanos in [0, 100_000_000, 400_000_000, 1_000_000_000] {
            aggregator.record_event(time_nanos, "T gate");
        }

        assert_eq!(
            aggregator.get_profiles(),
            TemporalProfiles {
                profiles: vec![
                    TimeSeries {
                        metric_name: "T gate rate".to_owned(),
                        metric_units: "events/s".to_owned(),
                        series: vec![(0.0, 6.0), (0.5, 0.0), (1.0, 2.0)],
                    },
                    TimeSeries {
                        metric_name: "X gate rate".to_owned(),
                        metric_units: "events/s".to_owned(),
                        series: vec![(0.0, 6.0), (0.5, 2.0), (1.0, 4.0)],
                    },
                ],
                aggregation_window_sec: Some(0.5),
            }
        );
    }
}
