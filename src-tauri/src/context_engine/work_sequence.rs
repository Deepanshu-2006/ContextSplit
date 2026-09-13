use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::context_engine::memory::{InformationProvenance, MemoryItemReference};
use crate::context_engine::segments::{ContextActivitySegment, ContextActivitySegmenter};
use crate::context_engine::task_extractor::is_generic_task_name;
use crate::context_engine::transitions::ContextTransitionType;

// =========================================================================
// Duration & Format Helpers (Non-Intrusive, Epistemically Honest)
// =========================================================================

/// Formats duration into a gentle, non-intrusive estimate.
/// Avoids exact attention claims like "You spent 40 minutes focused."
pub fn format_activity_duration_estimate(seconds: i64) -> String {
    let s = seconds.abs();
    if s < 60 {
        return "Active for less than a minute".to_string();
    }
    let mins = s / 60;
    if mins < 5 {
        return "Active for a few minutes".to_string();
    }
    if mins < 60 {
        let rounded_mins = ((mins + 2) / 5) * 5;
        return format!("Active for about {} minutes", rounded_mins);
    }
    let hours = mins / 60;
    let rem_mins = mins % 60;
    if rem_mins < 10 {
        if hours == 1 {
            "Active for about 1 hour".to_string()
        } else {
            format!("Active for about {} hours", hours)
        }
    } else {
        let rounded_rem = ((rem_mins + 2) / 5) * 5;
        if hours == 1 {
            format!("Active for about 1 hour {} minutes", rounded_rem)
        } else {
            format!("Active for about {} hours {} minutes", hours, rounded_rem)
        }
    }
}

/// Helper to format time window string (e.g. "09:00–10:20").
pub fn format_time_window(start_iso: &str, end_iso: &str) -> String {
    let start_fmt = parse_time_hh_mm(start_iso).unwrap_or_else(|| "Unknown".to_string());
    let end_fmt = parse_time_hh_mm(end_iso).unwrap_or_else(|| "Unknown".to_string());
    format!("{}–{}", start_fmt, end_fmt)
}

fn parse_time_hh_mm(iso_str: &str) -> Option<String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_str) {
        return Some(dt.format("%H:%M").to_string());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(iso_str, "%Y-%m-%d %H:%M:%S") {
        return Some(dt.format("%H:%M").to_string());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(iso_str, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(dt.format("%H:%M").to_string());
    }
    None
}

/// Helper to format date label relative to a reference time (e.g. "Today", "Yesterday", "Sep 10").
pub fn format_date_label(iso_date: &str, now: Option<DateTime<Utc>>) -> (String, String) {
    let dt = DateTime::parse_from_rfc3339(iso_date)
        .map(|d| d.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(iso_date, "%Y-%m-%d %H:%M:%S")
                .map(|d| DateTime::<Utc>::from_naive_utc_and_offset(d, Utc))
        })
        .or_else(|_| {
            NaiveDateTime::parse_from_str(iso_date, "%Y-%m-%dT%H:%M:%SZ")
                .map(|d| DateTime::<Utc>::from_naive_utc_and_offset(d, Utc))
        })
        .unwrap_or_else(|_| Utc::now());

    let current = now.unwrap_or_else(Utc::now);
    let date_key = dt.format("%Y-%m-%d").to_string();
    let current_key = current.format("%Y-%m-%d").to_string();

    let diff_days = (current.date_naive() - dt.date_naive()).num_days();

    let label = if date_key == current_key || diff_days == 0 {
        "Today".to_string()
    } else if diff_days == 1 {
        "Yesterday".to_string()
    } else if diff_days < 7 {
        dt.format("%A").to_string() // e.g. "Monday"
    } else if dt.format("%Y").to_string() == current.format("%Y").to_string() {
        dt.format("%b %-d").to_string() // e.g. "Sep 10"
    } else {
        dt.format("%b %-d, %Y").to_string() // e.g. "Sep 10, 2026"
    };

    (label, date_key)
}

