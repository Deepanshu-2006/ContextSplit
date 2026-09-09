pub use crate::db::models::{ContextInterpretationKind, ContextTransition, ContextTransitionType};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

/// Reference to a contextual state (either an established Workspace or a detected Candidate).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ContextReference {
    pub id: String,
    pub name: String,
    pub is_candidate: bool,
}

impl ContextReference {
    pub fn workspace(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_candidate: false,
        }
    }

    pub fn candidate(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_candidate: true,
        }
    }
}

/// Working memory snapshot of an active contextual state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextSnapshot {
    pub reference: ContextReference,
    pub confidence: f64,
    pub confidence_band: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub streak_count: usize,
    pub session_id: Option<String>,
}

/// Result of a detected contextual transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedTransition {
    pub id: String,
    pub from_context: Option<ContextReference>,
    pub to_context: Option<ContextReference>,
    pub timestamp: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub transition_type: ContextTransitionType,
    pub session_id: Option<String>,
    pub trigger_item_id: Option<String>,
    pub metadata: Value,
}

impl DetectedTransition {
    /// Converts this detected transition into a persistent `ContextTransition` model.
    pub fn to_context_transition(&self) -> ContextTransition {
        let mut meta = self.metadata.clone();
        if !meta.is_object() {
            meta = serde_json::json!({});
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "evidence".to_string(),
                serde_json::to_value(&self.evidence).unwrap_or_default(),
            );
            if let Some(from) = &self.from_context {
                obj.insert(
                    "from_context_name".to_string(),
                    serde_json::Value::String(from.name.clone()),
                );
                obj.insert(
                    "from_is_candidate".to_string(),
                    serde_json::Value::Bool(from.is_candidate),
                );
            }
            if let Some(to) = &self.to_context {
                obj.insert(
                    "to_context_name".to_string(),
                    serde_json::Value::String(to.name.clone()),
                );
                obj.insert(
                    "to_is_candidate".to_string(),
                    serde_json::Value::Bool(to.is_candidate),
                );
            }
        }

