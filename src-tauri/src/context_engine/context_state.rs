use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::context_engine::memory::{
    ContextMemory, ContextMemoryActivityState, ContextMemoryBuilder, ContextMemoryService,
};
use crate::context_engine::segments::{ContextActivitySegment, ContextActivitySegmenter};
use crate::context_engine::task_transitions::{
    DetectedTaskTransition, TaskTransitionType,
};
use crate::context_engine::transitions::{
    ContextTransition, ContextTransitionType,
};
use crate::context_engine::workset::{
    ContextWorkset, ContextWorksetBuilder, ContextWorksetService, WorksetState,
};
use crate::db::models::{ContextEvidenceItem, Item, TaskCandidate, TaskCandidateStatus};

// =========================================================================
// ContextStateValue: Conservative Working State Classification
// =========================================================================

/// Conservative classification of the user's working state inside a context.
///
/// Invariants:
/// - Never claim `Active` merely because an item exists.
/// - Never treat application presence as proof of work.
/// - Never treat the last captured item as the complete state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStateValue {
    /// Actively focused and working inside this context within the immediate active window (<= 15 min).
    Active,
    /// Recently active (15 min - 24 hours ago), ready to resume with warm state.
    RecentlyActive,
    /// Working session was paused (e.g., switched away to another context, ended task, or brief pause).
    Paused,
    /// No activity for an extended period (> 7 days), cold state.
    Dormant,
    /// Conflicting signals, ambiguous context switches, or insufficient stability.
    Uncertain,
    /// Brand new context or zero/negligible evidence to reconstruct state.
    InsufficientEvidence,
}

impl ContextStateValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::RecentlyActive => "recently_active",
            Self::Paused => "paused",
            Self::Dormant => "dormant",
            Self::Uncertain => "uncertain",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::RecentlyActive => "Recently Active",
            Self::Paused => "Paused",
            Self::Dormant => "Dormant",
            Self::Uncertain => "Uncertain",
            Self::InsufficientEvidence => "Insufficient Evidence",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn is_recently_active(&self) -> bool {
        matches!(self, Self::RecentlyActive)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused)
    }

    pub fn is_dormant(&self) -> bool {
        matches!(self, Self::Dormant)
    }

    pub fn is_uncertain(&self) -> bool {
        matches!(self, Self::Uncertain)
    }

    pub fn is_insufficient(&self) -> bool {
        matches!(self, Self::InsufficientEvidence)
    }
}

impl std::fmt::Display for ContextStateValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// ContextState: Derived Contextual Working State Representation
// =========================================================================

/// Derived representation of the user's last known working state inside a context.
///
/// Answering: "Where did I leave this work?"
///
/// Combines `ContextMemory`, `ContextWorkset`, `ContextActivitySegments`, `ContextTransitions`,
/// `TaskCandidates`, `TaskTransitions`, `Items`, and `ContextEvidence` without duplicating data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextState {
    /// Context / Workspace identifier
    pub context_id: String,
    /// Human-friendly context name (e.g. "Aurora Website", "Portfolio")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_name: Option<String>,
    /// Active or last inferred task ID if known (nullable)
    pub task_id: Option<String>,
    /// Active or last inferred task title if known (nullable)
    pub task_title: Option<String>,
    /// Conservative working state classification
    pub state: ContextStateValue,
    /// ISO-8601 timestamp of last known activity in this context (nullable)
    pub last_active_at: Option<String>,
    /// ID of the last continuous activity segment (nullable)
    pub last_activity_segment_id: Option<String>,
    /// ID of the last recorded context transition (nullable)
    pub last_transition_id: Option<String>,
    /// ID of the last meaningful item captured or interacted with (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_item_id: Option<String>,
    /// Title or label of the last meaningful item (nullable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_item_title: Option<String>,
    /// Derived contextual workset (nullable)
    pub current_workset: Option<ContextWorkset>,
    /// Relevant recent item IDs associated with this state
    pub recent_item_ids: Vec<String>,
    /// Concise human-readable summary of recent activity (nullable)
    pub recent_activity: Option<String>,
    /// Foreground application associated with recent work if known (nullable)
    pub active_application: Option<String>,
    /// Web domain associated with recent work if known (nullable)
    pub active_domain: Option<String>,
    /// Applications observed across recent context activity
    #[serde(default)]
    pub recent_applications: Vec<String>,
    /// Human-readable description of the latest meaningful action (nullable)
    pub latest_meaningful_action: Option<String>,
    /// Deterministic reconstruction answering "Where did I leave this work?"
    #[serde(default)]
    pub last_meaningful_activity: String,
    /// Calibrated state confidence score [0.0, 1.0]
    pub state_confidence: f64,
    /// Epistemic source ("confirmed", "inferred", "observed")
    #[serde(default = "default_inferred_provenance")]
    pub provenance: String,
    /// Whether user feedback explicitly grounded this state
    #[serde(default)]
    pub has_user_feedback: bool,
    /// ISO-8601 timestamp when this state was reconstructed
    pub reconstructed_at: String,
}

fn default_inferred_provenance() -> String {
    "inferred".to_string()
}

impl ContextState {
    pub fn is_working(&self) -> bool {
        matches!(self.state, ContextStateValue::Active | ContextStateValue::RecentlyActive)
    }

    pub fn last_meaningful_workset(&self) -> Option<&ContextWorkset> {
        self.current_workset.as_ref()
    }
}

// =========================================================================
// Helpers for Parsing & Timestamp Math
// =========================================================================

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ") {
        return Some(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    None
}

// =========================================================================
// ContextStateBuilder: Fluid Derivation Pipeline
// =========================================================================

/// Builder and derivation engine for `ContextState`.
#[derive(Debug, Clone)]
pub struct ContextStateBuilder {
    pub context_id: String,
    pub context_name: Option<String>,
    pub memory: Option<ContextMemory>,
    pub workset: Option<ContextWorkset>,
    pub activity_segments: Vec<ContextActivitySegment>,
    pub context_transitions: Vec<ContextTransition>,
    pub task_candidates: Vec<TaskCandidate>,
    pub task_transitions: Vec<DetectedTaskTransition>,
    pub items: Vec<Item>,
    pub evidence: Vec<ContextEvidenceItem>,
    pub has_user_feedback: bool,
    pub reference_time: Option<DateTime<Utc>>,
}

