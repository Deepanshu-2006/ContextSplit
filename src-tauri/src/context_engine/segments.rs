use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context_engine::transitions::{
    format_duration_human, ContextReference, ContextTransitionDetector, ContextTransitionType,
    DetectedTransition, TransitionEvaluationResult,
};

/// Summary of an item within a contextual activity segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentItemReference {
    pub item_id: String,
    pub title: Option<String>,
    pub item_type: String,
    pub timestamp: String,
    pub confidence: f64,
}

/// Represents a continuous period of contextual activity.
///
/// Invariant: Segments are derived dynamically from existing activity, sessions,
/// and detected transitions without creating duplicate items or sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextActivitySegment {
    pub id: String,
    pub context: Option<ContextReference>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub duration_formatted: String,
    pub confidence: f64,
    pub session_ids: Vec<String>,
    pub item_references: Vec<SegmentItemReference>,
    pub transition_in: Option<DetectedTransition>,
    pub transition_out: Option<DetectedTransition>,
    pub is_ambiguous: bool,
    pub item_count: usize,
    pub metadata: Value,
}

/// Input event for stream-based contextual segment construction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentStreamEvent {
    pub item_id: String,
    pub session_id: Option<String>,
    pub timestamp: String,
    pub target_context: Option<ContextReference>,
    pub confidence: f64,
    pub confidence_band: String,
    pub is_ambiguous: bool,
    pub item_type: String,
    pub item_title: Option<String>,
    pub evidence: Vec<String>,
}

/// Active working state while accumulating a continuous activity segment.
#[derive(Debug, Clone)]
struct ActiveSegmentState {
    id: String,
    context: Option<ContextReference>,
    started_at: String,
    last_event_at: String,
    confidence_sum: f64,
    session_ids: Vec<String>,
    items: Vec<SegmentItemReference>,
    transition_in: Option<DetectedTransition>,
    is_ambiguous: bool,
}

pub(crate) fn parse_timestamp_utc(ts: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return dt.with_timezone(&Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ") {
        return DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
    }
    Utc::now()
}

impl ActiveSegmentState {
    fn new(
        context: Option<ContextReference>,
        started_at: String,
        confidence: f64,
        session_id: Option<String>,
        transition_in: Option<DetectedTransition>,
        is_ambiguous: bool,
    ) -> Self {
        let mut session_ids = Vec::new();
        if let Some(s) = session_id {
            session_ids.push(s);
        }

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            context,
            started_at: started_at.clone(),
            last_event_at: started_at,
            confidence_sum: confidence,
            session_ids,
            items: Vec::new(),
            transition_in,
            is_ambiguous,
        }
    }

    fn add_item(&mut self, item: SegmentItemReference, session_id: Option<&str>) {
        if let Some(s) = session_id {
            if !self.session_ids.iter().any(|existing| existing == s) {
                self.session_ids.push(s.to_string());
            }
        }
        self.last_event_at = item.timestamp.clone();
        self.items.push(item);
    }

    fn seal(self, transition_out: Option<DetectedTransition>) -> ContextActivitySegment {
        let start_dt = parse_timestamp_utc(&self.started_at);
        let end_dt = parse_timestamp_utc(&self.last_event_at);

        let duration_seconds = end_dt.signed_duration_since(start_dt).num_seconds().max(0);
        let duration_formatted = format_duration_human(duration_seconds);

        let item_count = self.items.len();
        let avg_confidence = if item_count > 0 {
            (self.confidence_sum / item_count as f64).clamp(0.0, 1.0)
        } else {
            self.confidence_sum.clamp(0.0, 1.0)
        };

        ContextActivitySegment {
            id: self.id,
            context: self.context,
            started_at: self.started_at,
            ended_at: self.last_event_at,
            duration_seconds,
            duration_formatted,
            confidence: avg_confidence,
            session_ids: self.session_ids,
            item_references: self.items,
            transition_in: self.transition_in,
            transition_out,
            is_ambiguous: self.is_ambiguous,
            item_count,
            metadata: serde_json::json!({
                "duration_seconds": duration_seconds,
            }),
        }
    }
}

