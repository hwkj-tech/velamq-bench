use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum LoadShape {
    Flat {
        rate: f64,
    },
    Ramp {
        from: f64,
        to: f64,
        duration_ms: u64,
    },
    Step {
        stages: Vec<LoadStage>,
    },
    Soak {
        rate: f64,
        duration_ms: u64,
    },
    Spike {
        baseline: f64,
        peak: f64,
        peak_duration_ms: u64,
        period_ms: u64,
    },
}

impl Default for LoadShape {
    fn default() -> Self {
        Self::Flat { rate: 1.0 }
    }
}

impl LoadShape {
    pub fn instant_rate(&self, elapsed_ms: u64) -> f64 {
        match self {
            Self::Flat { rate } => *rate,
            Self::Ramp {
                from,
                to,
                duration_ms,
            } => {
                if *duration_ms == 0 || elapsed_ms >= *duration_ms {
                    return *to;
                }
                let progress = elapsed_ms as f64 / *duration_ms as f64;
                from + (to - from) * progress
            }
            Self::Step { stages } => {
                let mut cursor = 0_u64;
                for stage in stages {
                    cursor = cursor.saturating_add(stage.duration_ms);
                    if elapsed_ms < cursor {
                        return stage.rate;
                    }
                }
                stages.last().map(|stage| stage.rate).unwrap_or(0.0)
            }
            Self::Soak { rate, duration_ms } => {
                if *duration_ms == 0 || elapsed_ms <= *duration_ms {
                    *rate
                } else {
                    0.0
                }
            }
            Self::Spike {
                baseline,
                peak,
                peak_duration_ms,
                period_ms,
            } => {
                if *period_ms == 0 {
                    return *baseline;
                }
                let offset = elapsed_ms % *period_ms;
                if offset < *peak_duration_ms {
                    *peak
                } else {
                    *baseline
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoadStage {
    pub rate: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LoadProfile {
    pub connect_shape: LoadShape,
    pub message_shape: LoadShape,
    pub total_duration_ms: u64,
}

impl Default for LoadProfile {
    fn default() -> Self {
        Self {
            connect_shape: LoadShape::Flat { rate: 100.0 },
            message_shape: LoadShape::Flat { rate: 1.0 },
            total_duration_ms: 60_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_shape_round_trips_through_json() {
        let shapes = vec![
            LoadShape::Flat { rate: 10.0 },
            LoadShape::Ramp {
                from: 1.0,
                to: 9.0,
                duration_ms: 8000,
            },
            LoadShape::Step {
                stages: vec![
                    LoadStage {
                        rate: 1.0,
                        duration_ms: 1000,
                    },
                    LoadStage {
                        rate: 2.0,
                        duration_ms: 2000,
                    },
                ],
            },
            LoadShape::Soak {
                rate: 4.0,
                duration_ms: 1000,
            },
            LoadShape::Spike {
                baseline: 1.0,
                peak: 20.0,
                peak_duration_ms: 100,
                period_ms: 1000,
            },
        ];

        for shape in shapes {
            let json = serde_json::to_string(&shape).unwrap();
            let decoded: LoadShape = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, shape);
        }
    }

    #[test]
    fn instant_rate_follows_shape_math() {
        assert_eq!(LoadShape::Flat { rate: 3.0 }.instant_rate(999), 3.0);
        assert_eq!(
            LoadShape::Ramp {
                from: 0.0,
                to: 10.0,
                duration_ms: 1000,
            }
            .instant_rate(500),
            5.0
        );
        assert_eq!(
            LoadShape::Step {
                stages: vec![
                    LoadStage {
                        rate: 2.0,
                        duration_ms: 100,
                    },
                    LoadStage {
                        rate: 8.0,
                        duration_ms: 100,
                    },
                ],
            }
            .instant_rate(120),
            8.0
        );
        assert_eq!(
            LoadShape::Soak {
                rate: 7.0,
                duration_ms: 100,
            }
            .instant_rate(101),
            0.0
        );
        assert_eq!(
            LoadShape::Spike {
                baseline: 1.0,
                peak: 9.0,
                peak_duration_ms: 50,
                period_ms: 100,
            }
            .instant_rate(225),
            9.0
        );
    }
}