// =========================================================================
// Data Transfer Objects (DTOs)
// =========================================================================

/// A reconstructed work sequence milestone entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkSequenceEntry {
    /// Unique identifier for this entry
    pub id: String,
    /// Context ID if assigned
    pub context_id: Option<String>,
    /// Context name (e.g., "University OS", "Aurora Website", "Tokyo Trip")
    pub context_name: String,
    /// Formatted time window (e.g. "09:00–10:20")
    pub time_window: String,
    /// ISO-8601 start timestamp
    pub started_at: String,
    /// ISO-8601 end timestamp
    pub ended_at: String,
    /// Active duration in seconds
    pub duration_seconds: i64,
    /// Non-intrusive estimate: e.g. "Active for about 40 minutes"
    pub duration_estimate: String,
    /// Inferred or confirmed task title (e.g. "Process scheduling assignment", "Hero animation")
    pub task_title: Option<String>,
    /// Qualitative task confidence band ("High", "Medium", "Low", "Insufficient")
    pub task_confidence_band: Option<String>,
    /// Epistemic source of the task (Inferred, Confirmed, Observed)
    pub task_provenance: InformationProvenance,
    /// Transition type that initiated this segment (context_start, context_switch, context_return, context_end)
    pub transition_type: Option<String>,
    /// Whether this milestone was a return to a previously active context
    pub is_context_return: bool,
    /// Number of items or evidence captured in this segment
    pub item_count: usize,
    /// Lightweight references to recent items
    pub recent_items: Vec<MemoryItemReference>,
}

/// Day-level grouping of work sequence entries (e.g. "Today", "Yesterday").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkSequenceDayGroup {
    /// Human-friendly label (e.g. "Today", "Yesterday", "Sep 10")
    pub date_label: String,
    /// ISO date string (YYYY-MM-DD)
    pub date_key: String,
    /// Chronological list of work sequence entries
    pub entries: Vec<WorkSequenceEntry>,
}

/// Lightweight reference to a recently active context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentContextUsageRef {
    pub context_id: String,
    pub context_name: String,
    pub session_count: usize,
    pub total_duration_seconds: i64,
    pub duration_estimate: String,
    pub last_active_at: String,
}

/// Lightweight reference to a recently active task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentTaskUsageRef {
    pub title: String,
    pub context_id: Option<String>,
    pub context_name: String,
    pub confidence_band: String,
    pub provenance: InformationProvenance,
    pub last_active_at: String,
}

/// High-level response containing the entire cross-context recent work sequence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentWorkSequenceResponse {
    /// Day-grouped chronological work sequence
    pub days: Vec<WorkSequenceDayGroup>,
    /// Flat list of all entries ordered chronologically
    pub all_entries: Vec<WorkSequenceEntry>,
    /// Unique list of recent contexts used
    pub recent_contexts: Vec<RecentContextUsageRef>,
    /// Unique list of recent tasks active across contexts
    pub recent_tasks: Vec<RecentTaskUsageRef>,
    /// Total count of context switches
    pub context_switches: usize,
    /// Total count of context returns
    pub context_returns: usize,
    /// The single most recently active context
    pub last_active_context: Option<RecentContextUsageRef>,
    /// The single most recently active task
    pub last_active_task: Option<RecentTaskUsageRef>,
    /// High-level summary statement
    pub sequence_statement: String,
}

// =========================================================================
// Recent Work Sequence Service Implementation
// =========================================================================

/// Service that reconstructs the user's recent cross-context work sequence
/// from ContextTransition, ContextActivitySegment, and TaskCandidate data.
#[derive(Debug, Default)]
pub struct RecentWorkSequenceService {
    segmenter: ContextActivitySegmenter,
}

impl RecentWorkSequenceService {
    pub fn new() -> Self {
        Self {
            segmenter: ContextActivitySegmenter::new(),
        }
    }