/// Service for deriving continuous contextual activity segments.
///
/// Features:
/// - Aggregates continuous items into unified segment records.
/// - Excludes inactivity gaps from active work durations.
/// - Preserves full transition boundaries (`transition_in`, `transition_out`).
/// - Distinguishes switches, starts, ends, returns, and ambiguous periods.
#[derive(Debug, Clone)]
pub struct ContextActivitySegmenter {
    pub inactivity_threshold_secs: i64,
    pub detector: ContextTransitionDetector,
}

impl ContextActivitySegmenter {
    pub const DEFAULT_INACTIVITY_THRESHOLD_SECS: i64 = 1800; // 30 minutes

    pub fn new() -> Self {
        Self {
            inactivity_threshold_secs: Self::DEFAULT_INACTIVITY_THRESHOLD_SECS,
            detector: ContextTransitionDetector::new(),
        }
    }
}

impl Default for ContextActivitySegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextActivitySegmenter {

    pub fn with_inactivity_threshold(mut self, secs: i64) -> Self {
        self.inactivity_threshold_secs = secs.max(1);
        self.detector = self.detector.with_inactivity_threshold(secs);
        self
    }

    pub fn with_detector(mut self, detector: ContextTransitionDetector) -> Self {
        self.detector = detector;
        self.inactivity_threshold_secs = self.detector.inactivity_threshold_secs;
        self
    }

    /// Derives contextual activity segments from a stream of in-memory events.
    pub fn segment_event_stream(
        &mut self,
        events: &[SegmentStreamEvent],
    ) -> Vec<ContextActivitySegment> {
        let mut segments: Vec<ContextActivitySegment> = Vec::new();
        let mut active_state: Option<ActiveSegmentState> = None;

        for event in events {
            let event_time_dt = parse_timestamp_utc(&event.timestamp);

            // 1. Check if inactivity threshold elapsed since last event
            if let Some(ref act) = active_state {
                let last_dt = parse_timestamp_utc(&act.last_event_at);
                let elapsed = event_time_dt.signed_duration_since(last_dt).num_seconds();
                if elapsed >= self.inactivity_threshold_secs {
                    // Inactivity gap! Seal previous segment at its last active timestamp
                    let end_transition = self.detector.check_inactivity(&event.timestamp);
                    let sealed = active_state.take().unwrap().seal(end_transition);
                    segments.push(sealed);
                }
            }

            // 2. Evaluate item against contextual state
            let eval_result = self.detector.evaluate_interpretation(
                Some(&event.item_id),
                event.session_id.as_deref(),
                &event.timestamp,
                event.target_context.clone(),
                event.confidence,
                &event.confidence_band,
                event.is_ambiguous,
                event.evidence.clone(),
            );

            let item_ref = SegmentItemReference {
                item_id: event.item_id.clone(),
                title: event.item_title.clone(),
                item_type: event.item_type.clone(),
                timestamp: event.timestamp.clone(),
                confidence: event.confidence,
            };

            match eval_result {
                TransitionEvaluationResult::Transition(transition) => {
                    // Seal existing segment if active
                    if let Some(act) = active_state.take() {
                        let sealed = act.seal(Some(transition.clone()));
                        segments.push(sealed);
                    }

                    // Open new segment for the transitioned context
                    let is_ambiguous = transition.transition_type
                        == ContextTransitionType::AmbiguousTransition
                        || event.is_ambiguous;
                    let target_ctx = if is_ambiguous {
                        None
                    } else {
                        transition.to_context.clone().or(event.target_context.clone())
                    };

                    let mut new_act = ActiveSegmentState::new(
                        target_ctx,
                        event.timestamp.clone(),
                        event.confidence,
                        event.session_id.clone(),
                        Some(transition),
                        is_ambiguous,
                    );
                    new_act.add_item(item_ref, event.session_id.as_deref());
                    active_state = Some(new_act);
                }
                TransitionEvaluationResult::Continuation {
                    confidence,
                    ..
                } => {
                    // Continuous activity in existing segment
                    if let Some(mut act) = active_state.take() {
                        if act.is_ambiguous && !event.is_ambiguous {
                            // Conclude previous ambiguous period and start fresh active segment
                            segments.push(act.seal(None));
                            let mut new_act = ActiveSegmentState::new(
                                event.target_context.clone(),
                                event.timestamp.clone(),
                                event.confidence,
                                event.session_id.clone(),
                                None,
                                false,
                            );
                            new_act.add_item(item_ref, event.session_id.as_deref());
                            active_state = Some(new_act);
                        } else {
                            act.confidence_sum += confidence;
                            act.add_item(item_ref, event.session_id.as_deref());
                            active_state = Some(act);
                        }
                    } else {
                        // Fallback if no active state
                        let mut new_act = ActiveSegmentState::new(
                            event.target_context.clone(),
                            event.timestamp.clone(),
                            event.confidence,
                            event.session_id.clone(),
                            None,
                            event.is_ambiguous,
                        );
                        new_act.add_item(item_ref, event.session_id.as_deref());
                        active_state = Some(new_act);
                    }
                }
                TransitionEvaluationResult::Pending { .. }
                | TransitionEvaluationResult::NoTransition { .. } => {
                    // Ambient noise or pending candidate: if active segment exists, record item
                    if let Some(ref mut act) = active_state {
                        act.add_item(item_ref, event.session_id.as_deref());
                    }
                }
            }
        }

        // 3. Seal the final active segment
        if let Some(act) = active_state.take() {
            segments.push(act.seal(None));
        }

        segments
    }

