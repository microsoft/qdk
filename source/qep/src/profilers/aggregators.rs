// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::TimeSeries;
use rustc_hash::FxHashMap;

pub enum AggregationType {
    /// `record_event` is called for single events.
    /// Reported time series are rates per second.
    Rate,

    /// `record_event` is called with durations of periods a unit is being used.
    /// Reported time series is the average number of units in use.
    /// For example, if there are 10 factories, and they are in use 20% of time and idle
    /// 80% of time, average number of factories in use is 2.
    Utilization,
}

/// Aggregates events into fixed, non-overlapping windows anchored at time zero.
/// Each metric includes zero-count windows through its last recorded event.
pub struct TumblingWindowAggregator {
    /// Aggregation window size in nanoseconds.
    window_size_nanos: usize,

    /// Counts per window for each event name.
    event_counts: FxHashMap<String, Vec<usize>>,

    agg_type: AggregationType,
}

impl TumblingWindowAggregator {
    /// Creates an aggregator with a nonzero window size and the specified normalization.
    ///
    /// # Panics
    ///
    /// Panics if `window_size_nanos` is zero.
    #[must_use]
    pub fn new(window_size_nanos: usize, agg_type: AggregationType) -> Self {
        assert!(window_size_nanos > 0, "window size must be nonzero");
        Self {
            window_size_nanos,
            event_counts: FxHashMap::default(),
            agg_type,
        }
    }

    /// Adds `delta` events; events on a window boundary belong to the next window.
    /// Events may be recorded out of timestamp order.
    pub fn record_event(&mut self, time_nanos: usize, event_name: &str, delta: usize) {
        let counts = match self.event_counts.get_mut(event_name) {
            Some(counts) => counts,
            None => self.event_counts.entry(event_name.to_owned()).or_default(),
        };
        let window_id = time_nanos / self.window_size_nanos;
        let window_count = window_id + 1;
        if counts.len() < window_count {
            counts.resize(window_count, 0);
        }
        counts[window_id] += delta;
    }

    /// Returns the sum of recorded deltas across all metrics and windows.
    #[must_use]
    pub fn total_event_count(&self) -> usize {
        self.event_counts.values().flatten().sum()
    }

    /// Returns event rates in events/second and factory utilization in factories.
    /// Profiles use the supplied group name and are ordered by metric name.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn get_profiles(&self, group_name: &str) -> Vec<TimeSeries> {
        let window_size_sec = self.window_size_nanos as f64 / 1e9;
        let mut profiles = Vec::with_capacity(self.event_counts.len());
        for (event_name, counts) in &self.event_counts {
            let (metric_name, metric_units, divisor) = match self.agg_type {
                AggregationType::Utilization => (
                    event_name.clone(),
                    "factories",
                    self.window_size_nanos as f64,
                ),
                AggregationType::Rate => {
                    (format!("{event_name} rate"), "events/s", window_size_sec)
                }
            };
            let mut series = Vec::with_capacity(counts.len());
            for (i, &count) in counts.iter().enumerate() {
                series.push((i as f64 * window_size_sec, count as f64 / divisor));
            }
            profiles.push(TimeSeries {
                metric_name,
                metric_units: metric_units.to_owned(),
                series,
                group_name: group_name.to_owned(),
            });
        }
        profiles.sort_by(|left, right| left.metric_name.cmp(&right.metric_name));

        profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks ten events across two metrics, including boundaries and empty windows.
    #[test]
    fn collects_two_event_metrics_across_windows() {
        let mut aggregator = TumblingWindowAggregator::new(500_000_000, AggregationType::Rate);
        assert_eq!(aggregator.total_event_count(), 0);
        assert!(aggregator.get_profiles("Event rates").is_empty());

        for time_nanos in [0, 1, 499_999_999, 500_000_000, 1_100_000_000, 1_000_000_000] {
            aggregator.record_event(time_nanos, "X gate", 1);
        }
        for time_nanos in [0, 100_000_000, 400_000_000, 1_000_000_000] {
            aggregator.record_event(time_nanos, "T gate", 1);
        }
        assert_eq!(aggregator.total_event_count(), 10);

        assert_eq!(
            aggregator.get_profiles("Event rates"),
            vec![
                TimeSeries {
                    metric_name: "T gate rate".to_owned(),
                    metric_units: "events/s".to_owned(),
                    group_name: "Event rates".to_owned(),
                    series: vec![(0.0, 6.0), (0.5, 0.0), (1.0, 2.0)],
                },
                TimeSeries {
                    metric_name: "X gate rate".to_owned(),
                    metric_units: "events/s".to_owned(),
                    group_name: "Event rates".to_owned(),
                    series: vec![(0.0, 6.0), (0.5, 2.0), (1.0, 4.0)],
                },
            ]
        );
    }

    /// Accumulates deltas into totals and rates, including out-of-order and zero updates.
    #[test]
    fn records_event_deltas() {
        let mut aggregator = TumblingWindowAggregator::new(500_000_000, AggregationType::Rate);
        for (time_nanos, delta) in [(0, 3), (500_000_000, 5), (100, 2), (1_000_000_000, 0)] {
            aggregator.record_event(time_nanos, "X gate", delta);
        }
        assert_eq!(aggregator.total_event_count(), 10);
        assert_eq!(
            aggregator.get_profiles("Event rates"),
            vec![TimeSeries {
                metric_name: "X gate rate".to_owned(),
                metric_units: "events/s".to_owned(),
                group_name: "Event rates".to_owned(),
                series: vec![(0.0, 10.0), (0.5, 10.0), (1.0, 0.0)],
            }]
        );
    }

    /// Normalizes busy nanoseconds by window duration regardless of metric name.
    #[test]
    fn reports_factory_utilization_in_factories() {
        let mut aggregator =
            TumblingWindowAggregator::new(500_000_000, AggregationType::Utilization);
        aggregator.record_event(0, "Factory utilization", 750_000_000);
        aggregator.record_event(1, "Factory utilization", 500_000_000);
        aggregator.record_event(1_000_000_000, "Factory utilization", 250_000_000);
        aggregator.record_event(0, "Other utilization", 250_000_000);
        assert_eq!(aggregator.total_event_count(), 1_750_000_000);
        assert_eq!(
            aggregator.get_profiles("Utilization"),
            vec![
                TimeSeries {
                    metric_name: "Factory utilization".to_owned(),
                    metric_units: "factories".to_owned(),
                    group_name: "Utilization".to_owned(),
                    series: vec![(0.0, 2.5), (0.5, 0.0), (1.0, 0.5)],
                },
                TimeSeries {
                    metric_name: "Other utilization".to_owned(),
                    metric_units: "factories".to_owned(),
                    group_name: "Utilization".to_owned(),
                    series: vec![(0.0, 0.5)],
                },
            ]
        );
    }

    /// Treats the factory-utilization name as an ordinary event metric in rate mode.
    #[test]
    fn rate_aggregation_does_not_special_case_metric_names() {
        let mut aggregator = TumblingWindowAggregator::new(500_000_000, AggregationType::Rate);
        aggregator.record_event(0, "Factory utilization", 3);
        assert_eq!(aggregator.total_event_count(), 3);
        assert_eq!(
            aggregator.get_profiles("Event rates"),
            vec![TimeSeries {
                metric_name: "Factory utilization rate".to_owned(),
                metric_units: "events/s".to_owned(),
                group_name: "Event rates".to_owned(),
                series: vec![(0.0, 6.0)],
            }]
        );
    }
}