    /// Reconstructs the complete cross-context work sequence from activity segments and database.
    pub fn reconstruct_from_segments(
        &self,
        conn: &Connection,
        segments: &[ContextActivitySegment],
        limit_days: Option<usize>,
        reference_now: Option<DateTime<Utc>>,
    ) -> RecentWorkSequenceResponse {
        let max_days = limit_days.unwrap_or(7);
        let now = reference_now.unwrap_or_else(Utc::now);

        let mut all_entries: Vec<WorkSequenceEntry> = Vec::new();
        let mut context_usage_map: HashMap<String, (String, usize, i64, String)> = HashMap::new(); // id -> (name, count, duration, last_active)
        let mut tasks_map: HashMap<String, RecentTaskUsageRef> = HashMap::new();
        let mut context_switches = 0;
        let mut context_returns = 0;

        for seg in segments {
            // Ignore ambiguous noise segments with 0 or 1 noisy item without context
            if seg.is_ambiguous && seg.context.is_none() && seg.item_count <= 1 {
                continue;
            }

            let context_id = seg.context.as_ref().map(|c| c.id.clone());
            let context_name = seg
                .context
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Unassigned Activity".to_string());

            let is_return = if let Some(ref t_in) = seg.transition_in {
                match t_in.transition_type {
                    ContextTransitionType::ContextReturn => {
                        context_returns += 1;
                        true
                    }
                    ContextTransitionType::ContextSwitch => {
                        context_switches += 1;
                        false
                    }
                    _ => false,
                }
            } else {
                false
            };

            let transition_type_str = seg.transition_in.as_ref().map(|t| {
                match t.transition_type {
                    ContextTransitionType::ContextStart => "context_start".to_string(),
                    ContextTransitionType::ContextSwitch => "context_switch".to_string(),
                    ContextTransitionType::ContextReturn => "context_return".to_string(),
                    ContextTransitionType::ContextEnd => "context_end".to_string(),
                    ContextTransitionType::AmbiguousTransition => "ambiguous".to_string(),
                }
            });

            // Reconstruct task candidate for this segment
            let (task_title, task_confidence_band, task_provenance) = self.infer_segment_task(conn, seg);

            // Build lightweight item references
            let recent_items: Vec<MemoryItemReference> = seg
                .item_references
                .iter()
                .map(|i| MemoryItemReference {
                    id: i.item_id.clone(),
                    title: i.title.clone(),
                    item_type: i.item_type.clone(),
                    timestamp: i.timestamp.clone(),
                    provenance: InformationProvenance::Observed,
                    app_name: None,
                    domain: None,
                })
                .collect();

            let duration_estimate = format_activity_duration_estimate(seg.duration_seconds);
            let time_window = format_time_window(&seg.started_at, &seg.ended_at);

            let entry = WorkSequenceEntry {
                id: seg.id.clone(),
                context_id: context_id.clone(),
                context_name: context_name.clone(),
                time_window,
                started_at: seg.started_at.clone(),
                ended_at: seg.ended_at.clone(),
                duration_seconds: seg.duration_seconds,
                duration_estimate,
                task_title: task_title.clone(),
                task_confidence_band: task_confidence_band.clone(),
                task_provenance,
                transition_type: transition_type_str,
                is_context_return: is_return,
                item_count: seg.item_count,
                recent_items,
            };

            // Track context usage
            if let Some(ref cid) = context_id {
                let entry_data = context_usage_map.entry(cid.clone()).or_insert_with(|| {
                    (context_name.clone(), 0, 0, seg.ended_at.clone())
                });
                entry_data.1 += 1;
                entry_data.2 += seg.duration_seconds;
                if seg.ended_at > entry_data.3 {
                    entry_data.3 = seg.ended_at.clone();
                }
            }

            // Track recent tasks
            if let Some(ref title) = task_title {
                let key = format!("{}:{}", context_id.as_deref().unwrap_or(""), title);
                tasks_map.entry(key).or_insert_with(|| RecentTaskUsageRef {
                    title: title.clone(),
                    context_id: context_id.clone(),
                    context_name: context_name.clone(),
                    confidence_band: task_confidence_band.unwrap_or_else(|| "Medium".to_string()),
                    provenance: task_provenance,
                    last_active_at: seg.ended_at.clone(),
                });
            }

            all_entries.push(entry);
        }

        // Group entries by day
        let mut day_groups_map: BTreeMap<String, (String, Vec<WorkSequenceEntry>)> = BTreeMap::new();

        for entry in &all_entries {
            let (label, date_key) = format_date_label(&entry.started_at, Some(now));
            let group = day_groups_map.entry(date_key.clone()).or_insert_with(|| (label, Vec::new()));
            group.1.push(entry.clone());
        }

        // Convert into sorted list of Day Groups (newest days first)
        let mut days: Vec<WorkSequenceDayGroup> = day_groups_map
            .into_iter()
            .rev()
            .take(max_days)
            .map(|(date_key, (date_label, entries))| WorkSequenceDayGroup {
                date_label,
                date_key,
                entries,
            })
            .collect();

        // Sort entries within each day in chronological order
        for day in &mut days {
            day.entries.sort_by(|a, b| a.started_at.cmp(&b.started_at));
        }

        // Prepare Recent Contexts list
        let mut recent_contexts: Vec<RecentContextUsageRef> = context_usage_map
            .into_iter()
            .map(|(id, (name, count, duration, last_active))| RecentContextUsageRef {
                context_id: id,
                context_name: name,
                session_count: count,
                total_duration_seconds: duration,
                duration_estimate: format_activity_duration_estimate(duration),
                last_active_at: last_active,
            })
            .collect();
        recent_contexts.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

        // Prepare Recent Tasks list
        let mut recent_tasks: Vec<RecentTaskUsageRef> = tasks_map.into_values().collect();
        recent_tasks.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));

        let last_active_context = recent_contexts.first().cloned();
        let last_active_task = recent_tasks.first().cloned();

        // Format high-level summary statement
        let sequence_statement = if !recent_contexts.is_empty() {
            let names: Vec<&str> = recent_contexts.iter().take(3).map(|c| c.context_name.as_str()).collect();
            if names.len() == 1 {
                format!("Recent work focused on {}.", names[0])
            } else if names.len() == 2 {
                format!("Recent work across {} and {}.", names[0], names[1])
            } else {
                format!("Recent work across {}, {}, and {}.", names[0], names[1], names[2])
            }
        } else {
            "No recent contextual work recorded.".to_string()
        };

        RecentWorkSequenceResponse {
            days,
            all_entries,
            recent_contexts,
            recent_tasks,
            context_switches,
            context_returns,
            last_active_context,
            last_active_task,
            sequence_statement,
        }
    }

    /// Queries the full work sequence directly from the database.
    pub fn get_recent_work_sequence(
        &self,
        conn: &Connection,
        limit_days: Option<usize>,
    ) -> Result<RecentWorkSequenceResponse, String> {
        let mut segmenter = self.segmenter.clone();
        let segments = segmenter.build_all_segments(conn)?;
        Ok(self.reconstruct_from_segments(conn, &segments, limit_days, None))
    }

    /// Helper to infer a candidate task for a specific activity segment.
    fn infer_segment_task(
        &self,
        conn: &Connection,
        seg: &ContextActivitySegment,
    ) -> (Option<String>, Option<String>, InformationProvenance) {
        if let Some(ref ctx) = seg.context {
            // First check if a task candidate can be extracted from the segment's items
            if !seg.item_references.is_empty() {
                // Collect evidence specifically for this context
                if let Ok(evidence_set) = crate::context_engine::task_evidence::TaskEvidenceCollector::new()
                    .collect_for_context(conn, &ctx.id)
                {
                    let candidates = crate::context_engine::task_extractor::TaskCandidateExtractor::extract_from_evidence(
                        &evidence_set,
                        None,
                    );
                    if !candidates.is_empty() {
                        let ranked = crate::context_engine::task_ranking::TaskCandidateRanker::rank_candidates(
                            &candidates,
                            Some(0.90),
                            None,
                        );
                        if let Some(top) = ranked.top_candidate.as_ref() {
                            if top.confidence >= 0.40 && !is_generic_task_name(&top.candidate.title) {
                                let band = if top.confidence >= 0.70 {
                                    "High"
                                } else if top.confidence >= 0.50 {
                                    "Medium"
                                } else {
                                    "Low"
                                };
                                return (
                                    Some(top.candidate.title.clone()),
                                    Some(band.to_string()),
                                    InformationProvenance::Inferred,
                                );
                            }
                        }
                    }
                }
            }

            // Check if item references have a common salient subject or title snippet
            for item in &seg.item_references {
                if let Some(ref title) = item.title {
                    let clean = title.trim();
                    if !clean.is_empty() && !is_generic_task_name(clean) && clean.len() >= 4 {
                        return (
                            Some(clean.to_string()),
                            Some("Medium".to_string()),
                            InformationProvenance::Observed,
                        );
                    }
                }
            }
        }

        (None, None, InformationProvenance::Observed)
    }
}

