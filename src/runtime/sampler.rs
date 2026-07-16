use std::{
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use tokio::time::Instant;

use crate::model::{LatencyBucket, MetricSnapshot};

#[derive(Debug)]
pub struct WorkloadSampler {
    run_id: String,
    run_workload_id: String,
    counters: Counters,
}

#[derive(Debug, Default)]
struct Counters {
    connected_current: AtomicU64,
    connected_total: AtomicU64,
    published: AtomicU64,
    received: AtomicU64,
    errors: AtomicU64,
    latency: StdMutex<LatencyWindow>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CounterSample {
    connected_current: u64,
    connected_total: u64,
    published: u64,
    received: u64,
    errors: u64,
}

#[derive(Debug, Default)]
struct LatencyWindow {
    total_count: u64,
    window_count: u64,
    window_sum_us: u64,
    window_min_us: u64,
    window_max_us: u64,
    window_values_us: Vec<u64>,
}

#[derive(Debug, Default, Clone)]
struct LatencySample {
    total_count: u64,
    window_count: u64,
    window_sum_us: u64,
    window_min_us: u64,
    window_max_us: u64,
    window_values_us: Vec<u64>,
}

impl WorkloadSampler {
    pub fn new(run_id: impl Into<String>, run_workload_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            run_workload_id: run_workload_id.into(),
            counters: Counters::default(),
        }
    }

    pub fn legacy(run_id: &str) -> Self {
        Self::new(run_id, format!("legacy-run-workload-{run_id}"))
    }

    pub fn client_connected(&self) {
        self.counters
            .connected_current
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .connected_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn client_disconnected(&self) {
        let _ = self.counters.connected_current.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |value| value.checked_sub(1),
        );
    }

    pub fn published(&self) {
        self.counters.published.fetch_add(1, Ordering::Relaxed);
    }

    pub fn received(&self) {
        self.counters.received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn latency(&self, latency: Duration) {
        let latency_us = latency.as_micros().min(u64::MAX as u128) as u64;
        let mut window = self
            .counters
            .latency
            .lock()
            .expect("latency mutex poisoned");
        window.total_count = window.total_count.saturating_add(1);
        window.window_count = window.window_count.saturating_add(1);
        window.window_sum_us = window.window_sum_us.saturating_add(latency_us);
        if window.window_min_us == 0 || latency_us < window.window_min_us {
            window.window_min_us = latency_us;
        }
        if latency_us > window.window_max_us {
            window.window_max_us = latency_us;
        }
        window.window_values_us.push(latency_us);
    }

    pub fn error(&self) {
        self.counters.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn sample(&self) -> CounterSample {
        CounterSample {
            connected_current: self.counters.connected_current.load(Ordering::Relaxed),
            connected_total: self.counters.connected_total.load(Ordering::Relaxed),
            published: self.counters.published.load(Ordering::Relaxed),
            received: self.counters.received.load(Ordering::Relaxed),
            errors: self.counters.errors.load(Ordering::Relaxed),
        }
    }

    pub fn snapshot(
        &self,
        started: Instant,
        previous: &mut CounterSample,
        previous_tick: &mut Instant,
    ) -> MetricSnapshot {
        let now = Instant::now();
        let elapsed = started.elapsed().as_millis() as u64;
        let sample = self.sample();
        let latency = self.drain_latency();
        let seconds = now.duration_since(*previous_tick).as_secs_f64().max(0.001);
        let latency_avg_ms = if latency.window_count > 0 {
            latency.window_sum_us as f64 / latency.window_count as f64 / 1000.0
        } else {
            0.0
        };
        let mut latency_values = latency.window_values_us;
        latency_values.sort_unstable();

        let publish_rate = sample.published.saturating_sub(previous.published) as f64 / seconds;
        let receive_rate = sample.received.saturating_sub(previous.received) as f64 / seconds;
        let connect_rate = sample
            .connected_total
            .saturating_sub(previous.connected_total) as f64
            / seconds;
        let error_rate = sample.errors.saturating_sub(previous.errors) as f64 / seconds;

        *previous = sample;
        *previous_tick = now;

        MetricSnapshot {
            run_id: self.run_id.clone(),
            run_workload_id: Some(self.run_workload_id.clone()),
            ts: Utc::now(),
            elapsed_ms: elapsed,
            connected: sample.connected_current,
            published: sample.published,
            received: sample.received,
            errors: sample.errors,
            publish_rate,
            receive_rate,
            connect_rate,
            error_rate,
            latency_count: latency.total_count,
            latency_window_count: latency.window_count,
            latency_window_sum_us: latency.window_sum_us,
            latency_histogram: latency_histogram(&latency_values),
            latency_avg_ms,
            latency_min_ms: latency.window_min_us as f64 / 1000.0,
            latency_p50_ms: percentile_ms(&latency_values, 0.50),
            latency_p90_ms: percentile_ms(&latency_values, 0.90),
            latency_p95_ms: percentile_ms(&latency_values, 0.95),
            latency_p99_ms: percentile_ms(&latency_values, 0.99),
            latency_p999_ms: percentile_ms(&latency_values, 0.999),
            latency_max_ms: latency.window_max_us as f64 / 1000.0,
        }
    }

    fn drain_latency(&self) -> LatencySample {
        let mut window = self
            .counters
            .latency
            .lock()
            .expect("latency mutex poisoned");
        let sample = LatencySample {
            total_count: window.total_count,
            window_count: window.window_count,
            window_sum_us: window.window_sum_us,
            window_min_us: window.window_min_us,
            window_max_us: window.window_max_us,
            window_values_us: std::mem::take(&mut window.window_values_us),
        };
        window.window_count = 0;
        window.window_sum_us = 0;
        window.window_min_us = 0;
        window.window_max_us = 0;
        sample
    }
}

fn latency_histogram(values_us: &[u64]) -> Vec<LatencyBucket> {
    let mut buckets = std::collections::BTreeMap::<u64, u64>::new();
    for value in values_us {
        let upper_bound = value.max(&1).next_power_of_two();
        *buckets.entry(upper_bound).or_default() += 1;
    }
    buckets
        .into_iter()
        .map(|(upper_bound_us, count)| LatencyBucket {
            upper_bound_us,
            count,
        })
        .collect()
}

fn percentile_ms(sorted_values_us: &[u64], quantile: f64) -> f64 {
    if sorted_values_us.is_empty() {
        return 0.0;
    }

    let rank = (quantile.clamp(0.0, 1.0) * sorted_values_us.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted_values_us.len() - 1);
    sorted_values_us[index] as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_builds_rates_and_latency_percentiles() {
        let sampler = WorkloadSampler::new("run-a", "workload-a");
        sampler.client_connected();
        sampler.published();
        sampler.received();
        sampler.latency(Duration::from_millis(2));
        sampler.latency(Duration::from_millis(8));

        let started = Instant::now() - Duration::from_secs(1);
        let mut previous = CounterSample::default();
        let mut previous_tick = Instant::now() - Duration::from_secs(1);
        let snapshot = sampler.snapshot(started, &mut previous, &mut previous_tick);

        assert_eq!(snapshot.run_id, "run-a");
        assert_eq!(snapshot.run_workload_id.as_deref(), Some("workload-a"));
        assert_eq!(snapshot.connected, 1);
        assert_eq!(snapshot.published, 1);
        assert_eq!(snapshot.received, 1);
        assert_eq!(snapshot.latency_count, 2);
        assert_eq!(snapshot.latency_p50_ms, 2.0);
        assert_eq!(snapshot.latency_p99_ms, 8.0);
    }
}