impl ContextStateBuilder {
    pub fn new(context_id: impl Into<String>) -> Self {
        Self {
            context_id: context_id.into(),
            context_name: None,
            memory: None,
            workset: None,
            activity_segments: Vec::new(),
            context_transitions: Vec::new(),
            task_candidates: Vec::new(),
            task_transitions: Vec::new(),
            items: Vec::new(),
            evidence: Vec::new(),
            has_user_feedback: false,
            reference_time: None,
        }
    }

    pub fn with_context_name(mut self, name: impl Into<String>) -> Self {
        self.context_name = Some(name.into());
        self
    }

    pub fn with_memory(mut self, memory: Option<ContextMemory>) -> Self {
        if let Some(ref m) = memory {
            if self.context_name.is_none() && !m.context_name.is_empty() {
                self.context_name = Some(m.context_name.clone());
            }
        }
        self.memory = memory;
        self
    }

    pub fn with_workset(mut self, workset: Option<ContextWorkset>) -> Self {
        if let Some(ref w) = workset {
            if self.context_name.is_none() && !w.context_name.is_empty() {
                self.context_name = Some(w.context_name.clone());
            }
        }
        self.workset = workset;
        self
    }

    pub fn with_activity_segments(mut self, segments: Vec<ContextActivitySegment>) -> Self {
        self.activity_segments = segments;
        self
    }

    pub fn with_context_transitions(mut self, transitions: Vec<ContextTransition>) -> Self {
        self.context_transitions = transitions;
        self
    }

    pub fn with_task_candidates(mut self, candidates: Vec<TaskCandidate>) -> Self {
        self.task_candidates = candidates;
        self
    }

    pub fn with_task_transitions(mut self, transitions: Vec<DetectedTaskTransition>) -> Self {
        self.task_transitions = transitions;
        self
    }

    pub fn with_items(mut self, items: Vec<Item>) -> Self {
        self.items = items;
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<ContextEvidenceItem>) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn with_user_feedback(mut self, has_feedback: bool) -> Self {
        self.has_user_feedback = has_feedback;
        self
    }

    pub fn with_reference_time(mut self, time: DateTime<Utc>) -> Self {
        self.reference_time = Some(time);
        self
    }

