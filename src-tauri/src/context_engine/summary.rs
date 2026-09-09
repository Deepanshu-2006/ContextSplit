use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::context_engine::segments::{ContextActivitySegment, ContextActivitySegmenter};
use crate::context_engine::transitions::{ContextReference, ContextTransitionType};

/// Formats duration in a compact form:
/// - 0s -> "0m"
/// - < 60s -> "Xs"
/// - < 3600s -> "Xm"
/// - >= 3600s -> "Xh Ym" (or "Xh" if Y == 0)
pub fn format_duration_compact(seconds: i64) -> String {
    let s = seconds.abs();
    if s == 0 {
        return "0m".to_string();
    }
    if s < 60 {
        return format!("{}s", s);
    }
    let mins = s / 60;
    if mins < 60 {
        return format!("{}m", mins);
    }
    let hours = mins / 60;
    let rem_mins = mins % 60;
    if rem_mins > 0 {
        format!("{}h {}m", hours, rem_mins)
    } else {
        format!("{}h", hours)
    }
}

/// Aggregated usage statistics for a specific context within a time range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextUsageSummary {
    pub context: Option<ContextReference>,
    pub total_duration_seconds: i64,
    pub total_duration_formatted: String,
    pub percentage_of_total: f64,
    pub segment_count: usize,
    pub item_count: usize,
    pub average_confidence: f64,
}

/// Summary of the single longest continuous activity segment in the period.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LongestSegmentSummary {
    pub segment_id: String,
    pub context: Option<ContextReference>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub duration_formatted: String,
    pub item_count: usize,
}

/// A single step in the chronological context sequence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSequenceEntry {
    pub context: Option<ContextReference>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub duration_formatted: String,
    pub transition_type: Option<ContextTransitionType>,
}

/// Comprehensive summary of contextual activity across a time range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextActivitySummary {
    pub time_range_start: Option<String>,
    pub time_range_end: Option<String>,
    pub total_active_duration_seconds: i64,
    pub total_active_duration_formatted: String,
    pub contexts_used: Vec<ContextUsageSummary>,
    pub context_switches: usize,
    pub context_returns: usize,
    pub longest_segment: Option<LongestSegmentSummary>,
    pub most_recently_active_context: Option<ContextReference>,
    pub context_sequence: Vec<ContextSequenceEntry>,
    pub sequence_formatted: String,
    pub formatted_breakdown: String,
    pub evidence_grounding_note: String,
}

/// Service that aggregates activity segments into a coherent contextual summary.
#[derive(Debug, Default)]
pub struct ContextActivitySummaryService {
    segmenter: ContextActivitySegmenter,
}

impl ContextActivitySummaryService {
    /// Creates a new ContextActivitySummaryService with default configuration.
    pub fn new() -> Self {
        Self {
            segmenter: ContextActivitySegmenter::new(),
        }
    }

    /// Creates a service with a customized segmenter.
    pub fn with_segmenter(segmenter: ContextActivitySegmenter) -> Self {
        Self { segmenter }
    }