        ContextTransition {
            id: self.id.clone(),
            from_context_id: self.from_context.as_ref().map(|c| c.id.clone()),
            to_context_id: self.to_context.as_ref().map(|c| c.id.clone()),
            transition_type: self.transition_type,
            confidence: self.confidence,
            detected_at: self.timestamp.clone(),
            session_id: self.session_id.clone(),
            trigger_item_id: self.trigger_item_id.clone(),
            trigger_signal_id: None,
            metadata: meta.to_string(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Comprehensive result of evaluating an item against the current contextual state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransitionEvaluationResult {
    /// Meaningful contextual transition detected (Switch, Return, Start, End, Ambiguous).
    Transition(DetectedTransition),
    /// Activity continued within the current context without a state transition (Context A -> Context A continuation).
    Continuation {
        context: ContextReference,
        streak_count: usize,
        confidence: f64,
        timestamp: String,
        evidence: Vec<String>,
    },
    /// No confident context or ignored ambient noise (anti-jitter filter).
    NoTransition {
        reason: String,
        timestamp: String,
    },
}

impl TransitionEvaluationResult {
    pub fn transition(&self) -> Option<&DetectedTransition> {
        match self {
            TransitionEvaluationResult::Transition(t) => Some(t),
            _ => None,
        }
    }

    pub fn is_transition(&self) -> bool {
        matches!(self, TransitionEvaluationResult::Transition(_))
    }

    pub fn is_continuation(&self) -> bool {
        matches!(self, TransitionEvaluationResult::Continuation { .. })
    }
}

/// State-aware detector for meaningful changes between contextual states.
///
/// Detects:
/// 1. Context A -> Context B (`ContextSwitch`)
/// 2. Context A -> Inactivity (`ContextEnd`)
/// 3. Inactivity -> Context B (`ContextStart`)
/// 4. Context A -> Ambiguous (`AmbiguousTransition`)
/// 5. Context A -> Context A continuation (`Continuation`)
/// 6. Context A -> Context B -> Context A return (`ContextReturn`)
///
/// Invariant: Detecting transitions is strictly an activity inference.
/// It NEVER modifies Item or Workspace memberships.
#[derive(Debug, Clone)]
pub struct ContextTransitionDetector {
    pub inactivity_threshold_secs: i64,
    pub min_switch_confidence: f64,
    pub history_depth: usize,
    pub current_context: Option<ContextSnapshot>,
    pub context_history: VecDeque<ContextSnapshot>,
    pub last_activity_at: Option<String>,
    pub active_session_id: Option<String>,
}

impl ContextTransitionDetector {
    pub const DEFAULT_INACTIVITY_THRESHOLD_SECS: i64 = 1800; // 30 minutes
    pub const DEFAULT_MIN_SWITCH_CONFIDENCE: f64 = 0.40; // Medium threshold
    pub const DEFAULT_HISTORY_DEPTH: usize = 5;

    pub fn new() -> Self {
        Self {
            inactivity_threshold_secs: Self::DEFAULT_INACTIVITY_THRESHOLD_SECS,
            min_switch_confidence: Self::DEFAULT_MIN_SWITCH_CONFIDENCE,
            history_depth: Self::DEFAULT_HISTORY_DEPTH,
            current_context: None,
            context_history: VecDeque::new(),
            last_activity_at: None,
            active_session_id: None,
        }
    }

    pub fn with_inactivity_threshold(mut self, secs: i64) -> Self {
        self.inactivity_threshold_secs = secs.max(1);
        self
    }

    pub fn with_min_switch_confidence(mut self, conf: f64) -> Self {
        self.min_switch_confidence = conf.clamp(0.0, 1.0);
        self
    }

    pub fn with_history_depth(mut self, depth: usize) -> Self {
        self.history_depth = depth.max(1);
        self
    }

    pub fn current_context(&self) -> Option<&ContextSnapshot> {
        self.current_context.as_ref()
    }

    pub fn context_history(&self) -> &VecDeque<ContextSnapshot> {
        &self.context_history
    }

    pub fn reset(&mut self) {
        self.current_context = None;
        self.context_history.clear();
        self.last_activity_at = None;
        self.active_session_id = None;
    }

    /// Pushes a snapshot to the bounded context history, avoiding adjacent duplicate references.
    fn push_history(&mut self, snapshot: ContextSnapshot) {
        if let Some(front) = self.context_history.front() {
            if front.reference.id == snapshot.reference.id {
                return;
            }
        }
        self.context_history.push_front(snapshot);
        while self.context_history.len() > self.history_depth {
            self.context_history.pop_back();
        }
    }

    /// Checks if the inactivity threshold has elapsed since `last_activity_at`.
    ///
    /// Detects Scenario 2: Context A -> Inactivity (`ContextEnd`).
    pub fn check_inactivity(&mut self, current_time_iso: &str) -> Option<DetectedTransition> {
        let last_time_str = self.last_activity_at.as_deref()?;
        let last_dt = last_time_str.parse::<DateTime<Utc>>().ok()?;
        let now_dt = current_time_iso
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        let elapsed = now_dt.signed_duration_since(last_dt).num_seconds();
        if elapsed >= self.inactivity_threshold_secs && self.current_context.is_some() {
            let ended = self.current_context.take().unwrap();
            let ref_clone = ended.reference.clone();
            self.push_history(ended);

            let evidence = vec![
                format!(
                    "Inactivity gap detected: {:.1} minutes without activity exceeded the {}-second threshold",
                    elapsed as f64 / 60.0,
                    self.inactivity_threshold_secs
                ),
                format!("Concluded contextual activity in '{}'", ref_clone.name),
            ];

            let transition = DetectedTransition {
                id: uuid::Uuid::new_v4().to_string(),
                from_context: Some(ref_clone),
                to_context: None,
                timestamp: current_time_iso.to_string(),
                confidence: 1.0,
                evidence,
                transition_type: ContextTransitionType::ContextEnd,
                session_id: self.active_session_id.clone(),
                trigger_item_id: None,
                metadata: serde_json::json!({
                    "elapsed_seconds": elapsed,
                    "inactivity_threshold_secs": self.inactivity_threshold_secs,
                }),
            };

            Some(transition)
        } else {
            None
        }
    }

    /// Pure transition evaluation given an item context candidate and metadata.
    ///
    /// Evaluates all 6 core transition patterns:
    /// 1. Context A -> Context B (`ContextSwitch`)
    /// 2. Context A -> Inactivity (`ContextEnd`)
    /// 3. Inactivity -> Context B (`ContextStart`)
    /// 4. Context A -> Ambiguous (`AmbiguousTransition`)
    /// 5. Context A -> Context A continuation (`Continuation`)
    /// 6. Context A -> Context B -> Context A return (`ContextReturn`)
    pub fn evaluate_interpretation(
        &mut self,
        item_id: Option<&str>,
        session_id: Option<&str>,
        item_timestamp: &str,
        target_context: Option<ContextReference>,
        confidence: f64,
        confidence_band: &str,
        is_ambiguous: bool,
        evidence: Vec<String>,
    ) -> TransitionEvaluationResult {
        let item_time_dt = item_timestamp
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        // Check if an inactivity gap occurred prior to this item
        let is_resumed_after_inactivity = if let Some(last_time_str) = &self.last_activity_at {
            if let Ok(last_dt) = last_time_str.parse::<DateTime<Utc>>() {
                item_time_dt.signed_duration_since(last_dt).num_seconds()
                    >= self.inactivity_threshold_secs
            } else {
                false
            }
        } else {
            false
        };

        if is_resumed_after_inactivity && self.current_context.is_some() {
            let prior = self.current_context.take().unwrap();
            self.push_history(prior);
        }

        // Scenario 4: Context A -> Ambiguous
        if is_ambiguous {
            if let Some(curr) = &self.current_context {
                let from_ref = curr.reference.clone();
                let amb_conf = confidence.clamp(0.20, 0.50);

                let mut full_evidence = vec![
                    "Activity shifted into an ambiguous state with competing context candidates"
                        .to_string(),
                ];
                full_evidence.extend(evidence.clone());

                let transition = DetectedTransition {
                    id: uuid::Uuid::new_v4().to_string(),
                    from_context: Some(from_ref),
                    to_context: None,
                    timestamp: item_timestamp.to_string(),
                    confidence: amb_conf,
                    evidence: full_evidence,
                    transition_type: ContextTransitionType::AmbiguousTransition,
                    session_id: session_id.map(|s| s.to_string()),
                    trigger_item_id: item_id.map(|s| s.to_string()),
                    metadata: serde_json::json!({
                        "ambiguity_reason": "Competing affinities across multiple contexts",
                    }),
                };

                self.last_activity_at = Some(item_timestamp.to_string());
                return TransitionEvaluationResult::Transition(transition);
            } else {
                return TransitionEvaluationResult::NoTransition {
                    reason: "Ambiguous activity observed with no active context established"
                        .to_string(),
                    timestamp: item_timestamp.to_string(),
                };
            }
        }

        // Handle target candidate context
        let target = match target_context {
            Some(t) => t,
            None => {
                // Scenario: Weak evidence, no context, or noise (anti-jitter filter)
                return TransitionEvaluationResult::NoTransition {
                    reason: "Insufficient contextual evidence to infer a state transition"
                        .to_string(),
                    timestamp: item_timestamp.to_string(),
                };
            }
        };

        // Anti-jitter: Ignore switches below minimum confidence threshold
        if confidence < self.min_switch_confidence {
            return TransitionEvaluationResult::NoTransition {
                reason: format!(
                    "Context candidate '{}' confidence ({:.0}%) below minimum switch threshold ({:.0}%)",
                    target.name,
                    confidence * 100.0,
                    self.min_switch_confidence * 100.0
                ),
                timestamp: item_timestamp.to_string(),
            };
        }

        // Evaluate against current active context
        match &mut self.current_context {
            Some(curr) if curr.reference.id == target.id => {
                // Scenario 5: Context A -> Context A continuation
                curr.streak_count += 1;
                curr.last_seen_at = item_timestamp.to_string();
                curr.confidence = (curr.confidence * 0.7 + confidence * 0.3).clamp(0.0, 1.0);
                self.last_activity_at = Some(item_timestamp.to_string());
                self.active_session_id = session_id.map(|s| s.to_string());

                let mut cont_evidence = vec![format!(
                    "Continued activity in '{}' (streak: {} items)",
                    curr.reference.name, curr.streak_count
                )];
                cont_evidence.extend(evidence);

                TransitionEvaluationResult::Continuation {
                    context: curr.reference.clone(),
                    streak_count: curr.streak_count,
                    confidence,
                    timestamp: item_timestamp.to_string(),
                    evidence: cont_evidence,
                }
            }
            Some(curr) => {
                // Different context than active: either Switch (Scenario 1) or Return (Scenario 6)
                let from_ref = curr.reference.clone();
                let is_return = self.context_history.iter().any(|h| h.reference.id == target.id);

                let (trans_type, trans_reason) = if is_return {
                    (
                        ContextTransitionType::ContextReturn,
                        format!(
                            "Returned to '{}' after detour in '{}'",
                            target.name, from_ref.name
                        ),
                    )
                } else {
                    (
                        ContextTransitionType::ContextSwitch,
                        format!(
                            "Switched context from '{}' to '{}'",
                            from_ref.name, target.name
                        ),
                    )
                };

                let prior_snapshot = self.current_context.take().unwrap();
                self.push_history(prior_snapshot);

                let new_snapshot = ContextSnapshot {
                    reference: target.clone(),
                    confidence,
                    confidence_band: confidence_band.to_string(),
                    first_seen_at: item_timestamp.to_string(),
                    last_seen_at: item_timestamp.to_string(),
                    streak_count: 1,
                    session_id: session_id.map(|s| s.to_string()),
                };
                self.current_context = Some(new_snapshot);
                self.last_activity_at = Some(item_timestamp.to_string());
                self.active_session_id = session_id.map(|s| s.to_string());

                let mut full_evidence = vec![trans_reason];
                full_evidence.extend(evidence);

                let transition = DetectedTransition {
                    id: uuid::Uuid::new_v4().to_string(),
                    from_context: Some(from_ref),
                    to_context: Some(target),
                    timestamp: item_timestamp.to_string(),
                    confidence,
                    evidence: full_evidence,
                    transition_type: trans_type,
                    session_id: session_id.map(|s| s.to_string()),
                    trigger_item_id: item_id.map(|s| s.to_string()),
                    metadata: serde_json::json!({
                        "is_return": is_return,
                    }),
                };

                TransitionEvaluationResult::Transition(transition)
            }
            None => {
                // Scenario 3: Inactivity -> Context B (`ContextStart`)
                let new_snapshot = ContextSnapshot {
                    reference: target.clone(),
                    confidence,
                    confidence_band: confidence_band.to_string(),
                    first_seen_at: item_timestamp.to_string(),
                    last_seen_at: item_timestamp.to_string(),
                    streak_count: 1,
                    session_id: session_id.map(|s| s.to_string()),
                };
                self.current_context = Some(new_snapshot);
                self.last_activity_at = Some(item_timestamp.to_string());
                self.active_session_id = session_id.map(|s| s.to_string());

                let mut start_evidence = vec![format!("Started activity in '{}'", target.name)];
                start_evidence.extend(evidence);

                let transition = DetectedTransition {
                    id: uuid::Uuid::new_v4().to_string(),
                    from_context: None,
                    to_context: Some(target),
                    timestamp: item_timestamp.to_string(),
                    confidence,
                    evidence: start_evidence,
                    transition_type: ContextTransitionType::ContextStart,
                    session_id: session_id.map(|s| s.to_string()),
                    trigger_item_id: item_id.map(|s| s.to_string()),
                    metadata: serde_json::json!({
                        "resumed_after_inactivity": is_resumed_after_inactivity,
                    }),
                };

                TransitionEvaluationResult::Transition(transition)
            }
        }
    }

    /// Evaluates an item by ID from the database against the current contextual state.
    ///
    /// Respects user-confirmed workspace memberships as authoritative ground-truth,
    /// falls back to Context Engine interpretation scoring, and prevents spurious transitions.
    ///
    /// Invariant: Does NOT modify item or workspace memberships.
    pub fn process_item(
        &mut self,
        conn: &Connection,
        item_id: &str,
    ) -> Result<TransitionEvaluationResult, String> {
        let item = crate::db::operations::get_item(conn, item_id)
            .map_err(|e| format!("Item '{}' not found: {}", item_id, e))?;

        // 1. Check user-confirmed membership (authoritative context)
        let confirmed_workspace = if let Some(ws_id) = &item.workspace_id {
            if ws_id != "inbox" && !ws_id.is_empty() {
                crate::db::operations::get_workspace(conn, ws_id).ok()
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ws) = confirmed_workspace {
            let target_ref = ContextReference::workspace(&ws.id, &ws.name);
            let evidence = vec![format!(
                "User-confirmed item assignment to workspace '{}'",
                ws.name
            )];
            return Ok(self.evaluate_interpretation(
                Some(&item.id),
                item.session_id.as_deref(),
                &item.created_at,
                Some(target_ref),
                1.0,
                "Very High",
                false,
                evidence,
            ));
        }

        // 2. Otherwise, evaluate through the Context Engine scoring pipeline
        let interp = crate::context_engine::scoring::evaluate_item_context(conn, item_id)?;

        let is_ambiguous = interp.interpretation_kind
            == Some(crate::db::models::ContextInterpretationKind::Ambiguous);

        let (target_ref, conf, band, evidence) = if let Some(ws) = interp.existing_workspaces.first()
        {
            if ws.confidence >= self.min_switch_confidence && ws.score >= 0.35 {
                (
                    Some(ContextReference::workspace(&ws.workspace_id, &ws.workspace_name)),
                    ws.confidence,
                    ws.confidence_band.as_str(),
                    ws.reasons.clone(),
                )
            } else {
                (None, ws.confidence, ws.confidence_band.as_str(), vec![interp.summary.clone()])
            }
        } else if let Some(cand) = interp.potential_new_contexts.first() {
            if cand.confidence >= self.min_switch_confidence {
                (
                    Some(ContextReference::candidate(
                        cand.candidate_id.as_deref().unwrap_or("new_candidate"),
                        &cand.label,
                    )),
                    cand.confidence,
                    cand.confidence_band.as_str(),
                    cand.reasons.clone(),
                )
            } else {
                (None, cand.confidence, cand.confidence_band.as_str(), vec![interp.summary.clone()])
            }
        } else {
            (None, 0.0, "Very Low", vec![interp.summary.clone()])
        };

        Ok(self.evaluate_interpretation(
            Some(&item.id),
            item.session_id.as_deref(),
            &item.created_at,
            target_ref,
            conf,
            band,
            is_ambiguous,
            evidence,
        ))
    }

    /// Evaluates a chronological list of items and returns all detected state transitions.
    ///
    /// Invariant: Does NOT modify Item or Workspace memberships.
    pub fn detect_sequence(
        &mut self,
        conn: &Connection,
        item_ids: &[String],
    ) -> Result<Vec<DetectedTransition>, String> {
        let mut transitions = Vec::new();
        for id in item_ids {
            match self.process_item(conn, id)? {
                TransitionEvaluationResult::Transition(t) => transitions.push(t),
                _ => {}
            }
        }
        Ok(transitions)
    }
}

/// Builder helper for constructing validated `ContextTransition` instances.
#[derive(Debug, Clone)]
pub struct ContextTransitionBuilder {
    id: Option<String>,
    from_context_id: Option<String>,
    to_context_id: Option<String>,
    transition_type: ContextTransitionType,
    confidence: f64,
    detected_at: Option<String>,
    session_id: Option<String>,
    trigger_item_id: Option<String>,
    trigger_signal_id: Option<String>,
    metadata: Value,
}

impl ContextTransitionBuilder {
    pub fn new(transition_type: ContextTransitionType, confidence: f64) -> Self {
        Self {
            id: None,
            from_context_id: None,
            to_context_id: None,
            transition_type,
            confidence: confidence.clamp(0.0, 1.0),
            detected_at: None,
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn from_context(mut self, context_id: impl Into<String>) -> Self {
        self.from_context_id = Some(context_id.into());
        self
    }

    pub fn from_context_opt(mut self, context_id: Option<String>) -> Self {
        self.from_context_id = context_id;
        self
    }

    pub fn to_context(mut self, context_id: impl Into<String>) -> Self {
        self.to_context_id = Some(context_id.into());
        self
    }

    pub fn to_context_opt(mut self, context_id: Option<String>) -> Self {
        self.to_context_id = context_id;
        self
    }

    pub fn detected_at(mut self, timestamp: impl Into<String>) -> Self {
        self.detected_at = Some(timestamp.into());
        self
    }

    pub fn session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn session_opt(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn trigger_item(mut self, item_id: impl Into<String>) -> Self {
        self.trigger_item_id = Some(item_id.into());
        self
    }

    pub fn trigger_item_opt(mut self, item_id: Option<String>) -> Self {
        self.trigger_item_id = item_id;
        self
    }

    pub fn trigger_signal(mut self, signal_id: impl Into<String>) -> Self {
        self.trigger_signal_id = Some(signal_id.into());
        self
    }

    pub fn trigger_signal_opt(mut self, signal_id: Option<String>) -> Self {
        self.trigger_signal_id = signal_id;
        self
    }

    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn build(self) -> ContextTransition {
        let now = Utc::now().to_rfc3339();
        ContextTransition {
            id: self.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            from_context_id: self.from_context_id,
            to_context_id: self.to_context_id,
            transition_type: self.transition_type,
            confidence: self.confidence,
            detected_at: self.detected_at.unwrap_or_else(|| now.clone()),
            session_id: self.session_id,
            trigger_item_id: self.trigger_item_id,
            trigger_signal_id: self.trigger_signal_id,
            metadata: self.metadata.to_string(),
            created_at: now,
        }
    }
}

/// Formats a human-readable summary of a contextual transition.
pub fn format_transition_summary(
    transition: &ContextTransition,
    from_name: Option<&str>,
    to_name: Option<&str>,
) -> String {
    let from_label = from_name.unwrap_or("Previous Context");
    let to_label = to_name.unwrap_or("New Context");

    match transition.transition_type {
        ContextTransitionType::ContextSwitch => {
            if let (Some(_), Some(_)) = (&transition.from_context_id, &transition.to_context_id) {
                format!(
                    "Context switched from '{}' to '{}' ({:.0}% confidence)",
                    from_label,
                    to_label,
                    transition.confidence * 100.0
                )
            } else {
                format!(
                    "Context switched to '{}' ({:.0}% confidence)",
                    to_label,
                    transition.confidence * 100.0
                )
            }
        }
        ContextTransitionType::ContextReturn => format!(
            "Returned to '{}' after detour ({:.0}% confidence)",
            to_label,
            transition.confidence * 100.0
        ),
        ContextTransitionType::ContextStart => format!(
            "Started activity in '{}' ({:.0}% confidence)",
            to_label,
            transition.confidence * 100.0
        ),
        ContextTransitionType::ContextEnd => format!(
            "Concluded activity in '{}' ({:.0}% confidence)",
            from_label,
            transition.confidence * 100.0
        ),
        ContextTransitionType::AmbiguousTransition => format!(
            "Ambiguous activity shift ({:.0}% confidence)",
            transition.confidence * 100.0
        ),
    }
}

/// Persists a `ContextTransition` into the SQLite database.
///
/// Invariant: Recording a transition is purely an activity inference and
/// NEVER alters Item or Workspace memberships.
pub fn record_context_transition(
    conn: &Connection,
    transition: &ContextTransition,
) -> Result<ContextTransition, String> {
    crate::db::operations::create_context_transition(conn, transition)
        .map_err(|e| format!("Failed to record context transition: {}", e))
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
    fn test_transition_types_and_metadata() {
        let types = [
            (ContextTransitionType::ContextSwitch, "context_switch", "Context Switch", true),
            (ContextTransitionType::ContextReturn, "context_return", "Return to Context", true),
            (ContextTransitionType::ContextStart, "context_start", "Context Start", true),
            (ContextTransitionType::ContextEnd, "context_end", "Context End", true),
            (ContextTransitionType::AmbiguousTransition, "ambiguous_transition", "Ambiguous Transition", false),
        ];

        for (t_enum, t_str, display, is_conf) in types {
            assert_eq!(t_enum.as_str(), t_str);
            assert_eq!(t_enum.display_name(), display);
            assert_eq!(t_enum.is_confident(), is_conf);
            assert_eq!(ContextTransitionType::from_str_lenient(t_str), Some(t_enum));
        }
    }

    #[test]
    fn test_scenario_1_context_switch_a_to_b() {
        let mut detector = ContextTransitionDetector::new();

        // 1. Start in Context A (Project Alpha)
        let ctx_a = ContextReference::workspace("ws_a", "Project Alpha");
        let res1 = detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Active app VS Code on Alpha codebase".to_string()],
        );

        assert!(res1.is_transition());
        let t1 = res1.transition().unwrap();
        assert_eq!(t1.transition_type, ContextTransitionType::ContextStart);
        assert_eq!(t1.to_context.as_ref().unwrap().id, "ws_a");

        // 2. Switch to Context B (Project Beta)
        let ctx_b = ContextReference::workspace("ws_b", "Project Beta");
        let res2 = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["Active app Figma for Beta UI design".to_string()],
        );

        assert!(res2.is_transition());
        let t2 = res2.transition().unwrap();
        assert_eq!(t2.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(t2.from_context.as_ref().unwrap().id, "ws_a");
        assert_eq!(t2.to_context.as_ref().unwrap().id, "ws_b");
        assert_eq!(t2.confidence, 0.88);
        assert!(t2.evidence.iter().any(|e| e.contains("Switched context from 'Project Alpha' to 'Project Beta'")));

        // Verify history contains Context A
        assert_eq!(detector.context_history().len(), 1);
        assert_eq!(detector.context_history().front().unwrap().reference.id, "ws_a");
    }

    #[test]
    fn test_scenario_2_context_a_to_inactivity() {
        let mut detector = ContextTransitionDetector::new().with_inactivity_threshold(1800);

        // Start in Context A
        let ctx_a = ContextReference::workspace("ws_a", "Project Alpha");
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Activity in Alpha".to_string()],
        );

        assert!(detector.current_context().is_some());

        // Check before threshold (15 mins) -> No inactivity
        let no_end = detector.check_inactivity("2026-09-10T10:15:00Z");
        assert!(no_end.is_none());
        assert!(detector.current_context().is_some());

        // Check after threshold (45 mins) -> ContextEnd emitted
        let end_trans = detector.check_inactivity("2026-09-10T10:45:00Z");
        assert!(end_trans.is_some());
        let t_end = end_trans.unwrap();
        assert_eq!(t_end.transition_type, ContextTransitionType::ContextEnd);
        assert_eq!(t_end.from_context.as_ref().unwrap().id, "ws_a");
        assert!(t_end.to_context.is_none());
        assert!(t_end.evidence.iter().any(|e| e.contains("Inactivity gap detected")));

        // Verify current context is cleared and moved to history
        assert!(detector.current_context().is_none());
        assert_eq!(detector.context_history().front().unwrap().reference.id, "ws_a");
    }

    #[test]
    fn test_scenario_3_inactivity_to_context_b() {
        let mut detector = ContextTransitionDetector::new();

        // 1. Initial inactivity / no active context
        assert!(detector.current_context().is_none());

        // 2. First capture in Context B after inactivity
        let ctx_b = ContextReference::workspace("ws_b", "Finance Dashboard");
        let res = detector.evaluate_interpretation(
            Some("item_10"),
            Some("sess_2"),
            "2026-09-10T11:00:00Z",
            Some(ctx_b.clone()),
            0.85,
            "High",
            false,
            vec!["Browser opened to stripe.com dashboard".to_string()],
        );

        assert!(res.is_transition());
        let t = res.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::ContextStart);
        assert!(t.from_context.is_none());
        assert_eq!(t.to_context.as_ref().unwrap().id, "ws_b");
        assert_eq!(t.confidence, 0.85);
        assert!(t.evidence.iter().any(|e| e.contains("Started activity in 'Finance Dashboard'")));
    }

    #[test]
    fn test_scenario_4_context_a_to_ambiguous() {
        let mut detector = ContextTransitionDetector::new();

        // 1. Start in Context A
        let ctx_a = ContextReference::workspace("ws_a", "Design System");
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.92,
            "Very High",
            false,
            vec!["Figma design tokens".to_string()],
        );

        // 2. Ambiguous item arrives (competing signals)
        let res = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:03:00Z",
            None,
            0.45,
            "Medium",
            true, // is_ambiguous
            vec![
                "Design System: 68% score".to_string(),
                "Marketing Site: 65% score".to_string(),
            ],
        );

        assert!(res.is_transition());
        let t = res.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::AmbiguousTransition);
        assert_eq!(t.from_context.as_ref().unwrap().id, "ws_a");
        assert!(t.to_context.is_none());
        assert!(t.evidence.iter().any(|e| e.contains("ambiguous state")));
    }

    #[test]
    fn test_scenario_5_context_a_continuation_no_spurious_switch() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Rust Engine");

        // First item starts context
        let res1 = detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Cargo build".to_string()],
        );
        assert!(res1.is_transition());

