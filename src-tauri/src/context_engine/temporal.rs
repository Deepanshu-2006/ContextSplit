use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Configuration for temporal context weighting.
/// Allows customization of decay curves, half-life periods, and minimum relevance floors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemporalDecayConfig {
    /// Decay curve: "exponential" (default), "linear", "step", or "none"
    pub strategy: String,
    /// Half-life in days for exponential decay (default: 14.0 days)
    pub half_life_days: f64,
    /// Minimum floor weight (0.0 to 1.0) so historical context is never completely erased (default: 0.20)
    pub min_weight_floor: f64,
    /// Grace period in hours where activity receives full 1.0 weight (default: 24.0 hours)
    pub grace_period_hours: f64,
}

impl Default for TemporalDecayConfig {
    fn default() -> Self {
        Self {
            strategy: "exponential".to_string(),
            half_life_days: 14.0,
            min_weight_floor: 0.20,
            grace_period_hours: 24.0,
        }
    }
}

impl TemporalDecayConfig {
    /// Computes the temporal weight between min_weight_floor and 1.0,
    /// along with a concise, human-readable explanation.
    pub fn compute_weight_and_explanation(
        &self,
        timestamp_str: &str,
        now: DateTime<Utc>,
    ) -> (f64, String) {
        let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return (1.0, "Recent activity (current)".to_string()),
        };

        let duration = now.signed_duration_since(timestamp);
        let total_seconds = duration.num_seconds().max(0) as f64;
        let hours_elapsed = total_seconds / 3600.0;
        let days_elapsed = hours_elapsed / 24.0;

        if self.strategy == "none" {
            return (1.0, "Temporal decay disabled (100% weight)".to_string());
        }

        // Within grace period (e.g. 24h) -> full 1.0 weight
        if hours_elapsed <= self.grace_period_hours {
            let expl = if hours_elapsed < 1.0 {
                "Recent activity (past hour): 100% temporal relevance".to_string()
            } else {
                format!("Recent activity ({:.0}h ago): 100% temporal relevance", hours_elapsed)
            };
            return (1.0, expl);
        }

        let effective_hours = hours_elapsed - self.grace_period_hours;
        let effective_days = effective_hours / 24.0;

        let raw_decay = match self.strategy.as_str() {
            "linear" => {
                let max_days = (self.half_life_days * 2.0).max(1.0);
                (1.0 - (effective_days / max_days)).max(0.0)
            }
            "step" => {
                if days_elapsed > 30.0 {
                    0.0
                } else if days_elapsed > 7.0 {
                    0.4
                } else {
                    0.8
                }
            }
            _ => {
                // Exponential decay: 0.5 ^ (effective_days / half_life_days)
                let hl = if self.half_life_days <= 0.0 { 14.0 } else { self.half_life_days };
                0.5_f64.powf(effective_days / hl)
            }
        };

        // Scale between min_weight_floor and 1.0
        let weight = self.min_weight_floor + (1.0 - self.min_weight_floor) * raw_decay;
        let clamped_weight = weight.clamp(self.min_weight_floor, 1.0);

        let expl = if days_elapsed < 2.0 {
            format!("Activity from yesterday: {:.0}% temporal relevance", clamped_weight * 100.0)
        } else if days_elapsed < 7.0 {
            format!("Activity from {:.0} days ago: {:.0}% temporal relevance", days_elapsed, clamped_weight * 100.0)
        } else {
            format!(
                "Historical activity ({:.0} days ago): discounted to {:.0}% temporal relevance",
                days_elapsed,
                clamped_weight * 100.0
            )
        };

        (clamped_weight, expl)
    }

    /// Convenience helper for just the numeric weight
    pub fn compute_weight(&self, timestamp_str: &str, now: DateTime<Utc>) -> f64 {
        self.compute_weight_and_explanation(timestamp_str, now).0
    }
}