    /// Aggregates pre-computed activity segments, optionally bounding them to [start_iso, end_iso].
    pub fn summarize_segments(
        &self,
        segments: &[ContextActivitySegment],
        start_iso: Option<&str>,
        end_iso: Option<&str>,
    ) -> ContextActivitySummary {
        let start_dt = start_iso.and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let end_dt = end_iso.and_then(|s| s.parse::<DateTime<Utc>>().ok());

        // 1. Filter and clamp segments overlapping the specified time range
        let mut bounded_segments: Vec<(ContextActivitySegment, i64, String, String)> = Vec::new();

        for seg in segments {
            let seg_start_dt = match seg.started_at.parse::<DateTime<Utc>>() {
                Ok(dt) => dt,
                Err(_) => continue,
            };
            let seg_end_dt = match seg.ended_at.parse::<DateTime<Utc>>() {
                Ok(dt) => dt,
                Err(_) => seg_start_dt,
            };

            // Range overlap checks
            if let Some(s_dt) = start_dt {
                if seg_end_dt < s_dt {
                    continue; // Entirely before start
                }
            }
            if let Some(e_dt) = end_dt {
                if seg_start_dt > e_dt {
                    continue; // Entirely after end
                }
            }

            // Clamping
            let effective_start = match start_dt {
                Some(s_dt) if seg_start_dt < s_dt => s_dt,
                _ => seg_start_dt,
            };
            let effective_end = match end_dt {
                Some(e_dt) if seg_end_dt > e_dt => e_dt,
                _ => seg_end_dt,
            };

            let effective_duration = if seg.duration_seconds == 0 {
                0
            } else {
                effective_end
                    .signed_duration_since(effective_start)
                    .num_seconds()
                    .max(0)
            };

            bounded_segments.push((
                seg.clone(),
                effective_duration,
                effective_start.to_rfc3339(),
                effective_end.to_rfc3339(),
            ));
        }

        // 2. Aggregate total contextual active duration
        let total_active_duration_seconds: i64 = bounded_segments.iter().map(|(_, dur, _, _)| *dur).sum();
        let total_active_duration_formatted = format_duration_compact(total_active_duration_seconds);

        // 3. Aggregate contexts used
        struct ContextAccumulator {
            context: Option<ContextReference>,
            total_duration: i64,
            segment_count: usize,
            item_count: usize,
            confidence_sum: f64,
        }

        let mut context_map: HashMap<Option<String>, ContextAccumulator> = HashMap::new();

        for (seg, effective_dur, _, _) in &bounded_segments {
            let ctx_key = seg.context.as_ref().map(|c| c.id.clone());
            let entry = context_map.entry(ctx_key).or_insert_with(|| ContextAccumulator {
                context: seg.context.clone(),
                total_duration: 0,
                segment_count: 0,
                item_count: 0,
                confidence_sum: 0.0,
            });

            entry.total_duration += *effective_dur;
            entry.segment_count += 1;
            entry.item_count += seg.item_count;
            entry.confidence_sum += seg.confidence;
        }

        let mut contexts_used: Vec<ContextUsageSummary> = context_map
            .into_values()
            .map(|acc| {
                let percentage = if total_active_duration_seconds > 0 {
                    (acc.total_duration as f64 / total_active_duration_seconds as f64) * 100.0
                } else if !bounded_segments.is_empty() {
                    (acc.segment_count as f64 / bounded_segments.len() as f64) * 100.0
                } else {
                    0.0
                };
                let avg_conf = if acc.segment_count > 0 {
                    acc.confidence_sum / acc.segment_count as f64
                } else {
                    0.0
                };

                ContextUsageSummary {
                    context: acc.context,
                    total_duration_seconds: acc.total_duration,
                    total_duration_formatted: format_duration_compact(acc.total_duration),
                    percentage_of_total: (percentage * 10.0).round() / 10.0,
                    segment_count: acc.segment_count,
                    item_count: acc.item_count,
                    average_confidence: (avg_conf * 100.0).round() / 100.0,
                }
            })
            .collect();

        // Sort descending by duration, then by items
        contexts_used.sort_by(|a, b| {
            b.total_duration_seconds
                .cmp(&a.total_duration_seconds)
                .then_with(|| b.item_count.cmp(&a.item_count))
        });

        // 4. Transitions, switches & returns count
        let mut context_switches = 0;
        let mut context_returns = 0;
        let mut context_sequence: Vec<ContextSequenceEntry> = Vec::new();

        for (idx, (seg, effective_dur, eff_start, eff_end)) in bounded_segments.iter().enumerate() {
            let transition_type = seg.transition_in.as_ref().map(|t| t.transition_type.clone());

            if let Some(ref t_in) = seg.transition_in {
                match t_in.transition_type {
                    ContextTransitionType::ContextSwitch => context_switches += 1,
                    ContextTransitionType::ContextReturn => {
                        context_returns += 1;
                        context_switches += 1;
                    }
                    _ => {}
                }
            } else if idx > 0 {
                // Fallback: If no explicit transition_in recorded, check if context ID changed from previous segment
                let prev_ctx = bounded_segments[idx - 1].0.context.as_ref().map(|c| &c.id);
                let curr_ctx = seg.context.as_ref().map(|c| &c.id);
                if prev_ctx != curr_ctx && prev_ctx.is_some() && curr_ctx.is_some() {
                    context_switches += 1;
                }
            }

            context_sequence.push(ContextSequenceEntry {
                context: seg.context.clone(),
                started_at: eff_start.clone(),
                ended_at: eff_end.clone(),
                duration_seconds: *effective_dur,
                duration_formatted: format_duration_compact(*effective_dur),
                transition_type,
            });
        }

        // 5. Longest segment
        let longest_segment = bounded_segments
            .iter()
            .max_by_key(|(_, dur, _, _)| *dur)
            .map(|(seg, dur, eff_start, eff_end)| LongestSegmentSummary {
                segment_id: seg.id.clone(),
                context: seg.context.clone(),
                started_at: eff_start.clone(),
                ended_at: eff_end.clone(),
                duration_seconds: *dur,
                duration_formatted: format_duration_compact(*dur),
                item_count: seg.item_count,
            });

        // 6. Most recently active context
        let most_recently_active_context = bounded_segments
            .iter()
            .rev()
            .find_map(|(seg, _, _, _)| seg.context.clone());

        // 7. Sequence formatted text (e.g. "College → Portfolio → College → Client Website → Portfolio")
        let sequence_names: Vec<String> = context_sequence
            .iter()
            .map(|entry| {
                entry
                    .context
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| "Ambiguous Context".to_string())
            })
            .collect();
        let sequence_formatted = sequence_names.join(" → ");