// =========================================================================
// Unit & Integration Tests (Using Day 15 Transition Dataset)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::segments::SegmentItemReference;
    use crate::context_engine::transitions::{ContextReference, ContextTransitionType, DetectedTransition};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    fn make_test_segment(
        id: &str,
        context_name: Option<&str>,
        context_id: Option<&str>,
        start: &str,
        end: &str,
        duration: i64,
        trans_type: Option<ContextTransitionType>,
        items_count: usize,
    ) -> ContextActivitySegment {
        let context = context_name.map(|name| ContextReference {
            id: context_id.unwrap_or(name).to_string(),
            name: name.to_string(),
            is_candidate: false,
        });

        let transition_in = trans_type.map(|tt| DetectedTransition {
            id: format!("trans_{}", id),
            transition_type: tt,
            from_context: None,
            to_context: context.clone(),
            confidence: 0.85,
            timestamp: start.to_string(),
            evidence: vec!["test transition".to_string()],
            session_id: None,
            trigger_item_id: None,
            duration_since_previous_activity_seconds: None,
            metadata: serde_json::json!({}),
        });

        let mut items = Vec::new();
        for i in 0..items_count {
            items.push(SegmentItemReference {
                item_id: format!("{}_item_{}", id, i),
                title: Some(format!("{} Task Item {}", context_name.unwrap_or("Generic"), i)),
                item_type: "url".to_string(),
                timestamp: start.to_string(),
                confidence: 0.80,
            });
        }

        ContextActivitySegment {
            id: id.to_string(),
            context,
            started_at: start.to_string(),
            ended_at: end.to_string(),
            duration_seconds: duration,
            duration_formatted: format_activity_duration_estimate(duration),
            confidence: 0.85,
            session_ids: vec!["sess_1".to_string()],
            item_references: items,
            transition_in,
            transition_out: None,
            is_ambiguous: false,
            item_count: items_count,
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn test_format_activity_duration_estimate() {
        assert_eq!(format_activity_duration_estimate(20), "Active for less than a minute");
        assert_eq!(format_activity_duration_estimate(180), "Active for a few minutes");
        assert_eq!(format_activity_duration_estimate(2400), "Active for about 40 minutes");
        assert_eq!(format_activity_duration_estimate(3600), "Active for about 1 hour");
        assert_eq!(format_activity_duration_estimate(4500), "Active for about 1 hour 15 minutes");
        assert_eq!(format_activity_duration_estimate(7200), "Active for about 2 hours");

        // Verifies estimate strictly avoids "You spent X focused"
        let estimate = format_activity_duration_estimate(2400);
        assert!(!estimate.contains("spent"));
        assert!(!estimate.contains("focused"));
        assert!(estimate.starts_with("Active for"));
    }

    #[test]
    fn test_format_time_window() {
        let window = format_time_window("2026-09-10T09:00:00Z", "2026-09-10T10:20:00Z");
        assert_eq!(window, "09:00–10:20");
    }

    #[test]
    fn test_prompt_example_work_sequence_reconstruction() {
        let conn = setup_test_db();
        let service = RecentWorkSequenceService::new();

        // Model the prompt's exact sequence:
        // University OS (09:00–10:20)
        // Aurora Website (10:30–11:45)
        // University OS (12:00–13:10) [Return]
        // Tokyo Trip (14:00–14:40)
        // Aurora Website (16:00–17:20) [Return]
        let segments = vec![
            make_test_segment("s1", Some("University OS"), Some("ws_univ"), "2026-09-10T09:00:00Z", "2026-09-10T10:20:00Z", 4800, Some(ContextTransitionType::ContextStart), 3),
            make_test_segment("s2", Some("Aurora Website"), Some("ws_aurora"), "2026-09-10T10:30:00Z", "2026-09-10T11:45:00Z", 4500, Some(ContextTransitionType::ContextSwitch), 4),
            make_test_segment("s3", Some("University OS"), Some("ws_univ"), "2026-09-10T12:00:00Z", "2026-09-10T13:10:00Z", 4200, Some(ContextTransitionType::ContextReturn), 3),
            make_test_segment("s4", Some("Tokyo Trip"), Some("ws_tokyo"), "2026-09-10T14:00:00Z", "2026-09-10T14:40:00Z", 2400, Some(ContextTransitionType::ContextSwitch), 2),
            make_test_segment("s5", Some("Aurora Website"), Some("ws_aurora"), "2026-09-10T16:00:00Z", "2026-09-10T17:20:00Z", 4800, Some(ContextTransitionType::ContextReturn), 4),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-10T18:00:00Z").unwrap().with_timezone(&Utc);
        let res = service.reconstruct_from_segments(&conn, &segments, Some(7), Some(ref_now));

        assert_eq!(res.all_entries.len(), 5);
        assert_eq!(res.days.len(), 1);
        assert_eq!(res.days[0].date_label, "Today");
        assert_eq!(res.days[0].entries.len(), 5);

        // Verify sequence order & time windows
        assert_eq!(res.days[0].entries[0].context_name, "University OS");
        assert_eq!(res.days[0].entries[0].time_window, "09:00–10:20");
        assert_eq!(res.days[0].entries[0].duration_estimate, "Active for about 1 hour 20 minutes");

        assert_eq!(res.days[0].entries[1].context_name, "Aurora Website");
        assert_eq!(res.days[0].entries[1].time_window, "10:30–11:45");

        assert_eq!(res.days[0].entries[2].context_name, "University OS");
        assert_eq!(res.days[0].entries[2].time_window, "12:00–13:10");
        assert!(res.days[0].entries[2].is_context_return);

        assert_eq!(res.days[0].entries[3].context_name, "Tokyo Trip");
        assert_eq!(res.days[0].entries[3].time_window, "14:00–14:40");
        assert_eq!(res.days[0].entries[3].duration_estimate, "Active for about 40 minutes");

        assert_eq!(res.days[0].entries[4].context_name, "Aurora Website");
        assert_eq!(res.days[0].entries[4].time_window, "16:00–17:20");
        assert!(res.days[0].entries[4].is_context_return);

        // Verify context switches and returns
        assert_eq!(res.context_switches, 2);
        assert_eq!(res.context_returns, 2);

        // Verify recent contexts (3 distinct contexts)
        assert_eq!(res.recent_contexts.len(), 3);
        assert_eq!(res.recent_contexts[0].context_name, "Aurora Website");
        assert_eq!(res.last_active_context.as_ref().unwrap().context_name, "Aurora Website");
    }

    #[test]
    fn test_day15_transition_dataset_work_sequence() {
        let conn = setup_test_db();
        let service = RecentWorkSequenceService::new();

        // Model the Day 15 transition dataset from workflow_tests.rs:
        // College (09:00–09:20) -> Portfolio (09:25–09:45) -> College Return (10:15–10:20) -> Portfolio Return (11:00–11:05)
        let segments = vec![
            make_test_segment("c_seg1", Some("College"), Some("ws_college"), "2026-09-10T09:00:00Z", "2026-09-10T09:20:00Z", 1200, Some(ContextTransitionType::ContextStart), 5),
            make_test_segment("p_seg1", Some("Portfolio"), Some("ws_portfolio"), "2026-09-10T09:25:00Z", "2026-09-10T09:45:00Z", 1200, Some(ContextTransitionType::ContextSwitch), 3),
            make_test_segment("c_seg2", Some("College"), Some("ws_college"), "2026-09-10T10:15:00Z", "2026-09-10T10:20:00Z", 300, Some(ContextTransitionType::ContextReturn), 2),
            make_test_segment("p_seg2", Some("Portfolio"), Some("ws_portfolio"), "2026-09-10T11:00:00Z", "2026-09-10T11:05:00Z", 300, Some(ContextTransitionType::ContextReturn), 2),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-10T12:00:00Z").unwrap().with_timezone(&Utc);
        let res = service.reconstruct_from_segments(&conn, &segments, Some(7), Some(ref_now));

        assert_eq!(res.all_entries.len(), 4);
        assert_eq!(res.context_switches, 1);
        assert_eq!(res.context_returns, 2);

        assert_eq!(res.all_entries[0].context_name, "College");
        assert_eq!(res.all_entries[0].time_window, "09:00–09:20");
        assert_eq!(res.all_entries[0].duration_estimate, "Active for about 20 minutes");

        assert_eq!(res.all_entries[1].context_name, "Portfolio");
        assert_eq!(res.all_entries[1].time_window, "09:25–09:45");
        assert_eq!(res.all_entries[1].duration_estimate, "Active for about 20 minutes");

        assert_eq!(res.all_entries[2].context_name, "College");
        assert!(res.all_entries[2].is_context_return);
        assert_eq!(res.all_entries[2].time_window, "10:15–10:20");

        assert_eq!(res.all_entries[3].context_name, "Portfolio");
        assert!(res.all_entries[3].is_context_return);
        assert_eq!(res.all_entries[3].time_window, "11:00–11:05");

        assert_eq!(res.last_active_context.as_ref().unwrap().context_name, "Portfolio");
    }

    #[test]
    fn test_multi_day_grouping_and_labeling() {
        let conn = setup_test_db();
        let service = RecentWorkSequenceService::new();

        let segments = vec![
            make_test_segment("d1", Some("Portfolio"), Some("ws_port"), "2026-09-08T10:00:00Z", "2026-09-08T11:00:00Z", 3600, Some(ContextTransitionType::ContextStart), 2),
            make_test_segment("d2", Some("College"), Some("ws_col"), "2026-09-09T14:00:00Z", "2026-09-09T15:00:00Z", 3600, Some(ContextTransitionType::ContextStart), 3),
            make_test_segment("d3", Some("TripWise"), Some("ws_trip"), "2026-09-10T09:00:00Z", "2026-09-10T10:00:00Z", 3600, Some(ContextTransitionType::ContextStart), 4),
        ];

        let ref_now = DateTime::parse_from_rfc3339("2026-09-10T18:00:00Z").unwrap().with_timezone(&Utc);
        let res = service.reconstruct_from_segments(&conn, &segments, Some(7), Some(ref_now));

        assert_eq!(res.days.len(), 3);
        assert_eq!(res.days[0].date_label, "Today");
        assert_eq!(res.days[0].entries[0].context_name, "TripWise");

        assert_eq!(res.days[1].date_label, "Yesterday");
        assert_eq!(res.days[1].entries[0].context_name, "College");

        assert_eq!(res.days[2].date_label, "Tuesday");
        assert_eq!(res.days[2].entries[0].context_name, "Portfolio");
    }
}