    /// Loads underlying sources from the database and prepares the builder.
    pub fn from_db(conn: &Connection, context_id: &str) -> Result<Self, String> {
        let ws = crate::db::operations::get_workspace(conn, context_id).ok();
        let context_name = ws.map(|w| w.name);

        let workset = ContextWorksetBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        let memory = ContextMemoryBuilder::from_db(conn, context_id)
            .and_then(|b| b.build())
            .ok();

        let activity_segments = Vec::new();

        let mut trans_stmt = conn.prepare(
            "SELECT id, from_context_id, to_context_id, transition_type, confidence, detected_at, session_id, trigger_item_id, trigger_signal_id, metadata, created_at 
             FROM context_transitions 
             WHERE from_context_id = ?1 OR to_context_id = ?1 
             ORDER BY detected_at DESC LIMIT 20",
        ).map_err(|e| e.to_string())?;

        let context_transitions: Vec<ContextTransition> = trans_stmt
            .query_map(rusqlite::params![context_id], |row| {
                let trans_type_str: String = row.get(3)?;
                let transition_type = ContextTransitionType::from_str_lenient(&trans_type_str)
                    .unwrap_or(ContextTransitionType::ContextSwitch);
                Ok(ContextTransition {
                    id: row.get(0)?,
                    from_context_id: row.get(1)?,
                    to_context_id: row.get(2)?,
                    transition_type,
                    confidence: row.get(4)?,
                    detected_at: row.get(5)?,
                    session_id: row.get(6)?,
                    trigger_item_id: row.get(7)?,
                    trigger_signal_id: row.get(8)?,
                    metadata: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        let mut task_stmt = conn.prepare(
            "SELECT id, context_id, title, confidence, status, first_detected_at, last_active_at, created_at, updated_at 
             FROM task_candidates 
             WHERE context_id = ?1 
             ORDER BY last_active_at DESC LIMIT 10",
        ).map_err(|e| e.to_string())?;

        let task_candidates: Vec<TaskCandidate> = task_stmt
            .query_map(rusqlite::params![context_id], |row| {
                let status_str: String = row.get(4)?;
                let status = match status_str.as_str() {
                    "detected" => TaskCandidateStatus::Detected,
                    "active" => TaskCandidateStatus::Active,
                    "inactive" => TaskCandidateStatus::Inactive,
                    "uncertain" => TaskCandidateStatus::Uncertain,
                    "dismissed" => TaskCandidateStatus::Dismissed,
                    _ => TaskCandidateStatus::Detected,
                };
                Ok(TaskCandidate {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    title: row.get(2)?,
                    confidence: row.get(3)?,
                    status,
                    first_detected_at: row.get(5)?,
                    last_active_at: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        let items = crate::db::operations::list_items(conn, Some(context_id))
            .unwrap_or_default();

        let mut has_user_feedback = false;
        if let Ok(mut mem_stmt) = conn.prepare("SELECT source FROM context_memberships WHERE workspace_id = ?1") {
            if let Ok(rows) = mem_stmt.query_map(rusqlite::params![context_id], |row| row.get::<_, String>(0)) {
                for s in rows.flatten() {
                    if s == "manual" || s == "user_confirmed" {
                        has_user_feedback = true;
                        break;
                    }
                }
            }
        }

        Ok(Self {
            context_id: context_id.to_string(),
            context_name,
            memory,
            workset,
            activity_segments,
            context_transitions,
            task_candidates,
            task_transitions: Vec::new(),
            items,
            evidence: Vec::new(),
            has_user_feedback,
            reference_time: None,
        })
    }

    /// Derives the conservative `ContextState` representation answering "Where did I leave this work?".
    pub fn build(self) -> Result<ContextState, String> {
        let now = self.reference_time.unwrap_or_else(Utc::now);
        let reconstructed_at = now.to_rfc3339();

        // Context name resolution
        let context_name = self.context_name.or_else(|| {
            self.workset.as_ref().map(|w| w.context_name.clone()).or_else(|| {
                self.memory.as_ref().map(|m| m.context_name.clone())
            })
        });

        // 1. Resolve Last Known Activity Timestamp
        let mut latest_ts: Option<DateTime<Utc>> = None;
        let mut latest_ts_str: Option<String> = None;

        let mut consider_ts = |ts_str: &str| {
            if let Some(parsed) = parse_ts(ts_str) {
                if latest_ts.is_none() || Some(parsed) > latest_ts {
                    latest_ts = Some(parsed);
                    latest_ts_str = Some(ts_str.to_string());
                }
            }
        };

        // From context activity segments
        for seg in &self.activity_segments {
            if seg.context.as_ref().map(|c| &c.id) == Some(&self.context_id) {
                consider_ts(&seg.ended_at);
                consider_ts(&seg.started_at);
            }
        }

        // From transitions
        for trans in &self.context_transitions {
            if trans.from_context_id.as_deref() == Some(&self.context_id)
                || trans.to_context_id.as_deref() == Some(&self.context_id)
            {
                consider_ts(&trans.detected_at);
            }
        }

        // From task candidates
        for task in &self.task_candidates {
            if task.context_id.as_deref() == Some(&self.context_id) {
                consider_ts(&task.last_active_at);
            }
        }

        // From memory
        if let Some(mem) = &self.memory {
            if let Some(la) = &mem.last_active_at {
                consider_ts(la);
            }
        }

        // From workset
        if let Some(ws) = &self.workset {
            if let Some(la) = &ws.last_active_at {
                consider_ts(la);
            }
        }

        // From items
        for item in &self.items {
            consider_ts(&item.created_at);
            consider_ts(&item.updated_at);
        }

        let last_active_at = latest_ts_str;

        // 2. Identify Latest Activity Segment and Transition IDs
        let mut context_segments: Vec<&ContextActivitySegment> = self
            .activity_segments
            .iter()
            .filter(|s| s.context.as_ref().map(|c| &c.id) == Some(&self.context_id))
            .collect();
        context_segments.sort_by(|a, b| {
            parse_ts(&b.ended_at)
                .unwrap_or(Utc::now())
                .cmp(&parse_ts(&a.ended_at).unwrap_or(Utc::now()))
        });

        let latest_segment = context_segments.first().copied();
        let last_activity_segment_id = latest_segment.map(|s| s.id.clone());

        let mut sorted_transitions = self.context_transitions.clone();
        sorted_transitions.sort_by(|a, b| {
            parse_ts(&b.detected_at)
                .unwrap_or(Utc::now())
                .cmp(&parse_ts(&a.detected_at).unwrap_or(Utc::now()))
        });

        let latest_transition = sorted_transitions.first();
        let last_transition_id = latest_transition.map(|t| t.id.clone());

        // 3. Resolve Last Meaningful Task Candidate (Nullable, zero forced guessing)
        let mut task_id: Option<String> = None;
        let mut task_title: Option<String> = None;

        if let Some(ws) = &self.workset {
            if let (Some(id), Some(title)) = (&ws.task_id, &ws.task_title) {
                if !title.is_empty() && !title.eq_ignore_ascii_case("unknown") && ws.confidence >= 0.40 {
                    task_id = Some(id.clone());
                    task_title = Some(title.clone());
                }
            }
        }

        if task_title.is_none() {
            if let Some(mem) = &self.memory {
                if let (Some(title), conf) = (&mem.current_task_title, mem.current_task_confidence) {
                    if !title.is_empty() && !title.eq_ignore_ascii_case("unknown") && conf.unwrap_or(0.0) >= 0.40 {
                        task_id = mem.current_task_id.clone();
                        task_title = Some(title.clone());
                    }
                }
            }
        }

        if task_title.is_none() {
            let mut valid_candidates: Vec<&TaskCandidate> = self
                .task_candidates
                .iter()
                .filter(|c| {
                    c.context_id.as_deref() == Some(&self.context_id)
                        && c.confidence >= 0.45
                        && !matches!(c.status, TaskCandidateStatus::Dismissed | TaskCandidateStatus::Inactive)
                        && !c.title.eq_ignore_ascii_case("unknown")
                })
                .collect();
            valid_candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(best) = valid_candidates.first() {
                task_id = Some(best.id.clone());
                task_title = Some(best.title.clone());
            }
        }

        // 4. Resolve Recent Item IDs and Last Meaningful Item
        let mut recent_item_ids = Vec::new();
        let mut seen_items = HashSet::new();
        let mut last_meaningful_item_id: Option<String> = None;
        let mut last_meaningful_item_title: Option<String> = None;

        if let Some(ws) = &self.workset {
            for item in &ws.items {
                if seen_items.insert(item.id.clone()) {
                    if last_meaningful_item_id.is_none() {
                        last_meaningful_item_id = Some(item.id.clone());
                        last_meaningful_item_title = item.title.clone();
                    }
                    recent_item_ids.push(item.id.clone());
                    if recent_item_ids.len() >= 10 {
                        break;
                    }
                }
            }
        }

        if recent_item_ids.len() < 10 {
            if let Some(seg) = latest_segment {
                for item_ref in &seg.item_references {
                    if seen_items.insert(item_ref.item_id.clone()) {
                        if last_meaningful_item_id.is_none() {
                            last_meaningful_item_id = Some(item_ref.item_id.clone());
                            last_meaningful_item_title = item_ref.title.clone();
                        }
                        recent_item_ids.push(item_ref.item_id.clone());
                        if recent_item_ids.len() >= 10 {
                            break;
                        }
                    }
                }
            }
        }

        if recent_item_ids.len() < 10 {
            for item in &self.items {
                if seen_items.insert(item.id.clone()) {
                    if last_meaningful_item_id.is_none() {
                        last_meaningful_item_id = Some(item.id.clone());
                        last_meaningful_item_title = item.title.clone();
                    }
                    recent_item_ids.push(item.id.clone());
                    if recent_item_ids.len() >= 10 {
                        break;
                    }
                }
            }
        }

        // 5. Resolve Active Applications, Domains & Multi-App Tracking
        let mut app_counts: HashMap<String, usize> = HashMap::new();
        let mut active_domain: Option<String> = None;

        if let Some(ws) = &self.workset {
            for item in &ws.items {
                if let Some(ref app) = item.app_name {
                    if !app.trim().is_empty() && !app.eq_ignore_ascii_case("unknown") {
                        *app_counts.entry(app.trim().to_string()).or_insert(0) += 1;
                    }
                }
                if active_domain.is_none() && item.domain.is_some() {
                    active_domain = item.domain.clone();
                }
            }
        }

        for item in &self.items {
            if let Ok(metadata) = serde_json::from_str::<Value>(&item.metadata) {
                if let Some(app) = metadata.get("app_name").and_then(|v| v.as_str()) {
                    let trimmed = app.trim();
                    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("unknown") {
                        *app_counts.entry(trimmed.to_string()).or_insert(0) += 1;
                    }
                }
                if active_domain.is_none() {
                    if let Some(dom) = metadata.get("domain").and_then(|v| v.as_str()) {
                        let trimmed = dom.trim();
                        if !trimmed.is_empty() && trimmed != "localhost" {
                            active_domain = Some(trimmed.to_string());
                        }
                    }
                }
            }
        }

        let mut app_list: Vec<(String, usize)> = app_counts.into_iter().collect();
        app_list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let active_application = app_list.first().map(|(app, _)| app.clone());
        let recent_applications: Vec<String> = app_list.into_iter().map(|(app, _)| app).collect();

        // 6. Resolve Recent Activity Summary
        let recent_activity: Option<String> = if let Some(ws) = &self.workset {
            if !ws.recent_activity.is_empty() {
                Some(ws.recent_activity.clone())
            } else {
                None
            }
        } else if let Some(mem) = &self.memory {
            if !mem.recent_activity.is_empty() {
                Some(mem.recent_activity.clone())
            } else {
                None
            }
        } else if !recent_item_ids.is_empty() {
            Some(format!("{} items recorded in this context", recent_item_ids.len()))
        } else {
            None
        };

        // 7. Resolve Latest Meaningful Action (Factual, grounded, zero fabricated verbs)
        let mut latest_meaningful_action: Option<String> = None;

        // Check task transitions
        if let Some(last_task_trans) = self.task_transitions.last() {
            match last_task_trans.transition_type {
                TaskTransitionType::TaskStart => {
                    let title = last_task_trans.to_task.as_deref().unwrap_or("new task");
                    latest_meaningful_action = Some(format!("Started task: {}", title));
                }
                TaskTransitionType::TaskSwitch => {
                    let from = last_task_trans.from_task.as_deref().unwrap_or("previous");
                    let to = last_task_trans.to_task.as_deref().unwrap_or("new task");
                    latest_meaningful_action = Some(format!("Switched task from {} to {}", from, to));
                }
                TaskTransitionType::TaskReturn => {
                    let title = last_task_trans.to_task.as_deref().unwrap_or("task");
                    latest_meaningful_action = Some(format!("Resumed task: {}", title));
                }
                TaskTransitionType::TaskEnd => {
                    let title = last_task_trans.from_task.as_deref().unwrap_or("active task");
                    latest_meaningful_action = Some(format!("Completed or paused task: {}", title));
                }
                TaskTransitionType::AmbiguousTaskChange => {}
            }
        }

        // Check context transitions
        if latest_meaningful_action.is_none() {
            if let Some(trans) = latest_transition {
                if trans.to_context_id.as_deref() == Some(&self.context_id) {
                    if trans.transition_type == ContextTransitionType::ContextReturn {
                        latest_meaningful_action = Some("Returned to context after interruption".to_string());
                    } else if trans.transition_type == ContextTransitionType::ContextStart {
                        latest_meaningful_action = Some("Initiated working context session".to_string());
                    } else {
                        latest_meaningful_action = Some("Switched focus to this context".to_string());
                    }
                } else if trans.from_context_id.as_deref() == Some(&self.context_id) {
                    latest_meaningful_action = Some("Switched away to another context".to_string());
                }
            }
        }

        // Fallback to active task or segment action
        if latest_meaningful_action.is_none() {
            if let Some(title) = &task_title {
                latest_meaningful_action = Some(format!("Working on {}", title));
            } else if let Some(title) = &last_meaningful_item_title {
                latest_meaningful_action = Some(format!("Interacting with {}", title));
            }
        }

        // 8. Conservative State Classification & Confidence Scoring
        let has_no_data = self.items.is_empty()
            && self.activity_segments.is_empty()
            && self.context_transitions.is_empty()
            && self.memory.is_none()
            && self.workset.is_none()
            && self.evidence.is_empty();

        let elapsed_seconds = latest_ts.map(|t| (now - t).num_seconds().max(0));

        let (state, mut state_confidence) = if has_no_data {
            (ContextStateValue::InsufficientEvidence, 0.0)
        } else if self.items.is_empty() && latest_segment.is_none() && self.evidence.is_empty() {
            (ContextStateValue::InsufficientEvidence, 0.0)
        } else if let Some(mem) = &self.memory {
            if mem.activity_state == ContextMemoryActivityState::InsufficientEvidence
                && self.items.is_empty()
                && context_segments.is_empty()
            {
                (ContextStateValue::InsufficientEvidence, 0.10)
            } else {
                Self::evaluate_state(
                    &self.context_id,
                    elapsed_seconds,
                    latest_segment,
                    latest_transition,
                    &self.task_transitions,
                    &self.workset,
                    &self.memory,
                    &self.items,
                )
            }
        } else {
            Self::evaluate_state(
                &self.context_id,
                elapsed_seconds,
                latest_segment,
                latest_transition,
                &self.task_transitions,
                &self.workset,
                &self.memory,
                &self.items,
            )
        };

        // User feedback increases confidence and solidifies state
        let provenance = if self.has_user_feedback {
            state_confidence = (state_confidence * 0.30 + 0.70).clamp(0.75, 1.0);
            "confirmed".to_string()
        } else if self.items.is_empty() {
            "observed".to_string()
        } else {
            "inferred".to_string()
        };

        // 9. Reconstruct "Where did I leave this work?" (Deterministic natural phrasing)
        let last_meaningful_activity = Self::construct_meaningful_activity_statement(
            context_name.as_deref().unwrap_or("this workspace"),
            state,
            task_title.as_deref(),
            &recent_applications,
            active_domain.as_deref(),
            last_meaningful_item_title.as_deref(),
            latest_transition,
            self.task_transitions.last(),
            &self.items,
        );

        Ok(ContextState {
            context_id: self.context_id,
            context_name,
            task_id,
            task_title,
            state,
            last_active_at,
            last_activity_segment_id,
            last_transition_id,
            last_meaningful_item_id,
            last_meaningful_item_title,
            current_workset: self.workset,
            recent_item_ids,
            recent_activity,
            active_application,
            active_domain,
            recent_applications,
            latest_meaningful_action,
            last_meaningful_activity,
            state_confidence,
            provenance,
            has_user_feedback: self.has_user_feedback,
            reconstructed_at,
        })
    }

    /// Constructs the deterministic natural working state answer to: "Where did I leave this work?"
    ///
    /// Rules:
    /// - Do NOT infer more than evidence supports.
    /// - Do NOT fabricate actions (e.g. "editing") unless an explicit signal exists.
    /// - If task + app: "Last known activity: Hero design in Figma."
    /// - If only URL captured: "Last known activity: Figma page related to Aurora Website."
    /// - If multi-app: "Last known activity: Hero design across VSCode and Safari."
    #[allow(clippy::too_many_arguments)]
    fn construct_meaningful_activity_statement(
        context_name: &str,
        state: ContextStateValue,
        task_title: Option<&str>,
        recent_apps: &[String],
        active_domain: Option<&str>,
        last_item_title: Option<&str>,
        latest_transition: Option<&ContextTransition>,
        last_task_transition: Option<&DetectedTaskTransition>,
        items: &[Item],
    ) -> String {
        // 1. Insufficient Evidence
        if state == ContextStateValue::InsufficientEvidence {
            return format!("No prior activity recorded for {}.", context_name);
        }

        // 2. Uncertain State
        if state == ContextStateValue::Uncertain {
            return format!(
                "Uncertain recent activity in {} with competing context signals.",
                context_name
            );
        }

        // 3. Context Return
        if let Some(trans) = latest_transition {
            if trans.transition_type == ContextTransitionType::ContextReturn {
                if let Some(task) = task_title {
                    return format!("Returned to {} working on {}.", context_name, task);
                } else {
                    return format!("Returned to {} context after interruption.", context_name);
                }
            }
        }

        // 4. Task Transition / Task Switch
        if let Some(tt) = last_task_transition {
            if tt.transition_type == TaskTransitionType::TaskSwitch {
                let from = tt.from_task.as_deref().unwrap_or("previous task");
                let to = tt.to_task.as_deref().unwrap_or("new task");
                return format!("Switched task from {} to {}.", from, to);
            } else if tt.transition_type == TaskTransitionType::TaskStart {
                let to = tt.to_task.as_deref().unwrap_or("new task");
                return format!("Started task: {}.", to);
            }
        }

        // 5. Context Switch Away / Paused
        if state == ContextStateValue::Paused {
            if let Some(trans) = latest_transition {
                if trans.transition_type == ContextTransitionType::ContextSwitch {
                    if let Some(task) = task_title {
                        if let Some(app) = recent_apps.first() {
                            return format!("Paused work on {} in {}.", task, app);
                        } else {
                            return format!("Paused work on {}.", task);
                        }
                    } else {
                        return format!("Paused work in {}.", context_name);
                    }
                }
            }
        }

        // 6. Dormant Context
        if state == ContextStateValue::Dormant {
            if let Some(task) = task_title {
                return format!(
                    "Dormant context. Last known activity: {} (no recent activity for over a week).",
                    task
                );
            } else {
                return format!("Dormant context with no recent activity.");
            }
        }

        // 7. Confident Task + App(s)
        if let Some(task) = task_title {
            if recent_apps.len() > 1 {
                return format!(
                    "Last known activity: {} across {} and {}.",
                    task, recent_apps[0], recent_apps[1]
                );
            } else if let Some(app) = recent_apps.first() {
                return format!("Last known activity: {} in {}.", task, app);
            } else {
                return format!("Last known activity: {}.", task);
            }
        }

        // 8. Weak Evidence / Single Capture without clear task
        // "If only a URL was captured: 'Last known activity: Figma page related to Aurora Website.'"
        // "Do NOT fabricate actions."
        if let Some(item_title) = last_item_title {
            if !item_title.trim().is_empty() && !item_title.eq_ignore_ascii_case("untitled") {
                if let Some(app) = recent_apps.first() {
                    return format!("Last known activity: {} in {}.", item_title, app);
                } else if let Some(dom) = active_domain {
                    return format!("Last known activity: {} on {}.", item_title, dom);
                } else {
                    return format!("Last known activity: {} related to {}.", item_title, context_name);
                }
            }
        }

        if let Some(app) = recent_apps.first() {
            return format!("Last known activity: {} page related to {}.", app, context_name);
        }

        if let Some(dom) = active_domain {
            return format!("Last known activity: {} page related to {}.", dom, context_name);
        }

        if !items.is_empty() {
            return format!("Last known activity: viewing items in {}.", context_name);
        }

        format!("Last known activity in {}.", context_name)
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_state(
        context_id: &str,
        elapsed_seconds: Option<i64>,
        latest_segment: Option<&ContextActivitySegment>,
        latest_transition: Option<&ContextTransition>,
        task_transitions: &[DetectedTaskTransition],
        workset: &Option<ContextWorkset>,
        memory: &Option<ContextMemory>,
        items: &[Item],
    ) -> (ContextStateValue, f64) {
        let elapsed = elapsed_seconds.unwrap_or(i64::MAX);

        // Check for severe ambiguity or contradictory transitions
        if let Some(t) = latest_transition {
            if t.transition_type == ContextTransitionType::AmbiguousTransition && t.confidence < 0.40 {
                return (ContextStateValue::Uncertain, 0.30);
            }
        }
        if let Some(tt) = task_transitions.last() {
            if tt.transition_type == TaskTransitionType::AmbiguousTaskChange && tt.confidence < 0.40 {
                return (ContextStateValue::Uncertain, 0.35);
            }
        }

        // Check for Dormancy (> 7 days or 604,800 seconds)
        if elapsed >= 7 * 86400 {
            let base_conf = if !items.is_empty() { 0.75 } else { 0.50 };
            return (ContextStateValue::Dormant, base_conf);
        }

        // Check if context memory indicates dormant state
        if let Some(mem) = memory {
            if mem.activity_state == ContextMemoryActivityState::Dormant && elapsed >= 3 * 86400 {
                return (ContextStateValue::Dormant, 0.70);
            }
        }

        // Check if user transitioned AWAY from this context (Paused)
        if let Some(trans) = latest_transition {
            if trans.from_context_id.as_deref() == Some(context_id)
                && trans.to_context_id.as_deref() != Some(context_id)
                && trans.transition_type == ContextTransitionType::ContextSwitch
            {
                return (ContextStateValue::Paused, trans.confidence.clamp(0.60, 0.95));
            }
        }

        // Check if task ended recently
        if let Some(tt) = task_transitions.last() {
            if tt.transition_type == TaskTransitionType::TaskEnd {
                return (ContextStateValue::Paused, 0.65);
            }
        }

        // Check for Active state
        // Invariant: Do NOT claim active work merely because an item exists or an app is present.
        // Requires recent activity (<= 15 minutes / 900s), active activity segment or corroborated interaction,
        // and sufficient confidence.
        if elapsed <= 900 {
            let has_active_segment = latest_segment
                .map(|s| !s.is_ambiguous && s.confidence >= 0.45)
                .unwrap_or(false);

            let has_active_workset = workset
                .as_ref()
                .map(|w| matches!(w.workset_state, WorksetState::Active | WorksetState::DetourReturn))
                .unwrap_or(false);

            let has_active_memory = memory
                .as_ref()
                .map(|m| m.activity_state == ContextMemoryActivityState::Active)
                .unwrap_or(false);

            let has_context_return = latest_transition
                .map(|t| {
                    t.to_context_id.as_deref() == Some(context_id)
                        && t.transition_type == ContextTransitionType::ContextReturn
                })
                .unwrap_or(false);

            // Conservative: Require at least one active behavioral corroboration
            if has_active_segment || has_active_workset || has_active_memory || has_context_return {
                let conf = if (has_active_segment || has_context_return) && (has_active_workset || has_active_memory) {
                    0.90
                } else {
                    0.80
                };
                return (ContextStateValue::Active, conf);
            }

            // If elapsed <= 900 but no corroborated segment, do not overclaim Active.
            // Fall back to RecentlyActive or Uncertain if items are single/weak.
            if items.len() <= 1 {
                return (ContextStateValue::RecentlyActive, 0.50);
            }
            return (ContextStateValue::RecentlyActive, 0.65);
        }

        // Check for Paused (30 min - 7 days after explicit switch or session close)
        if elapsed > 1800 && elapsed < 86400 {
            if let Some(ws) = workset {
                if ws.workset_state == WorksetState::DetourReturn {
                    return (ContextStateValue::RecentlyActive, 0.70);
                }
            }
            return (ContextStateValue::RecentlyActive, 0.65);
        }

        if elapsed >= 86400 {
            // 24h - 7 days: Paused / warm historical state
            return (ContextStateValue::Paused, 0.60);
        }

        // 15 - 30 minutes: Recently active
        (ContextStateValue::RecentlyActive, 0.70)
    }
}

// =========================================================================
// ContextStateService: Public Interface
// =========================================================================

/// Service for reconstructing and querying derived `ContextState`.
pub struct ContextStateService;

impl ContextStateService {
    pub fn new() -> Self {
        Self
    }

    /// Derives the contextual working state for a specific context/workspace.
    pub fn derive_for_context(
        &self,
        conn: &Connection,
        context_id: &str,
    ) -> Result<ContextState, String> {
        ContextStateBuilder::from_db(conn, context_id)?.build()
    }
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::segments::{ContextActivitySegment, SegmentItemReference};
    use crate::context_engine::task_transitions::{DetectedTaskTransition, TaskTransitionType};
    use crate::context_engine::transitions::{
        ContextReference, ContextTransition, ContextTransitionType,
    };
    use crate::db::models::{Item, TaskCandidate, TaskCandidateStatus};
    use chrono::{Duration, Utc};

    fn make_test_item(id: &str, title: &str, app: Option<&str>, created_at: &str) -> Item {
        let metadata = match app {
            Some(a) => format!(r#"{{"app_name":"{}"}}"#, a),
            None => "{}".to_string(),
        };
        Item {
            id: id.to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "text".to_string(),
            content: format!("Content for {}", title),
            title: Some(title.to_string()),
            metadata,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    // =====================================================================
    // STEP 3 REQUIRED TESTS:
    // strong evidence, weak evidence, conflicting evidence,
    // multiple applications, task transition, context transition,
    // long inactivity, context return
    // =====================================================================

    #[test]
    fn test_strong_evidence() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(2)).to_rfc3339();

        let item = make_test_item("item-1", "Hero design", Some("Figma"), &recent_ts);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Aurora Website")),
            started_at: (now - Duration::minutes(15)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 780,
            duration_formatted: "13m".to_string(),
            confidence: 0.90,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: "item-1".to_string(),
                title: Some("Hero design".to_string()),
                item_type: "text".to_string(),
                timestamp: recent_ts.clone(),
                confidence: 0.90,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero design".to_string(),
            confidence: 0.90,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(30)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(30)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(state.task_title.as_deref(), Some("Hero design"));
        assert_eq!(state.active_application.as_deref(), Some("Figma"));
        assert_eq!(state.last_meaningful_activity, "Last known activity: Hero design in Figma.");
    }

    #[test]
    fn test_weak_evidence() {
        let now = Utc::now();
        let capture_ts = (now - Duration::minutes(20)).to_rfc3339();

        // Only a URL / application is captured with no confident task or active continuous segment
        let item = Item {
            id: "item-url".to_string(),
            workspace_id: Some("ctx-1".to_string()),
            session_id: None,
            item_type: "url".to_string(),
            content: "https://figma.com/file/12345/Aurora".to_string(),
            title: None, // No specific title
            metadata: r#"{"app_name":"Figma","domain":"figma.com"}"#.to_string(),
            created_at: capture_ts.clone(),
            updated_at: capture_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .build()
            .unwrap();

        assert_eq!(state.task_title, None);
        assert_eq!(state.active_application.as_deref(), Some("Figma"));
        // Factual and grounded: zero fabricated action verbs
        assert_eq!(state.last_meaningful_activity, "Last known activity: Figma page related to Aurora Website.");
    }

    #[test]
    fn test_conflicting_evidence() {
        let now = Utc::now();
        let ambiguous_ts = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-1", "Ambiguous signal", None, &ambiguous_ts);

        let ambiguous_trans = ContextTransition {
            id: "trans-amb".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::AmbiguousTransition,
            confidence: 0.30,
            detected_at: ambiguous_ts.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: ambiguous_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![ambiguous_trans])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Uncertain);
        assert_eq!(
            state.last_meaningful_activity,
            "Uncertain recent activity in Aurora Website with competing context signals."
        );
    }

    #[test]
    fn test_multiple_applications() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(5)).to_rfc3339();

        let item_code = make_test_item("item-1", "Hero component", Some("VSCode"), &recent_ts);
        let item_browser = make_test_item("item-2", "Hero animation specs", Some("Safari"), &recent_ts);

        let seg = ContextActivitySegment {
            id: "seg-multi".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Portfolio")),
            started_at: (now - Duration::minutes(20)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 900,
            duration_formatted: "15m".to_string(),
            confidence: 0.88,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 2,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero section".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(30)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(30)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Portfolio")
            .with_reference_time(now)
            .with_items(vec![item_code, item_browser])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(state.recent_applications.len(), 2);
        assert!(state.recent_applications.contains(&"VSCode".to_string()));
        assert!(state.recent_applications.contains(&"Safari".to_string()));
        assert_eq!(
            state.last_meaningful_activity,
            "Last known activity: Hero section across Safari and VSCode."
        );
    }

    #[test]
    fn test_task_transition() {
        let now = Utc::now();
        let task_shift_time = (now - Duration::minutes(4)).to_rfc3339();

        let item = make_test_item("item-1", "Footer contact form", Some("VSCode"), &task_shift_time);

        let task_trans = DetectedTaskTransition {
            id: "tt-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            transition_type: TaskTransitionType::TaskSwitch,
            from_task: Some("Hero Animation".to_string()),
            to_task: Some("Contact Form Integration".to_string()),
            confidence: 0.82,
            timestamp: task_shift_time.clone(),
            evidence: vec![],
            trigger_item_id: None,
            duration_seconds: None,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_task_transitions(vec![task_trans])
            .build()
            .unwrap();

        assert_eq!(
            state.last_meaningful_activity,
            "Switched task from Hero Animation to Contact Form Integration."
        );
    }

    #[test]
    fn test_context_transition() {
        let now = Utc::now();
        let switch_time = (now - Duration::minutes(25)).to_rfc3339();

        let item = make_test_item("item-1", "Hero design", Some("Figma"), &switch_time);

        let transition = ContextTransition {
            id: "trans-switch-1".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::ContextSwitch,
            confidence: 0.90,
            detected_at: switch_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: switch_time.clone(),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero design".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(60)).to_rfc3339(),
            last_active_at: switch_time.clone(),
            created_at: (now - Duration::minutes(60)).to_rfc3339(),
            updated_at: switch_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![transition])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Paused);
        assert_eq!(
            state.last_meaningful_activity,
            "Paused work on Hero design in Figma."
        );
    }

    #[test]
    fn test_long_inactivity() {
        let now = Utc::now();
        let two_weeks_ago = (now - Duration::days(14)).to_rfc3339();

        let item = make_test_item("item-old", "Archived database specs", None, &two_weeks_ago);

        let task = TaskCandidate {
            id: "task-old".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Database Migration".to_string(),
            confidence: 0.80,
            status: TaskCandidateStatus::Active,
            first_detected_at: two_weeks_ago.clone(),
            last_active_at: two_weeks_ago.clone(),
            created_at: two_weeks_ago.clone(),
            updated_at: two_weeks_ago.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Backend Core")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Dormant);
        assert_eq!(
            state.last_meaningful_activity,
            "Dormant context. Last known activity: Database Migration (no recent activity for over a week)."
        );
    }

    #[test]
    fn test_context_return() {
        let now = Utc::now();
        let return_time = (now - Duration::minutes(2)).to_rfc3339();

        let item = make_test_item("item-1", "Hero animation code", Some("Xcode"), &return_time);

        let return_transition = ContextTransition {
            id: "trans-ret-1".to_string(),
            from_context_id: Some("ctx-detour".to_string()),
            to_context_id: Some("ctx-1".to_string()),
            transition_type: ContextTransitionType::ContextReturn,
            confidence: 0.88,
            detected_at: return_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: return_time.clone(),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Hero Animation".to_string(),
            confidence: 0.88,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(40)).to_rfc3339(),
            last_active_at: return_time.clone(),
            created_at: (now - Duration::minutes(40)).to_rfc3339(),
            updated_at: return_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_context_name("Aurora Website")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![return_transition])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert_eq!(
            state.last_meaningful_activity,
            "Returned to Aurora Website working on Hero Animation."
        );
    }

    // =====================================================================
    // STEP 2 BASELINE REGRESSION TESTS
    // =====================================================================

    #[test]
    fn test_active_context() {
        let now = Utc::now();
        let recent_ts = (now - Duration::minutes(3)).to_rfc3339();

        let item = make_test_item("item-1", "HeroBanner Component", Some("VSCode"), &recent_ts);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "Portfolio")),
            started_at: (now - Duration::minutes(10)).to_rfc3339(),
            ended_at: recent_ts.clone(),
            duration_seconds: 420,
            duration_formatted: "7m".to_string(),
            confidence: 0.85,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![SegmentItemReference {
                item_id: "item-1".to_string(),
                title: Some("HeroBanner Component".to_string()),
                item_type: "text".to_string(),
                timestamp: recent_ts.clone(),
                confidence: 0.85,
            }],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let task = TaskCandidate {
            id: "task-1".to_string(),
            context_id: Some("ctx-1".to_string()),
            title: "Build Hero Section".to_string(),
            confidence: 0.85,
            status: TaskCandidateStatus::Active,
            first_detected_at: (now - Duration::minutes(20)).to_rfc3339(),
            last_active_at: recent_ts.clone(),
            created_at: (now - Duration::minutes(20)).to_rfc3339(),
            updated_at: recent_ts.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .with_task_candidates(vec![task])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Active);
        assert!(state.state_confidence >= 0.75);
        assert_eq!(state.task_title.as_deref(), Some("Build Hero Section"));
        assert_eq!(state.task_id.as_deref(), Some("task-1"));
        assert_eq!(state.last_activity_segment_id.as_deref(), Some("seg-1"));
        assert_eq!(state.active_application.as_deref(), Some("VSCode"));
        assert!(state.latest_meaningful_action.is_some());
    }

    #[test]
    fn test_recently_active_context() {
        let now = Utc::now();
        let two_hours_ago = (now - Duration::hours(2)).to_rfc3339();

        let item = make_test_item("item-1", "Trip planning itinerary", Some("Safari"), &two_hours_ago);
        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "TripWise")),
            started_at: (now - Duration::hours(3)).to_rfc3339(),
            ended_at: two_hours_ago.clone(),
            duration_seconds: 3600,
            duration_formatted: "1h".to_string(),
            confidence: 0.80,
            session_ids: vec!["sess-1".to_string()],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::RecentlyActive);
        assert!(state.state_confidence >= 0.60);
        assert_eq!(state.last_active_at, Some(two_hours_ago));
    }

    #[test]
    fn test_paused_context() {
        let now = Utc::now();
        let switch_time = (now - Duration::minutes(25)).to_rfc3339();

        let item = make_test_item("item-1", "Database schema", Some("DataGrip"), &switch_time);

        let transition = ContextTransition {
            id: "trans-1".to_string(),
            from_context_id: Some("ctx-1".to_string()),
            to_context_id: Some("ctx-2".to_string()),
            transition_type: ContextTransitionType::ContextSwitch,
            confidence: 0.90,
            detected_at: switch_time.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: switch_time.clone(),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_context_transitions(vec![transition])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Paused);
        assert_eq!(state.last_transition_id.as_deref(), Some("trans-1"));
        assert_eq!(
            state.latest_meaningful_action.as_deref(),
            Some("Switched away to another context")
        );
    }

    #[test]
    fn test_dormant_context() {
        let now = Utc::now();
        let two_weeks_ago = (now - Duration::days(14)).to_rfc3339();

        let item = make_test_item("item-old", "Archived specs", None, &two_weeks_ago);

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::Dormant);
        assert!(state.state_confidence > 0.0);
    }

    #[test]
    fn test_missing_task() {
        let now = Utc::now();
        let five_mins_ago = (now - Duration::minutes(5)).to_rfc3339();

        let item = make_test_item("item-1", "Scratch notes", Some("Notes"), &five_mins_ago);

        let seg = ContextActivitySegment {
            id: "seg-1".to_string(),
            context: Some(ContextReference::workspace("ctx-1", "General Workspace")),
            started_at: (now - Duration::minutes(10)).to_rfc3339(),
            ended_at: five_mins_ago.clone(),
            duration_seconds: 300,
            duration_formatted: "5m".to_string(),
            confidence: 0.70,
            session_ids: vec![],
            item_references: vec![],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 1,
            metadata: serde_json::json!({}),
        };

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_activity_segments(vec![seg])
            .build()
            .unwrap();

        assert_eq!(state.task_id, None);
        assert_eq!(state.task_title, None);
        assert_eq!(state.state, ContextStateValue::Active);
    }

    #[test]
    fn test_missing_workset() {
        let now = Utc::now();
        let ten_mins_ago = (now - Duration::minutes(10)).to_rfc3339();

        let item = make_test_item("item-1", "Design mock", Some("Figma"), &ten_mins_ago);

        let state = ContextStateBuilder::new("ctx-1")
            .with_reference_time(now)
            .with_items(vec![item])
            .with_workset(None)
            .build()
            .unwrap();

        assert_eq!(state.current_workset, None);
        assert_eq!(state.state, ContextStateValue::RecentlyActive);
        assert_eq!(state.recent_item_ids, vec!["item-1".to_string()]);
    }

    #[test]
    fn test_insufficient_evidence() {
        let now = Utc::now();

        let state = ContextStateBuilder::new("ctx-empty")
            .with_reference_time(now)
            .build()
            .unwrap();

        assert_eq!(state.state, ContextStateValue::InsufficientEvidence);
        assert_eq!(state.state_confidence, 0.0);
        assert_eq!(state.task_id, None);
        assert_eq!(state.task_title, None);
        assert_eq!(state.last_active_at, None);
        assert!(state.recent_item_ids.is_empty());
    }
}