        // 8. Human-readable breakdown text
        let mut breakdown_lines = Vec::new();
        for usage in &contexts_used {
            let name = usage
                .context
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("Ambiguous Activity");
            breakdown_lines.push(format!("{} — {}", name, usage.total_duration_formatted));
        }

        if !breakdown_lines.is_empty() {
            breakdown_lines.push(String::new());
        }
        breakdown_lines.push(format!("Context switches: {}", context_switches));
        if context_returns > 0 {
            breakdown_lines.push(format!("Context returns: {}", context_returns));
        }

        if !sequence_formatted.is_empty() {
            breakdown_lines.push(String::new());
            breakdown_lines.push("Sequence:".to_string());
            breakdown_lines.push(sequence_formatted.clone());
        }

        let formatted_breakdown = breakdown_lines.join("\n");

        let evidence_grounding_note = "Calculated from observed contextual activity events. Durations exclude detected inactivity periods and represent contextual activity rather than continuous human attention.".to_string();

        ContextActivitySummary {
            time_range_start: start_iso.map(|s| s.to_string()),
            time_range_end: end_iso.map(|s| s.to_string()),
            total_active_duration_seconds,
            total_active_duration_formatted,
            contexts_used,
            context_switches,
            context_returns,
            longest_segment,
            most_recently_active_context,
            context_sequence,
            sequence_formatted,
            formatted_breakdown,
            evidence_grounding_note,
        }
    }

    /// Derives and summarizes contextual activity for a specific session.
    pub fn summarize_session(
        &mut self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<ContextActivitySummary, String> {
        let segments = self.segmenter.build_segments_for_session(conn, session_id)?;
        Ok(self.summarize_segments(&segments, None, None))
    }

    /// Derives and summarizes contextual activity for a specific workspace.
    pub fn summarize_workspace(
        &mut self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<ContextActivitySummary, String> {
        let segments = self.segmenter.build_segments_for_workspace(conn, workspace_id)?;
        Ok(self.summarize_segments(&segments, None, None))
    }

    /// Derives and summarizes contextual activity across all activity within a specific ISO time range.
    pub fn summarize_time_range(
        &mut self,
        conn: &Connection,
        start_iso: &str,
        end_iso: &str,
    ) -> Result<ContextActivitySummary, String> {
        let segments = self.segmenter.build_all_segments(conn)?;
        Ok(self.summarize_segments(&segments, Some(start_iso), Some(end_iso)))
    }

    /// Derives and summarizes contextual activity across the user's entire history.
    pub fn summarize_all(
        &mut self,
        conn: &Connection,
    ) -> Result<ContextActivitySummary, String> {
        let segments = self.segmenter.build_all_segments(conn)?;
        Ok(self.summarize_segments(&segments, None, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::context_engine::segments::{ContextActivitySegment, SegmentItemReference};
    use crate::context_engine::transitions::{ContextReference, ContextTransitionType, DetectedTransition};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn make_test_segment(
        id: &str,
        ctx_id: Option<&str>,
        ctx_name: Option<&str>,
        started_at: &str,
        ended_at: &str,
        duration_seconds: i64,
        confidence: f64,
        transition_in: Option<ContextTransitionType>,
        items_count: usize,
    ) -> ContextActivitySegment {
        let context = ctx_id.map(|id| ContextReference::workspace(id, ctx_name.unwrap_or(id)));
        let t_in = transition_in.map(|tt| DetectedTransition {
            id: format!("trans_{}", id),
            from_context: None,
            to_context: context.clone(),
            timestamp: started_at.to_string(),
            confidence: 0.9,
            evidence: vec![],
            transition_type: tt,
            session_id: None,
            trigger_item_id: None,
            duration_since_previous_activity_seconds: None,
            metadata: serde_json::json!({}),
        });

        let item_references = (0..items_count)
            .map(|i| SegmentItemReference {
                item_id: format!("item_{}_{}", id, i),
                title: Some(format!("Item {}", i)),
                item_type: "text".to_string(),
                timestamp: started_at.to_string(),
                confidence,
            })
            .collect();

        ContextActivitySegment {
            id: id.to_string(),
            context,
            started_at: started_at.to_string(),
            ended_at: ended_at.to_string(),
            duration_seconds,
            duration_formatted: format_duration_compact(duration_seconds),
            confidence,
            session_ids: vec!["s1".to_string()],
            item_references,
            transition_in: t_in,
            transition_out: None,
            is_ambiguous: false,
            item_count: items_count,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_format_duration_compact() {
        assert_eq!(format_duration_compact(0), "0m");
        assert_eq!(format_duration_compact(45), "45s");
        assert_eq!(format_duration_compact(2880), "48m"); // 48m
        assert_eq!(format_duration_compact(5520), "1h 32m"); // 1h 32m
        assert_eq!(format_duration_compact(8040), "2h 14m"); // 2h 14m
        assert_eq!(format_duration_compact(7200), "2h"); // exactly 2h
    }

    #[test]
    fn test_empty_segments_summary() {
        let service = ContextActivitySummaryService::new();
        let summary = service.summarize_segments(&[], None, None);

        assert_eq!(summary.total_active_duration_seconds, 0);
        assert_eq!(summary.total_active_duration_formatted, "0m");
        assert!(summary.contexts_used.is_empty());
        assert_eq!(summary.context_switches, 0);
        assert_eq!(summary.context_returns, 0);
        assert!(summary.longest_segment.is_none());
        assert!(summary.most_recently_active_context.is_none());
        assert!(summary.context_sequence.is_empty());
        assert_eq!(summary.sequence_formatted, "");
    }

    #[test]
    fn test_single_context_continuous_activity() {
        let service = ContextActivitySummaryService::new();
        let seg = make_test_segment(
            "seg_1",
            Some("ws_portfolio"),
            Some("Portfolio"),
            "2026-09-10T10:00:00Z",
            "2026-09-10T12:14:00Z",
            8040, // 2h 14m
            0.92,
            Some(ContextTransitionType::ContextStart),
            8,
        );

        let summary = service.summarize_segments(&[seg], None, None);

        assert_eq!(summary.total_active_duration_seconds, 8040);
        assert_eq!(summary.total_active_duration_formatted, "2h 14m");
        assert_eq!(summary.contexts_used.len(), 1);
        assert_eq!(summary.contexts_used[0].context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(summary.contexts_used[0].total_duration_formatted, "2h 14m");
        assert_eq!(summary.contexts_used[0].percentage_of_total, 100.0);
        assert_eq!(summary.context_switches, 0);
        assert_eq!(summary.context_returns, 0);
        assert_eq!(summary.most_recently_active_context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(summary.sequence_formatted, "Portfolio");
    }

    #[test]
    fn test_multi_context_switches_returns_and_sequence() {
        // Sequence: College -> Portfolio -> College -> Client Website -> Portfolio
        let service = ContextActivitySummaryService::new();

        let segments = vec![
            make_test_segment(
                "s1",
                Some("ws_college"),
                Some("College"),
                "2026-09-10T09:00:00Z",
                "2026-09-10T09:50:00Z",
                3000, // 50m
                0.90,
                Some(ContextTransitionType::ContextStart),
                5,
            ),
            make_test_segment(
                "s2",
                Some("ws_portfolio"),
                Some("Portfolio"),
                "2026-09-10T10:00:00Z",
                "2026-09-10T11:14:00Z",
                4440, // 1h 14m
                0.95,
                Some(ContextTransitionType::ContextSwitch),
                10,
            ),
            make_test_segment(
                "s3",
                Some("ws_college"),
                Some("College"),
                "2026-09-10T11:30:00Z",
                "2026-09-10T12:12:00Z",
                2520, // 42m
                0.88,
                Some(ContextTransitionType::ContextReturn),
                4,
            ),
            make_test_segment(
                "s4",
                Some("ws_client"),
                Some("Client Website"),
                "2026-09-10T13:00:00Z",
                "2026-09-10T13:48:00Z",
                2880, // 48m
                0.91,
                Some(ContextTransitionType::ContextSwitch),
                6,
            ),
            make_test_segment(
                "s5",
                Some("ws_portfolio"),
                Some("Portfolio"),
                "2026-09-10T14:00:00Z",
                "2026-09-10T15:00:00Z",
                3600, // 1h 00m
                0.94,
                Some(ContextTransitionType::ContextReturn),
                8,
            ),
        ];

        let summary = service.summarize_segments(&segments, None, None);

        // Total active duration: 3000 + 4440 + 2520 + 2880 + 3600 = 16440s = 4h 34m
        assert_eq!(summary.total_active_duration_seconds, 16440);
        assert_eq!(summary.total_active_duration_formatted, "4h 34m");

        // Context totals:
        // Portfolio: 4440 + 3600 = 8040s (2h 14m)
        // College: 3000 + 2520 = 5520s (1h 32m)
        // Client Website: 2880s (48m)
        assert_eq!(summary.contexts_used.len(), 3);
        assert_eq!(summary.contexts_used[0].context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(summary.contexts_used[0].total_duration_formatted, "2h 14m");
        assert_eq!(summary.contexts_used[0].item_count, 18);
        assert_eq!(summary.contexts_used[0].segment_count, 2);

        assert_eq!(summary.contexts_used[1].context.as_ref().unwrap().name, "College");
        assert_eq!(summary.contexts_used[1].total_duration_formatted, "1h 32m");
        assert_eq!(summary.contexts_used[1].item_count, 9);
        assert_eq!(summary.contexts_used[1].segment_count, 2);

        assert_eq!(summary.contexts_used[2].context.as_ref().unwrap().name, "Client Website");
        assert_eq!(summary.contexts_used[2].total_duration_formatted, "48m");
        assert_eq!(summary.contexts_used[2].item_count, 6);
        assert_eq!(summary.contexts_used[2].segment_count, 1);

        // Switches and Returns:
        // s1: Start (0 switches)
        // s2: Switch (1 switch)
        // s3: Return (1 return, 1 switch)
        // s4: Switch (1 switch)
        // s5: Return (1 return, 1 switch)
        // Total switches = 4, Total returns = 2
        assert_eq!(summary.context_switches, 4);
        assert_eq!(summary.context_returns, 2);

        // Longest segment: s2 (4440s, Portfolio)
        let longest = summary.longest_segment.as_ref().unwrap();
        assert_eq!(longest.segment_id, "s2");
        assert_eq!(longest.context.as_ref().unwrap().name, "Portfolio");
        assert_eq!(longest.duration_formatted, "1h 14m");

        // Most recently active context: Portfolio (from s5)
        assert_eq!(summary.most_recently_active_context.as_ref().unwrap().name, "Portfolio");

        // Sequence:
        assert_eq!(
            summary.sequence_formatted,
            "College → Portfolio → College → Client Website → Portfolio"
        );

        // Check formatted breakdown text contains the key components
        assert!(summary.formatted_breakdown.contains("Portfolio — 2h 14m"));
        assert!(summary.formatted_breakdown.contains("College — 1h 32m"));
        assert!(summary.formatted_breakdown.contains("Client Website — 48m"));
        assert!(summary.formatted_breakdown.contains("Context switches: 4"));
        assert!(summary.formatted_breakdown.contains("Sequence:\nCollege → Portfolio → College → Client Website → Portfolio"));
    }

    #[test]
    fn test_time_range_filtering_and_clamping() {
        let service = ContextActivitySummaryService::new();

        let segments = vec![
            make_test_segment(
                "s1",
                Some("ws_a"),
                Some("Early"),
                "2026-09-10T08:00:00Z",
                "2026-09-10T09:00:00Z",
                3600,
                0.9,
                None,
                5,
            ),
            make_test_segment(
                "s2",
                Some("ws_b"),
                Some("Mid"),
                "2026-09-10T09:30:00Z",
                "2026-09-10T11:00:00Z",
                5400, // 1.5h
                0.9,
                None,
                10,
            ),
            make_test_segment(
                "s3",
                Some("ws_c"),
                Some("Late"),
                "2026-09-10T12:00:00Z",
                "2026-09-10T13:00:00Z",
                3600,
                0.9,
                None,
                5,
            ),
        ];

        // Query range 10:00 to 11:30 (clamps s2 from 10:00 to 11:00 = 1h = 3600s; excludes s1 and s3)
        let summary = service.summarize_segments(
            &segments,
            Some("2026-09-10T10:00:00Z"),
            Some("2026-09-10T11:30:00Z"),
        );

        assert_eq!(summary.contexts_used.len(), 1);
        assert_eq!(summary.contexts_used[0].context.as_ref().unwrap().name, "Mid");
        assert_eq!(summary.total_active_duration_seconds, 3600); // exactly 1 hour clamped
        assert_eq!(summary.total_active_duration_formatted, "1h");
    }

    #[test]
    fn test_database_integration_summary() {
        let conn = setup_test_db();

        let ws_portfolio = crate::db::operations::create_workspace(&conn, "Portfolio").unwrap();
        let ws_college = crate::db::operations::create_workspace(&conn, "College").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

        // Add items to simulate contextual work
        crate::db::operations::create_item(&conn, Some(&ws_college.id), Some(&session.id), "text", "Assignment", None, "{}").unwrap();
        crate::db::operations::create_item(&conn, Some(&ws_college.id), Some(&session.id), "text", "Lecture notes", None, "{}").unwrap();
        crate::db::operations::create_item(&conn, Some(&ws_portfolio.id), Some(&session.id), "text", "React website", None, "{}").unwrap();

        let mut service = ContextActivitySummaryService::new();
        let summary = service.summarize_session(&conn, &session.id).unwrap();

        assert_eq!(summary.contexts_used.len(), 2);
        assert_eq!(summary.context_sequence.len(), 2);
        assert_eq!(summary.sequence_formatted, "College → Portfolio");
        assert!(summary.evidence_grounding_note.contains("contextual activity"));
    }
}