    /// Derives contextual activity segments for a list of Item IDs from SQLite database.
    ///
    /// Invariant: Does NOT modify Item or Workspace memberships.
    pub fn build_segments_from_items(
        &mut self,
        conn: &Connection,
        item_ids: &[String],
    ) -> Result<Vec<ContextActivitySegment>, String> {
        let mut events = Vec::new();

        for id in item_ids {
            let item = crate::db::operations::get_item(conn, id)
                .map_err(|e| format!("Item '{}' not found: {}", id, e))?;

            let confirmed_ws = if let Some(ws_id) = &item.workspace_id {
                if ws_id != "inbox" && !ws_id.is_empty() {
                    crate::db::operations::get_workspace(conn, ws_id).ok()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(ws) = confirmed_ws {
                events.push(SegmentStreamEvent {
                    item_id: item.id.clone(),
                    session_id: item.session_id.clone(),
                    timestamp: item.created_at.clone(),
                    target_context: Some(ContextReference::workspace(&ws.id, &ws.name)),
                    confidence: 1.0,
                    confidence_band: "Very High".to_string(),
                    is_ambiguous: false,
                    item_type: item.item_type.clone(),
                    item_title: item.title.clone(),
                    evidence: vec![format!("User confirmed in workspace '{}'", ws.name)],
                });
            } else {
                let interp = crate::context_engine::scoring::evaluate_item_context(conn, id)?;
                let is_amb = interp.interpretation_kind
                    == Some(crate::db::models::ContextInterpretationKind::Ambiguous);

                let (target_ref, conf, band, evidence) = if let Some(ws) =
                    interp.existing_workspaces.first()
                {
                    (
                        Some(ContextReference::workspace(&ws.workspace_id, &ws.workspace_name)),
                        ws.confidence,
                        ws.confidence_band.clone(),
                        ws.reasons.clone(),
                    )
                } else if let Some(cand) = interp.potential_new_contexts.first() {
                    (
                        Some(ContextReference::candidate(
                            cand.candidate_id.as_deref().unwrap_or("cand"),
                            &cand.label,
                        )),
                        cand.confidence,
                        cand.confidence_band.clone(),
                        cand.reasons.clone(),
                    )
                } else {
                    (None, 0.0, "Very Low".to_string(), vec![interp.summary.clone()])
                };

                events.push(SegmentStreamEvent {
                    item_id: item.id.clone(),
                    session_id: item.session_id.clone(),
                    timestamp: item.created_at.clone(),
                    target_context: target_ref,
                    confidence: conf,
                    confidence_band: band,
                    is_ambiguous: is_amb,
                    item_type: item.item_type.clone(),
                    item_title: item.title.clone(),
                    evidence,
                });
            }
        }

        // Sort events chronologically
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(self.segment_event_stream(&events))
    }

    /// Derives contextual activity segments for an entire session.
    pub fn build_segments_for_session(
        &mut self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Vec<ContextActivitySegment>, String> {
        let items = crate::db::operations::list_items_for_session(conn, session_id)
            .map_err(|e| e.to_string())?;
        let item_ids: Vec<String> = items.into_iter().map(|i| i.id).collect();
        self.build_segments_from_items(conn, &item_ids)
    }

    /// Derives contextual activity segments for a specific workspace.
    pub fn build_segments_for_workspace(
        &mut self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<Vec<ContextActivitySegment>, String> {
        self.build_segments_for_workspace_bounded(conn, workspace_id, None)
    }

    /// Derives contextual activity segments for a specific workspace bounded by a recent item limit.
    pub fn build_segments_for_workspace_bounded(
        &mut self,
        conn: &Connection,
        workspace_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ContextActivitySegment>, String> {
        let ws_opt = crate::db::operations::get_workspace(conn, workspace_id).ok();
        let ws_name = ws_opt.as_ref().map(|w| w.name.clone()).unwrap_or_else(|| workspace_id.to_string());
        let limit_val = limit.unwrap_or(50).clamp(1, 200) as i64;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, created_at, item_type, title 
                 FROM items 
                 WHERE workspace_id = ?1 
                 ORDER BY created_at DESC 
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![workspace_id, limit_val], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut events = Vec::new();
        for r in rows.flatten() {
            let (id, session_id, created_at, item_type, title) = r;
            events.push(SegmentStreamEvent {
                item_id: id,
                session_id,
                timestamp: created_at,
                target_context: Some(ContextReference::workspace(workspace_id, &ws_name)),
                confidence: 1.0,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type,
                item_title: title,
                evidence: vec![format!("User confirmed in workspace '{}'", ws_name)],
            });
        }

        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(self.segment_event_stream(&events))
    }

    /// Derives contextual activity segments within a specific date/time window.
    pub fn build_segments_for_date_range(
        &mut self,
        conn: &Connection,
        start_iso: &str,
        end_iso: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ContextActivitySegment>, String> {
        let limit_val = limit.unwrap_or(300).clamp(1, 1000) as i64;
        let start_t = start_iso.replace(' ', "T");
        let end_t = end_iso.replace(' ', "T");
        let mut stmt = conn
            .prepare(
                "SELECT id FROM items 
                 WHERE (created_at >= ?1 AND created_at <= ?2)
                    OR (created_at >= ?3 AND created_at <= ?4)
                 ORDER BY created_at ASC 
                 LIMIT ?5",
            )
            .map_err(|e| e.to_string())?;
        let item_ids = stmt
            .query_map(rusqlite::params![start_iso, end_iso, start_t, end_t, limit_val], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        self.build_segments_from_items(conn, &item_ids)
    }

    /// Derives all contextual activity segments across all recorded items.
    pub fn build_all_segments(
        &mut self,
        conn: &Connection,
    ) -> Result<Vec<ContextActivitySegment>, String> {
        let mut stmt = conn
            .prepare("SELECT id FROM items ORDER BY created_at ASC")
            .map_err(|e| e.to_string())?;
        let item_ids = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        self.build_segments_from_items(conn, &item_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_continuous_activity_single_segment() {
        let mut segmenter = ContextActivitySegmenter::new();
        let ctx = ContextReference::workspace("ws_1", "Rust Core");

        let events: Vec<SegmentStreamEvent> = (1..=5)
            .map(|i| SegmentStreamEvent {
                item_id: format!("item_{}", i),
                session_id: Some("sess_1".to_string()),
                timestamp: format!("2026-09-10T10:{:02}:00Z", i * 2), // 10:02, 10:04, 10:06, 10:08, 10:10
                target_context: Some(ctx.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: Some(format!("Code edit {}", i)),
                evidence: vec!["VS Code".to_string()],
            })
            .collect();

        let segments = segmenter.segment_event_stream(&events);

        assert_eq!(segments.len(), 1, "5 continuous items in same workspace must produce exactly 1 segment");
        let seg = &segments[0];
        assert_eq!(seg.context.as_ref().unwrap().id, "ws_1");
        assert_eq!(seg.item_count, 5);
        assert_eq!(seg.started_at, "2026-09-10T10:02:00Z");
        assert_eq!(seg.ended_at, "2026-09-10T10:10:00Z");
        assert_eq!(seg.duration_seconds, 480); // 8 minutes
        assert_eq!(seg.duration_formatted, "8 minutes");
        assert_eq!(seg.session_ids, vec!["sess_1".to_string()]);
        assert_eq!(seg.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextStart);
        assert!(seg.transition_out.is_none(), "Active continuous segment has no exit transition");
    }

    #[test]
    fn test_context_switch_two_segments_with_transitions() {
        let mut segmenter = ContextActivitySegmenter::new();
        let ctx_a = ContextReference::workspace("ws_a", "Design System");
        let ctx_b = ContextReference::workspace("ws_b", "Backend API");

        let events = vec![
            SegmentStreamEvent {
                item_id: "item_a1".to_string(),
                session_id: Some("sess_1".to_string()),
                timestamp: "2026-09-10T10:00:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Figma tokens".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            SegmentStreamEvent {
                item_id: "item_a2".to_string(),
                session_id: Some("sess_1".to_string()),
                timestamp: "2026-09-10T10:05:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "url".to_string(),
                item_title: Some("Figma components".to_string()),
                evidence: vec!["Figma app".to_string()],
            },
            SegmentStreamEvent {
                item_id: "item_b1".to_string(),
                session_id: Some("sess_1".to_string()),
                timestamp: "2026-09-10T10:10:00Z".to_string(),
                target_context: Some(ctx_b.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: Some("Rust endpoints".to_string()),
                evidence: vec!["VS Code".to_string()],
            },
            SegmentStreamEvent {
                item_id: "item_b2".to_string(),
                session_id: Some("sess_1".to_string()),
                timestamp: "2026-09-10T10:15:00Z".to_string(),
                target_context: Some(ctx_b.clone()),
                confidence: 0.91,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: Some("SQL migrations".to_string()),
                evidence: vec!["VS Code".to_string()],
            },
        ];

        let segments = segmenter.segment_event_stream(&events);

        assert_eq!(segments.len(), 2, "Context switch must produce 2 discrete segments");

        // Segment 1 (Design System)
        let seg1 = &segments[0];
        assert_eq!(seg1.context.as_ref().unwrap().id, "ws_a");
        assert_eq!(seg1.started_at, "2026-09-10T10:00:00Z");
        assert_eq!(seg1.ended_at, "2026-09-10T10:05:00Z");
        assert_eq!(seg1.duration_seconds, 300);
        assert_eq!(seg1.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextStart);
        assert_eq!(seg1.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // Segment 2 (Backend API)
        let seg2 = &segments[1];
        assert_eq!(seg2.context.as_ref().unwrap().id, "ws_b");
        assert_eq!(seg2.started_at, "2026-09-10T10:10:00Z");
        assert_eq!(seg2.ended_at, "2026-09-10T10:15:00Z");
        assert_eq!(seg2.duration_seconds, 300);
        assert_eq!(seg2.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);
    }

    #[test]
    fn test_inactivity_exclusion_from_active_duration() {
        let mut segmenter = ContextActivitySegmenter::new().with_inactivity_threshold(1800); // 30 min
        let ctx = ContextReference::workspace("ws_1", "Sprint Focus");

        let events = vec![
            SegmentStreamEvent {
                item_id: "i1".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:00:00Z".to_string(),
                target_context: Some(ctx.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i2".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:15:00Z".to_string(),
                target_context: Some(ctx.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            // --- 45 MINUTE INACTIVITY GAP (10:15 to 11:00) ---
            SegmentStreamEvent {
                item_id: "i3".to_string(),
                session_id: Some("s2".to_string()),
                timestamp: "2026-09-10T11:00:00Z".to_string(),
                target_context: Some(ctx.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i4".to_string(),
                session_id: Some("s2".to_string()),
                timestamp: "2026-09-10T11:20:00Z".to_string(),
                target_context: Some(ctx.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
        ];

        let segments = segmenter.segment_event_stream(&events);

        assert_eq!(segments.len(), 2, "Inactivity gap must split activity into 2 separate segments");

        // Segment 1 (10:00 to 10:15 = 15 minutes, NOT 60 mins!)
        let seg1 = &segments[0];
        assert_eq!(seg1.started_at, "2026-09-10T10:00:00Z");
        assert_eq!(seg1.ended_at, "2026-09-10T10:15:00Z");
        assert_eq!(seg1.duration_seconds, 900); // exactly 15 mins
        assert_eq!(seg1.duration_formatted, "15 minutes");
        assert_eq!(seg1.transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextEnd);

        // Segment 2 (11:00 to 11:20 = 20 minutes)
        let seg2 = &segments[1];
        assert_eq!(seg2.started_at, "2026-09-10T11:00:00Z");
        assert_eq!(seg2.ended_at, "2026-09-10T11:20:00Z");
        assert_eq!(seg2.duration_seconds, 1200); // 20 mins
        assert_eq!(seg2.duration_formatted, "20 minutes");
        assert_eq!(seg2.transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextStart);
    }

    #[test]
    fn test_context_return_segments_a_b_a() {
        let mut segmenter = ContextActivitySegmenter::new();
        let ctx_a = ContextReference::workspace("ws_a", "Primary Coding");
        let ctx_b = ContextReference::workspace("ws_b", "Slack Chat");

        let events = vec![
            // 1. Initial Coding in A (10:00 - 10:10)
            SegmentStreamEvent {
                item_id: "i1".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:00:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i2".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:10:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            // 2. Detour to Slack B (10:12 - 10:16)
            SegmentStreamEvent {
                item_id: "i3".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:12:00Z".to_string(),
                target_context: Some(ctx_b.clone()),
                confidence: 0.88,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i4".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:16:00Z".to_string(),
                target_context: Some(ctx_b.clone()),
                confidence: 0.88,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            // 3. Return to Coding A (10:20 - 10:30)
            SegmentStreamEvent {
                item_id: "i5".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:20:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i6".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:30:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.92,
                confidence_band: "Very High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
        ];

        let segments = segmenter.segment_event_stream(&events);

        assert_eq!(segments.len(), 3, "A -> B -> A pattern must create 3 segments (A1, B, A2)");

        // Segment 1 (A1)
        assert_eq!(segments[0].context.as_ref().unwrap().id, "ws_a");
        assert_eq!(segments[0].transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // Segment 2 (B)
        assert_eq!(segments[1].context.as_ref().unwrap().id, "ws_b");
        assert_eq!(segments[1].transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(segments[1].transition_out.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);

        // Segment 3 (A2 - Return)
        assert_eq!(segments[2].context.as_ref().unwrap().id, "ws_a");
        assert_eq!(segments[2].transition_in.as_ref().unwrap().transition_type, ContextTransitionType::ContextReturn);
        let ret_details = segments[2].transition_in.as_ref().unwrap().as_context_return().unwrap();
        assert_eq!(ret_details.returned_context.id, "ws_a");
        assert_eq!(ret_details.previous_context.unwrap().id, "ws_b");
        assert_eq!(ret_details.duration_since_previous_activity_seconds, 600); // 10 minutes between 10:10 and 10:20
    }

    #[test]
    fn test_ambiguous_periods_isolate_into_ambiguous_segment() {
        let mut segmenter = ContextActivitySegmenter::new();
        let ctx_a = ContextReference::workspace("ws_a", "Design Tokens");

        let events = vec![
            SegmentStreamEvent {
                item_id: "i1".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:00:00Z".to_string(),
                target_context: Some(ctx_a.clone()),
                confidence: 0.90,
                confidence_band: "High".to_string(),
                is_ambiguous: false,
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec![],
            },
            SegmentStreamEvent {
                item_id: "i2".to_string(),
                session_id: Some("s1".to_string()),
                timestamp: "2026-09-10T10:05:00Z".to_string(),
                target_context: None,
                confidence: 0.40,
                confidence_band: "Medium".to_string(),
                is_ambiguous: true, // Ambiguous period
                item_type: "text".to_string(),
                item_title: None,
                evidence: vec!["Split affinity between Design and Marketing".to_string()],
            },
        ];

        let segments = segmenter.segment_event_stream(&events);

        assert_eq!(segments.len(), 2);
        assert!(!segments[0].is_ambiguous);
        assert!(segments[1].is_ambiguous, "Ambiguous period must be marked as ambiguous segment");
        assert_eq!(segments[1].transition_in.as_ref().unwrap().transition_type, ContextTransitionType::AmbiguousTransition);
    }

    #[test]
    fn test_database_integration_zero_duplication() {
        let conn = setup_test_db();

        let ws_a = crate::db::operations::create_workspace(&conn, "Design Tokens").unwrap();
        let ws_b = crate::db::operations::create_workspace(&conn, "Backend Code").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

        // Count initial items & sessions
        let init_items_count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let init_sessions_count = crate::db::operations::list_sessions(&conn).unwrap().len();

        let i1 = crate::db::operations::create_item(&conn, Some(&ws_a.id), Some(&session.id), "text", "Figma 1", None, "{}").unwrap();
        let i2 = crate::db::operations::create_item(&conn, Some(&ws_a.id), Some(&session.id), "text", "Figma 2", None, "{}").unwrap();
        let i3 = crate::db::operations::create_item(&conn, Some(&ws_b.id), Some(&session.id), "text", "Rust Code", None, "{}").unwrap();

        let mut segmenter = ContextActivitySegmenter::new();
        let segments = segmenter.build_segments_for_session(&conn, &session.id).unwrap();

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].context.as_ref().unwrap().id, ws_a.id);
        assert_eq!(segments[0].item_count, 2);
        assert_eq!(segments[1].context.as_ref().unwrap().id, ws_b.id);
        assert_eq!(segments[1].item_count, 1);

        // STRICT INVARIANT TEST: No duplicate items or sessions created during segmentation
        let final_items_count: i64 = conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        let final_sessions = crate::db::operations::list_sessions(&conn).unwrap();

        assert_eq!(final_items_count, init_items_count + 3);
        assert_eq!(final_sessions.len(), init_sessions_count);

        // Verify items were untouched
        let fresh_i1 = crate::db::operations::get_item(&conn, &i1.id).unwrap();
        let fresh_i2 = crate::db::operations::get_item(&conn, &i2.id).unwrap();
        let fresh_i3 = crate::db::operations::get_item(&conn, &i3.id).unwrap();
        assert_eq!(fresh_i1.workspace_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(fresh_i2.workspace_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(fresh_i3.workspace_id.as_deref(), Some(ws_b.id.as_str()));
    }
}
