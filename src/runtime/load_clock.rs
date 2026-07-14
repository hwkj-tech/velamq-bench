#![allow(dead_code)]

use std::time::Duration;

use tokio::time::Instant;

use crate::model::LoadShape;

#[derive(Debug, Clone)]
pub struct LoadClock {
    started: Instant,
    shape: LoadShape,
}

impl LoadClock {
    pub fn new(shape: LoadShape) -> Self {
        Self {
            started: Instant::now(),
            shape,
        }
    }

    pub fn instant_rate(&self, now: Instant) -> f64 {
        let elapsed_ms = now.duration_since(self.started).as_millis() as u64;
        self.shape.instant_rate(elapsed_ms)
    }

    pub fn interval_for_rate(rate: f64) -> Duration {
        if rate <= 0.0 {
            Duration::from_secs(1)
        } else {
            Duration::from_secs_f64(1.0 / rate.max(0.001))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LoadShape, LoadStage};

    #[test]
    fn interval_for_rate_caps_zero_to_one_second() {
        assert_eq!(LoadClock::interval_for_rate(0.0), Duration::from_secs(1));
        assert_eq!(
            LoadClock::interval_for_rate(2.0),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn clock_reads_current_shape_rate() {
        let clock = LoadClock::new(LoadShape::Step {
            stages: vec![
                LoadStage {
                    rate: 1.0,
                    duration_ms: 1000,
                },
                LoadStage {
                    rate: 5.0,
                    duration_ms: 1000,
                },
            ],
        });
        assert_eq!(clock.instant_rate(Instant::now()), 1.0);
    }
}