        // Second, third, fourth items in same context -> Continuations!
        for i in 2..=4 {
            let res = detector.evaluate_interpretation(
                Some(&format!("item_{}", i)),
                Some("sess_1"),
                &format!("2026-09-10T10:0{}:00Z", i),
                Some(ctx_a.clone()),
                0.92,
                "High",
                false,
                vec![format!("Code edit #{}", i)],
            );

            assert!(res.is_continuation(), "Item {} should be recognized as continuation", i);
            assert!(!res.is_transition(), "Item {} must NOT produce a false switch transition", i);

            if let TransitionEvaluationResult::Continuation { streak_count, context, .. } = res {
                assert_eq!(streak_count, i);
                assert_eq!(context.id, "ws_a");
            }
        }

        // Active context streak is now 4
        assert_eq!(detector.current_context().unwrap().streak_count, 4);
    }

    #[test]
    fn test_scenario_6_context_a_to_b_to_a_return() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Primary Coding");
        let ctx_b = ContextReference::workspace("ws_b", "Quick Slack Detour");

        // 1. Initial work in Context A
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["VS Code".to_string()],
        );

        // 2. Detour to Context B (Slack chat)
        let res_detour = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_b.clone()),
            0.80,
            "High",
            false,
            vec!["Slack message".to_string()],
        );
        assert_eq!(res_detour.transition().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // 3. Return to Context A!
        let res_return = detector.evaluate_interpretation(
            Some("item_3"),
            Some("sess_1"),
            "2026-09-10T10:08:00Z",
            Some(ctx_a.clone()),
            0.95,
            "Very High",
            false,
            vec!["Back in VS Code on Primary Coding".to_string()],
        );

        assert!(res_return.is_transition());
        let t_return = res_return.transition().unwrap();
        assert_eq!(t_return.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t_return.from_context.as_ref().unwrap().id, "ws_b");
        assert_eq!(t_return.to_context.as_ref().unwrap().id, "ws_a");
        assert!(t_return.evidence.iter().any(|e| e.contains("Returned to 'Primary Coding' after detour in 'Quick Slack Detour'")));
    }

    #[test]
    fn test_anti_jitter_weak_noise_filter() {
        let mut detector = ContextTransitionDetector::new().with_min_switch_confidence(0.40);

        let ctx_a = ContextReference::workspace("ws_a", "Active Project");
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Main project activity".to_string()],
        );

        // Low confidence noisy capture (e.g. random background ping with conf = 0.20)
        let ctx_noise = ContextReference::workspace("ws_other", "Unrelated Noise");
        let res_noise = detector.evaluate_interpretation(
            Some("item_noise"),
            Some("sess_1"),
            "2026-09-10T10:02:00Z",
            Some(ctx_noise),
            0.20, // Below 0.40 threshold
            "Low",
            false,
            vec!["Weak single domain match".to_string()],
        );

        assert!(!res_noise.is_transition(), "Low confidence noise must not trigger a transition");
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_a", "Active context must remain intact");
    }

    #[test]
    fn test_full_database_pipeline_and_membership_invariance() {
        let conn = setup_test_db();

        let ws_a = crate::db::operations::create_workspace(&conn, "Alpha Core").unwrap();
        let ws_b = crate::db::operations::create_workspace(&conn, "Beta UI").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

        // Create confirmed items in ws_a and ws_b
        let item_a1 = crate::db::operations::create_item(&conn, Some(&ws_a.id), Some(&session.id), "text", "fn alpha_core() {}", None, "{}").unwrap();
        let item_a2 = crate::db::operations::create_item(&conn, Some(&ws_a.id), Some(&session.id), "text", "struct AlphaState {}", None, "{}").unwrap();
        let item_b1 = crate::db::operations::create_item(&conn, Some(&ws_b.id), Some(&session.id), "url", "https://beta-ui.local", None, "{}").unwrap();
        let item_a3 = crate::db::operations::create_item(&conn, Some(&ws_a.id), Some(&session.id), "text", "Alpha return work", None, "{}").unwrap();

        let mut detector = ContextTransitionDetector::new();

        // 1. Process item A1 -> ContextStart (Alpha Core)
        let res1 = detector.process_item(&conn, &item_a1.id).unwrap();
        assert_eq!(res1.transition().unwrap().transition_type, ContextTransitionType::ContextStart);

        // 2. Process item A2 -> Continuation
        let res2 = detector.process_item(&conn, &item_a2.id).unwrap();
        assert!(res2.is_continuation());

        // 3. Process item B1 -> ContextSwitch (Alpha Core -> Beta UI)
        let res3 = detector.process_item(&conn, &item_b1.id).unwrap();
        assert_eq!(res3.transition().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // 4. Process item A3 -> ContextReturn (Beta UI -> Alpha Core)
        let res4 = detector.process_item(&conn, &item_a3.id).unwrap();
        assert_eq!(res4.transition().unwrap().transition_type, ContextTransitionType::ContextReturn);

        // Record the transition to DB
        let record = record_context_transition(&conn, &res4.transition().unwrap().to_context_transition()).unwrap();
        assert_eq!(record.transition_type, ContextTransitionType::ContextReturn);

        // STRICT INVARIANT TEST: Verify item memberships are NEVER mutated
        let fresh_a1 = crate::db::operations::get_item(&conn, &item_a1.id).unwrap();
        let fresh_a2 = crate::db::operations::get_item(&conn, &item_a2.id).unwrap();
        let fresh_b1 = crate::db::operations::get_item(&conn, &item_b1.id).unwrap();
        let fresh_a3 = crate::db::operations::get_item(&conn, &item_a3.id).unwrap();

        assert_eq!(fresh_a1.workspace_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(fresh_a2.workspace_id.as_deref(), Some(ws_a.id.as_str()));
        assert_eq!(fresh_b1.workspace_id.as_deref(), Some(ws_b.id.as_str()));
        assert_eq!(fresh_a3.workspace_id.as_deref(), Some(ws_a.id.as_str()));
    }

    #[test]
    fn test_detect_sequence() {
        let conn = setup_test_db();
        let ws1 = crate::db::operations::create_workspace(&conn, "Doc Review").unwrap();
        let ws2 = crate::db::operations::create_workspace(&conn, "Dev Ops").unwrap();

        let i1 = crate::db::operations::create_item(&conn, Some(&ws1.id), None, "text", "doc 1", None, "{}").unwrap();
        let i2 = crate::db::operations::create_item(&conn, Some(&ws1.id), None, "text", "doc 2", None, "{}").unwrap();
        let i3 = crate::db::operations::create_item(&conn, Some(&ws2.id), None, "text", "k8s deploy", None, "{}").unwrap();

        let mut detector = ContextTransitionDetector::new();
        let sequence = vec![i1.id.clone(), i2.id.clone(), i3.id.clone()];
        let transitions = detector.detect_sequence(&conn, &sequence).unwrap();

        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions[0].transition_type, ContextTransitionType::ContextStart);
        assert_eq!(transitions[1].transition_type, ContextTransitionType::ContextSwitch);
    }
}