/// Retrieves the configured temporal decay settings from the database,
/// falling back to default values if not configured.
pub fn get_temporal_decay_config(conn: &Connection) -> TemporalDecayConfig {
    let mut stmt = match conn.prepare("SELECT temporal_decay_config FROM settings WHERE id = 'default'") {
        Ok(s) => s,
        Err(_) => return TemporalDecayConfig::default(),
    };

    let res = stmt.query_row([], |row| {
        let raw: Option<String> = row.get(0)?;
        Ok(raw)
    });

    if let Ok(Some(json_str)) = res {
        if let Ok(config) = serde_json::from_str::<TemporalDecayConfig>(&json_str) {
            return config;
        }
    }

    TemporalDecayConfig::default()
}

/// Updates the temporal decay settings in the database.
pub fn update_temporal_decay_config(
    conn: &Connection,
    config: &TemporalDecayConfig,
) -> Result<(), String> {
    let json_str = serde_json::to_string(config).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE settings SET temporal_decay_config = ?1 WHERE id = 'default'",
        rusqlite::params![json_str],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_temporal_decay_recent_within_grace_period() {
        let config = TemporalDecayConfig::default();
        let now = Utc::now();

        // 2 hours ago (within 24h grace period)
        let recent_time = (now - Duration::hours(2)).to_rfc3339();
        let (weight, expl) = config.compute_weight_and_explanation(&recent_time, now);

        assert_eq!(weight, 1.0, "Recent activity should have 100% weight");
        assert!(expl.contains("100% temporal relevance"));
    }

    #[test]
    fn test_temporal_decay_exponential_half_life() {
        let config = TemporalDecayConfig {
            strategy: "exponential".to_string(),
            half_life_days: 14.0,
            min_weight_floor: 0.20,
            grace_period_hours: 24.0,
        };
        let now = Utc::now();

        // 15 days ago = 1 day grace + 14 days (1 half life)
        let half_life_time = (now - Duration::days(15)).to_rfc3339();
        let (weight, expl) = config.compute_weight_and_explanation(&half_life_time, now);

        // Raw decay after 1 half-life is 0.5.
        // Scaled weight = 0.20 + 0.80 * 0.5 = 0.60
        assert!((weight - 0.60).abs() < 0.05, "Weight after 1 half-life should be ~0.60, got: {}", weight);
        assert!(expl.contains("temporal relevance"));
    }

    #[test]
    fn test_temporal_decay_historical_floor() {
        let config = TemporalDecayConfig {
            strategy: "exponential".to_string(),
            half_life_days: 14.0,
            min_weight_floor: 0.20,
            grace_period_hours: 24.0,
        };
        let now = Utc::now();

        // 180 days ago (very old historical activity)
        let very_old_time = (now - Duration::days(180)).to_rfc3339();
        let (weight, expl) = config.compute_weight_and_explanation(&very_old_time, now);

        // Never drops below min_weight_floor of 0.20
        assert!((weight - 0.20).abs() < 0.01, "Old activity should respect floor (0.20), got: {}", weight);
        assert!(expl.contains("Historical activity"));
        assert!(expl.contains("20% temporal relevance"));
    }

    #[test]
    fn test_temporal_decay_strategies() {
        let now = Utc::now();
        let time_10d_ago = (now - Duration::days(10)).to_rfc3339();

        // Linear strategy
        let linear_config = TemporalDecayConfig {
            strategy: "linear".to_string(),
            half_life_days: 10.0,
            min_weight_floor: 0.10,
            grace_period_hours: 0.0,
        };
        let (lin_w, _) = linear_config.compute_weight_and_explanation(&time_10d_ago, now);
        // Linear: 10 days / 20 days max = 0.5 raw -> 0.10 + 0.90 * 0.5 = 0.55
        assert!((lin_w - 0.55).abs() < 0.05);

        // Disabled strategy ("none")
        let none_config = TemporalDecayConfig {
            strategy: "none".to_string(),
            ..Default::default()
        };
        let (none_w, _) = none_config.compute_weight_and_explanation(&time_10d_ago, now);
        assert_eq!(none_w, 1.0);
    }
}
