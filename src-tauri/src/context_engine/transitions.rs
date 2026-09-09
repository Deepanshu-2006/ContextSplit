pub use crate::db::models::{ContextInterpretationKind, ContextTransition, ContextTransitionType};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

/// Helper to format elapsed seconds into a human-readable duration string.
pub fn format_duration_human(seconds: i64) -> String {
    let s = seconds.abs();
    if s < 60 {
        format!("{} seconds", s)
    } else if s < 3600 {
        let mins = s / 60;
        let rem_s = s % 60;
        if rem_s > 0 && mins < 5 {
            format!("{}m {}s", mins, rem_s)
        } else {
            format!("{} minutes", mins)
        }
    } else if s < 86400 {
        let hours = s as f64 / 3600.0;
        if hours.fract() < 0.05 || hours.fract() > 0.95 {
            format!("{:.0} hours", hours.round())
        } else {
            format!("{:.1} hours", hours)
        }
    } else {
        let days = s as f64 / 86400.0;
        if days.fract() < 0.05 || days.fract() > 0.95 {
            format!("{:.0} days", days.round())
        } else {
            format!("{:.1} days", days)
        }
    }
}

/// Configuration for contextual transition stability, hysteresis, and return horizons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionStabilityConfig {
    /// Minimum confidence threshold for considering any transition candidate (default: 0.40)
    pub confidence_threshold: f64,
    /// Minimum evidence / score strength required (default: 0.35)
    pub min_evidence_strength: f64,
    /// Minimum consecutive observations required to confirm a moderate transition (default: 2)
    pub min_consecutive_evidence: usize,
    /// Minimum temporal persistence span in seconds for moderate changes (default: 0)
    pub min_temporal_persistence_secs: i64,
    /// Meaningful score margin/difference required over competitors (default: 0.15)
    pub meaningful_score_difference: f64,
    /// Minimum number of agreeing evidence categories/items (default: 2)
    pub min_evidence_agreement_count: usize,
    /// Confidence threshold at which a transition is fast-tracked immediately on item 1 (default: 0.85)
    pub fast_track_confidence: f64,
    /// Score threshold at which a high-scoring transition is fast-tracked (default: 0.80)
    pub fast_track_score: f64,
    /// Maximum elapsed time since previous activity to be considered a short-term return (default: 86400s / 24h)
    pub max_return_gap_seconds: i64,
    /// Minimum confidence required to confirm a return transition (default: 0.50)
    pub min_return_confidence: f64,
}

impl Default for TransitionStabilityConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.40,
            min_evidence_strength: 0.35,
            min_consecutive_evidence: 2,
            min_temporal_persistence_secs: 0,
            meaningful_score_difference: 0.15,
            min_evidence_agreement_count: 2,
            fast_track_confidence: 0.85,
            fast_track_score: 0.80,
            max_return_gap_seconds: 86400, // 24 hours
            min_return_confidence: 0.50,
        }
    }
}

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

/// Provisional context transition under observation awaiting sustained evidence confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingTransition {
    pub target_context: ContextReference,
    pub confidence: f64,
    pub confidence_band: String,
    pub score: f64,
    pub score_margin: f64,
    pub evidence: Vec<String>,
    pub evidence_agreement_count: usize,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub observation_count: usize,
    pub initial_trigger_item_id: Option<String>,
    pub session_id: Option<String>,
}

/// Lightweight marker of the most recently emitted transition to prevent duplicates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmittedTransitionMarker {
    pub from_context_id: Option<String>,
    pub to_context_id: Option<String>,
    pub transition_type: ContextTransitionType,
    pub emitted_at: String,
}

/// Structured details specific to a context return detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextReturnDetails {
    pub returned_context: ContextReference,
    pub previous_context: Option<ContextReference>,
    pub return_time: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
    pub duration_since_previous_activity_seconds: i64,
    pub duration_since_previous_activity_formatted: String,
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
    pub duration_since_previous_activity_seconds: Option<i64>,
    pub metadata: Value,
}

impl DetectedTransition {
    /// Extracts structured context return details if this transition represents a return.
    pub fn as_context_return(&self) -> Option<ContextReturnDetails> {
        if self.transition_type != ContextTransitionType::ContextReturn {
            return None;
        }
        let returned = self.to_context.clone()?;
        let duration_secs = self.duration_since_previous_activity_seconds.unwrap_or(0);
        let duration_formatted = format_duration_human(duration_secs);

        Some(ContextReturnDetails {
            returned_context: returned,
            previous_context: self.from_context.clone(),
            return_time: self.timestamp.clone(),
            confidence: self.confidence,
            evidence: self.evidence.clone(),
            duration_since_previous_activity_seconds: duration_secs,
            duration_since_previous_activity_formatted: duration_formatted,
        })
    }

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
            if let Some(dur) = self.duration_since_previous_activity_seconds {
                obj.insert(
                    "duration_since_previous_activity_seconds".to_string(),
                    serde_json::Value::Number(dur.into()),
                );
                obj.insert(
                    "duration_since_previous_activity_formatted".to_string(),
                    serde_json::Value::String(format_duration_human(dur)),
                );
            }
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
    /// Meaningful contextual transition confirmed and detected (Switch, Return, Start, End, Ambiguous).
    Transition(DetectedTransition),
    /// Activity continued within the current context without a state transition (Context A -> Context A continuation).
    Continuation {
        context: ContextReference,
        streak_count: usize,
        confidence: f64,
        timestamp: String,
        evidence: Vec<String>,
    },
    /// Provisional transition observation held in pending state awaiting sustained evidence.
    Pending {
        target: ContextReference,
        observation_count: usize,
        required_count: usize,
        confidence: f64,
        timestamp: String,
        reason: String,
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

    pub fn is_pending(&self) -> bool {
        matches!(self, TransitionEvaluationResult::Pending { .. })
    }
}

/// State-aware detector for meaningful changes between contextual states with hysteresis and stability.
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
    pub history_depth: usize,
    pub stability_config: TransitionStabilityConfig,
    pub current_context: Option<ContextSnapshot>,
    pub context_history: VecDeque<ContextSnapshot>,
    pub pending_transition: Option<PendingTransition>,
    pub last_emitted_transition: Option<EmittedTransitionMarker>,
    pub last_activity_at: Option<String>,
    pub active_session_id: Option<String>,
}

impl ContextTransitionDetector {
    pub const DEFAULT_INACTIVITY_THRESHOLD_SECS: i64 = 1800; // 30 minutes
    pub const DEFAULT_HISTORY_DEPTH: usize = 5;

    pub fn new() -> Self {
        Self {
            inactivity_threshold_secs: Self::DEFAULT_INACTIVITY_THRESHOLD_SECS,
            history_depth: Self::DEFAULT_HISTORY_DEPTH,
            stability_config: TransitionStabilityConfig::default(),
            current_context: None,
            context_history: VecDeque::new(),
            pending_transition: None,
            last_emitted_transition: None,
            last_activity_at: None,
            active_session_id: None,
        }
    }

    pub fn with_inactivity_threshold(mut self, secs: i64) -> Self {
        self.inactivity_threshold_secs = secs.max(1);
        self
    }

    pub fn with_min_switch_confidence(mut self, conf: f64) -> Self {
        self.stability_config.confidence_threshold = conf.clamp(0.0, 1.0);
        self
    }

    pub fn with_history_depth(mut self, depth: usize) -> Self {
        self.history_depth = depth.max(1);
        self
    }

    pub fn with_stability_config(mut self, config: TransitionStabilityConfig) -> Self {
        self.stability_config = config;
        self
    }

    pub fn with_min_consecutive_evidence(mut self, count: usize) -> Self {
        self.stability_config.min_consecutive_evidence = count.max(1);
        self
    }

    pub fn with_min_evidence_strength(mut self, strength: f64) -> Self {
        self.stability_config.min_evidence_strength = strength.clamp(0.0, 1.0);
        self
    }

    pub fn with_meaningful_score_difference(mut self, diff: f64) -> Self {
        self.stability_config.meaningful_score_difference = diff.clamp(0.0, 1.0);
        self
    }

    pub fn with_evidence_agreement_count(mut self, count: usize) -> Self {
        self.stability_config.min_evidence_agreement_count = count.max(1);
        self
    }

    pub fn with_fast_track_confidence(mut self, conf: f64) -> Self {
        self.stability_config.fast_track_confidence = conf.clamp(0.0, 1.0);
        self
    }

    pub fn with_max_return_gap_seconds(mut self, gap_secs: i64) -> Self {
        self.stability_config.max_return_gap_seconds = gap_secs.max(1);
        self
    }

    pub fn with_min_return_confidence(mut self, conf: f64) -> Self {
        self.stability_config.min_return_confidence = conf.clamp(0.0, 1.0);
        self
    }

    pub fn current_context(&self) -> Option<&ContextSnapshot> {
        self.current_context.as_ref()
    }

    pub fn context_history(&self) -> &VecDeque<ContextSnapshot> {
        &self.context_history
    }

    pub fn pending_transition(&self) -> Option<&PendingTransition> {
        self.pending_transition.as_ref()
    }

    pub fn reset(&mut self) {
        self.current_context = None;
        self.context_history.clear();
        self.pending_transition = None;
        self.last_emitted_transition = None;
        self.last_activity_at = None;
        self.active_session_id = None;
    }

    /// Pushes a snapshot to the bounded context history, updating previous entry if matching.
    fn push_history(&mut self, snapshot: ContextSnapshot) {
        // If snapshot matches the top of history, update its last_seen_at and streak
        if let Some(front) = self.context_history.front_mut() {
            if front.reference.id == snapshot.reference.id {
                front.last_seen_at = snapshot.last_seen_at;
                front.streak_count = snapshot.streak_count;
                return;
            }
        }
        self.context_history.push_front(snapshot);
        while self.context_history.len() > self.history_depth {
            self.context_history.pop_back();
        }
    }

    /// Checks if a transition should be emitted or suppressed as a duplicate.
    fn record_and_check_duplicate(
        &mut self,
        from_id: Option<&str>,
        to_id: Option<&str>,
        transition_type: ContextTransitionType,
        timestamp: &str,
    ) -> bool {
        if let Some(last) = &self.last_emitted_transition {
            if last.from_context_id.as_deref() == from_id
                && last.to_context_id.as_deref() == to_id
                && last.transition_type == transition_type
            {
                return false; // Duplicate
            }
        }
        self.last_emitted_transition = Some(EmittedTransitionMarker {
            from_context_id: from_id.map(|s| s.to_string()),
            to_context_id: to_id.map(|s| s.to_string()),
            transition_type,
            emitted_at: timestamp.to_string(),
        });
        true
    }

    /// Determines whether a context candidate qualifies for immediate fast-tracking without waiting for sustained evidence.
    pub fn is_fast_track(
        &self,
        is_user_confirmed: bool,
        confidence: f64,
        score: f64,
        score_margin: f64,
        evidence_agreement_count: usize,
    ) -> bool {
        if is_user_confirmed {
            return true;
        }
        if confidence >= self.stability_config.fast_track_confidence {
            return true;
        }
        if score >= self.stability_config.fast_track_score
            && score_margin >= self.stability_config.meaningful_score_difference
            && evidence_agreement_count >= self.stability_config.min_evidence_agreement_count
        {
            return true;
        }
        false
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
            self.pending_transition = None;

            if !self.record_and_check_duplicate(
                Some(&ref_clone.id),
                None,
                ContextTransitionType::ContextEnd,
                current_time_iso,
            ) {
                return None;
            }

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
                duration_since_previous_activity_seconds: None,
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

    /// Simplified evaluation method for backward compatibility and testing.
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
        let is_user_confirmed = confidence >= 0.99;
        let score = confidence;
        let score_margin = 0.20;
        let agreement = evidence.len();

        self.evaluate_interpretation_detailed(
            item_id,
            session_id,
            item_timestamp,
            target_context,
            confidence,
            confidence_band,
            score,
            score_margin,
            agreement,
            is_user_confirmed,
            is_ambiguous,
            evidence,
        )
    }

    /// Detailed transition evaluation with stability, hysteresis, return detection, and fast-tracking.
    pub fn evaluate_interpretation_detailed(
        &mut self,
        item_id: Option<&str>,
        session_id: Option<&str>,
        item_timestamp: &str,
        target_context: Option<ContextReference>,
        confidence: f64,
        confidence_band: &str,
        score: f64,
        score_margin: f64,
        evidence_agreement_count: usize,
        is_user_confirmed: bool,
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
            self.pending_transition = None;
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

                if !self.record_and_check_duplicate(
                    Some(&from_ref.id),
                    None,
                    ContextTransitionType::AmbiguousTransition,
                    item_timestamp,
                ) {
                    return TransitionEvaluationResult::NoTransition {
                        reason: "Duplicate ambiguous transition suppressed".to_string(),
                        timestamp: item_timestamp.to_string(),
                    };
                }

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
                    duration_since_previous_activity_seconds: None,
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
                // Weak evidence or no context (anti-jitter)
                return TransitionEvaluationResult::NoTransition {
                    reason: "Insufficient contextual evidence to infer a state transition"
                        .to_string(),
                    timestamp: item_timestamp.to_string(),
                };
            }
        };

        // Anti-jitter: Check minimum confidence threshold
        if confidence < self.stability_config.confidence_threshold && !is_user_confirmed {
            return TransitionEvaluationResult::NoTransition {
                reason: format!(
                    "Context candidate '{}' confidence ({:.0}%) below minimum threshold ({:.0}%)",
                    target.name,
                    confidence * 100.0,
                    self.stability_config.confidence_threshold * 100.0
                ),
                timestamp: item_timestamp.to_string(),
            };
        }

        // Scenario 5: Continued activity in existing active context
        if let Some(curr) = &mut self.current_context {
            if curr.reference.id == target.id {
                // Active context matched! Discard any temporary pending detour
                self.pending_transition = None;

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

                return TransitionEvaluationResult::Continuation {
                    context: curr.reference.clone(),
                    streak_count: curr.streak_count,
                    confidence,
                    timestamp: item_timestamp.to_string(),
                    evidence: cont_evidence,
                };
            }
        }

        // Fast-Track Evaluation: does this candidate qualify for immediate transition?
        let fast_track = self.is_fast_track(
            is_user_confirmed,
            confidence,
            score,
            score_margin,
            evidence_agreement_count,
        );

        // Case A: Fast-tracked context change
        if fast_track || self.stability_config.min_consecutive_evidence <= 1 {
            self.pending_transition = None;
            return self.confirm_transition(
                item_id,
                session_id,
                item_timestamp,
                target,
                confidence,
                confidence_band,
                evidence,
                is_resumed_after_inactivity,
                "Fast-tracked via strong explicit evidence",
            );
        }

        // Case B: Moderate unconfirmed change requiring sustained evidence (Hysteresis Buffer)
        if let Some(ref mut pending) = self.pending_transition {
            if pending.target_context.id == target.id {
                // Sustained observation of the same pending target!
                pending.observation_count += 1;
                pending.last_observed_at = item_timestamp.to_string();
                pending.confidence = (pending.confidence + confidence) / 2.0;

                let elapsed_temporal = item_time_dt
                    .signed_duration_since(
                        pending
                            .first_observed_at
                            .parse::<DateTime<Utc>>()
                            .unwrap_or_else(|_| Utc::now()),
                    )
                    .num_seconds();

                let is_sustained = pending.observation_count
                    >= self.stability_config.min_consecutive_evidence
                    || (self.stability_config.min_temporal_persistence_secs > 0
                        && elapsed_temporal
                            >= self.stability_config.min_temporal_persistence_secs);

                if is_sustained {
                    // Sustained threshold reached! Confirm transition!
                    let final_target = pending.target_context.clone();
                    let final_conf = pending.confidence;
                    let final_band = pending.confidence_band.clone();
                    let count = pending.observation_count;
                    self.pending_transition = None;

                    let mut sustained_evidence = vec![format!(
                        "Confirmed contextual transition to '{}' after {} sustained observations (avg confidence {:.0}%)",
                        final_target.name, count, final_conf * 100.0
                    )];
                    sustained_evidence.extend(evidence);

                    return self.confirm_transition(
                        item_id,
                        session_id,
                        item_timestamp,
                        final_target,
                        final_conf,
                        &final_band,
                        sustained_evidence,
                        is_resumed_after_inactivity,
                        "Confirmed after sustained evidence",
                    );
                } else {
                    // Still pending confirmation
                    self.last_activity_at = Some(item_timestamp.to_string());
                    return TransitionEvaluationResult::Pending {
                        target: target.clone(),
                        observation_count: pending.observation_count,
                        required_count: self.stability_config.min_consecutive_evidence,
                        confidence,
                        timestamp: item_timestamp.to_string(),
                        reason: format!(
                            "Observed {}/{} sustained signals for '{}'",
                            pending.observation_count,
                            self.stability_config.min_consecutive_evidence,
                            target.name
                        ),
                    };
                }
            }
        }

        // New pending candidate observation (observation count = 1)
        self.pending_transition = Some(PendingTransition {
            target_context: target.clone(),
            confidence,
            confidence_band: confidence_band.to_string(),
            score,
            score_margin,
            evidence: evidence.clone(),
            evidence_agreement_count,
            first_observed_at: item_timestamp.to_string(),
            last_observed_at: item_timestamp.to_string(),
            observation_count: 1,
            initial_trigger_item_id: item_id.map(|s| s.to_string()),
            session_id: session_id.map(|s| s.to_string()),
        });

        self.last_activity_at = Some(item_timestamp.to_string());
        TransitionEvaluationResult::Pending {
            target: target.clone(),
            observation_count: 1,
            required_count: self.stability_config.min_consecutive_evidence,
            confidence,
            timestamp: item_timestamp.to_string(),
            reason: format!(
                "First observation for candidate '{}' ({:.0}% confidence) held in pending state awaiting sustained evidence",
                target.name,
                confidence * 100.0
            ),
        }
    }

    /// Internal helper to execute, calculate return duration, and record a confirmed transition.
    fn confirm_transition(
        &mut self,
        item_id: Option<&str>,
        session_id: Option<&str>,
        item_timestamp: &str,
        target: ContextReference,
        confidence: f64,
        confidence_band: &str,
        evidence: Vec<String>,
        is_resumed_after_inactivity: bool,
        confirmation_note: &str,
    ) -> TransitionEvaluationResult {
        let item_time_dt = item_timestamp
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|_| Utc::now());

        match &mut self.current_context {
            Some(curr) => {
                let from_ref = curr.reference.clone();

                // Check if target exists in history for Context Return detection
                let history_match = self
                    .context_history
                    .iter()
                    .find(|h| h.reference.id == target.id)
                    .cloned();

                let (trans_type, trans_reason, duration_opt) = if let Some(hist) = history_match {
                    let last_seen_dt = hist
                        .last_seen_at
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| item_time_dt);
                    let duration_secs = item_time_dt
                        .signed_duration_since(last_seen_dt)
                        .num_seconds()
                        .max(0);

                    let is_within_return_horizon =
                        duration_secs <= self.stability_config.max_return_gap_seconds;
                    let has_return_confidence =
                        confidence >= self.stability_config.min_return_confidence;

                    if is_within_return_horizon && has_return_confidence {
                        let dur_formatted = format_duration_human(duration_secs);
                        (
                            ContextTransitionType::ContextReturn,
                            format!(
                                "Returned to '{}' after detour in '{}' (duration since previous activity: {})",
                                target.name, from_ref.name, dur_formatted
                            ),
                            Some(duration_secs),
                        )
                    } else {
                        (
                            ContextTransitionType::ContextSwitch,
                            format!(
                                "Switched context from '{}' to '{}'",
                                from_ref.name, target.name
                            ),
                            None,
                        )
                    }
                } else {
                    (
                        ContextTransitionType::ContextSwitch,
                        format!(
                            "Switched context from '{}' to '{}'",
                            from_ref.name, target.name
                        ),
                        None,
                    )
                };

                if !self.record_and_check_duplicate(
                    Some(&from_ref.id),
                    Some(&target.id),
                    trans_type,
                    item_timestamp,
                ) {
                    return TransitionEvaluationResult::NoTransition {
                        reason: "Duplicate transition suppressed".to_string(),
                        timestamp: item_timestamp.to_string(),
                    };
                }

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
                    duration_since_previous_activity_seconds: duration_opt,
                    metadata: serde_json::json!({
                        "confirmation_type": confirmation_note,
                        "duration_since_previous_activity_seconds": duration_opt,
                    }),
                };

                TransitionEvaluationResult::Transition(transition)
            }
            None => {
                // Inactivity -> Context B (ContextStart)
                if !self.record_and_check_duplicate(
                    None,
                    Some(&target.id),
                    ContextTransitionType::ContextStart,
                    item_timestamp,
                ) {
                    return TransitionEvaluationResult::NoTransition {
                        reason: "Duplicate start transition suppressed".to_string(),
                        timestamp: item_timestamp.to_string(),
                    };
                }

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
                    duration_since_previous_activity_seconds: None,
                    metadata: serde_json::json!({
                        "resumed_after_inactivity": is_resumed_after_inactivity,
                        "confirmation_type": confirmation_note,
                    }),
                };

                TransitionEvaluationResult::Transition(transition)
            }
        }
    }

    /// Evaluates an item by ID from the database against the current contextual state with full stability checks.
    ///
    /// Respects user-confirmed workspace memberships as authoritative ground-truth,
    /// falls back to Context Engine interpretation scoring, and applies hysteresis to prevent spurious transitions.
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
            return Ok(self.evaluate_interpretation_detailed(
                Some(&item.id),
                item.session_id.as_deref(),
                &item.created_at,
                Some(target_ref),
                1.0,
                "Very High",
                1.0,
                1.0,
                3,
                true, // is_user_confirmed -> fast track
                false,
                evidence,
            ));
        }

        // 2. Otherwise, evaluate through the Context Engine scoring pipeline
        let interp = crate::context_engine::scoring::evaluate_item_context(conn, item_id)?;

        let is_ambiguous = interp.interpretation_kind
            == Some(crate::db::models::ContextInterpretationKind::Ambiguous);

        let (target_ref, conf, band, score, margin, agreement, evidence) =
            if let Some(ws) = interp.existing_workspaces.first() {
                let s = ws.score;
                let m = if interp.existing_workspaces.len() > 1 {
                    (ws.score - interp.existing_workspaces[1].score).abs()
                } else {
                    ws.score
                };
                let agree = ws.reasons.len();

                if ws.confidence >= self.stability_config.confidence_threshold
                    && ws.score >= self.stability_config.min_evidence_strength
                {
                    (
                        Some(ContextReference::workspace(&ws.workspace_id, &ws.workspace_name)),
                        ws.confidence,
                        ws.confidence_band.as_str(),
                        s,
                        m,
                        agree,
                        ws.reasons.clone(),
                    )
                } else {
                    (
                        None,
                        ws.confidence,
                        ws.confidence_band.as_str(),
                        s,
                        m,
                        agree,
                        vec![interp.summary.clone()],
                    )
                }
            } else if let Some(cand) = interp.potential_new_contexts.first() {
                if cand.confidence >= self.stability_config.confidence_threshold {
                    (
                        Some(ContextReference::candidate(
                            cand.candidate_id.as_deref().unwrap_or("new_candidate"),
                            &cand.label,
                        )),
                        cand.confidence,
                        cand.confidence_band.as_str(),
                        cand.confidence,
                        0.20,
                        cand.reasons.len(),
                        cand.reasons.clone(),
                    )
                } else {
                    (
                        None,
                        cand.confidence,
                        cand.confidence_band.as_str(),
                        cand.confidence,
                        0.0,
                        1,
                        vec![interp.summary.clone()],
                    )
                }
            } else {
                (None, 0.0, "Very Low", 0.0, 0.0, 0, vec![interp.summary.clone()])
            };

        Ok(self.evaluate_interpretation_detailed(
            Some(&item.id),
            item.session_id.as_deref(),
            &item.created_at,
            target_ref,
            conf,
            band,
            score,
            margin,
            agreement,
            false,
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
    fn test_immediate_return() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Design Tokens");
        let ctx_b = ContextReference::workspace("ws_b", "Quick Slack Chat");

        // 1. Initial activity in Context A
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Figma design tokens".to_string()],
        );

        // 2. Detour to Context B for 2 minutes (fast-tracked)
        let res_b = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:02:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["Slack channel message".to_string()],
        );
        assert_eq!(res_b.transition().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // 3. Immediate return to Context A at 10:04:00 (120s duration)
        let res_a = detector.evaluate_interpretation(
            Some("item_3"),
            Some("sess_1"),
            "2026-09-10T10:04:00Z",
            Some(ctx_a.clone()),
            0.92,
            "Very High",
            false,
            vec!["Back in Figma editing tokens".to_string()],
        );

        assert!(res_a.is_transition());
        let t = res_a.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t.duration_since_previous_activity_seconds, Some(240));

        let return_details = t.as_context_return().unwrap();
        assert_eq!(return_details.returned_context.id, "ws_a");
        assert_eq!(return_details.previous_context.as_ref().unwrap().id, "ws_b");
        assert_eq!(return_details.return_time, "2026-09-10T10:04:00Z");
        assert_eq!(return_details.confidence, 0.92);
        assert!(return_details.evidence.iter().any(|e| e.contains("Returned to 'Design Tokens' after detour in 'Quick Slack Chat'")));
    }

    #[test]
    fn test_long_gap_return_within_and_beyond_horizon() {
        let mut detector = ContextTransitionDetector::new()
            .with_inactivity_threshold(20000)
            .with_max_return_gap_seconds(7200); // 2 hours max return horizon

        let ctx_a = ContextReference::workspace("ws_a", "API Backend");
        let ctx_b = ContextReference::workspace("ws_b", "Database Ops");

        // 1. Initial activity in A (10:00:00)
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["API work".to_string()],
        );

        // 2. Switch to B (10:10:00)
        detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:10:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["DB migrations".to_string()],
        );

        // Case A: Return at 11:30:00 (5400s elapsed since 10:00:00 -> within 7200s horizon)
        let res_within = detector.evaluate_interpretation(
            Some("item_3"),
            Some("sess_1"),
            "2026-09-10T11:30:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Resuming API work".to_string()],
        );

        let t_within = res_within.transition().unwrap();
        assert_eq!(t_within.transition_type, ContextTransitionType::ContextReturn);
        assert_eq!(t_within.duration_since_previous_activity_seconds, Some(5400));
        let ret = t_within.as_context_return().unwrap();
        assert_eq!(ret.duration_since_previous_activity_formatted, "1.5 hours");

        // Switch to B again (11:40:00)
        detector.evaluate_interpretation(
            Some("item_4"),
            Some("sess_1"),
            "2026-09-10T11:40:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["DB indexing".to_string()],
        );

        // Case B: Return at 15:00:00 (12600s elapsed since 11:30:00 -> EXCEEDS 7200s horizon)
        let res_beyond = detector.evaluate_interpretation(
            Some("item_5"),
            Some("sess_1"),
            "2026-09-10T15:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Fresh API sprint".to_string()],
        );

        let t_beyond = res_beyond.transition().unwrap();
        // Because horizon was exceeded, classified as standard ContextSwitch rather than short task return
        assert_eq!(t_beyond.transition_type, ContextTransitionType::ContextSwitch);
        assert!(t_beyond.duration_since_previous_activity_seconds.is_none());
    }

    #[test]
    fn test_weak_evidence_does_not_trigger_return() {
        let mut detector = ContextTransitionDetector::new()
            .with_min_return_confidence(0.50);

        let ctx_a = ContextReference::workspace("ws_a", "Primary Project");
        let ctx_b = ContextReference::workspace("ws_b", "Secondary Project");

        // 1. Establish in A
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Primary".to_string()],
        );

        // 2. Switch to B
        detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["Secondary".to_string()],
        );

        // 3. Weak signal for A (confidence = 0.35, below min_return_confidence 0.50 and threshold 0.40)
        let res_weak = detector.evaluate_interpretation(
            Some("item_3"),
            Some("sess_1"),
            "2026-09-10T10:08:00Z",
            Some(ctx_a.clone()),
            0.35, // Weak
            "Low",
            false,
            vec!["Vague peripheral match".to_string()],
        );

        assert!(!res_weak.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_b");
    }

    #[test]
    fn test_ambiguous_return_produces_ambiguous_transition() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Work A");
        let ctx_b = ContextReference::workspace("ws_b", "Work B");

        // 1. Establish in A
        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["A".to_string()],
        );

        // 2. Switch to B
        detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["B".to_string()],
        );

        // 3. Ambiguous item arrives competing between A and C
        let res_amb = detector.evaluate_interpretation(
            Some("item_3"),
            Some("sess_1"),
            "2026-09-10T10:10:00Z",
            None,
            0.45,
            "Medium",
            true, // is_ambiguous
            vec!["Work A: 60%".to_string(), "Work C: 58%".to_string()],
        );

        assert!(res_amb.is_transition());
        let t = res_amb.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::AmbiguousTransition);
        assert!(t.as_context_return().is_none());
    }

    #[test]
    fn test_multiple_alternating_contexts_a_b_a_b_a() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Frontend");
        let ctx_b = ContextReference::workspace("ws_b", "Backend");

        // 1. Start Frontend (10:00:00)
        let t1 = detector.evaluate_interpretation(
            Some("i1"),
            Some("s1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["React component".to_string()],
        );
        assert_eq!(t1.transition().unwrap().transition_type, ContextTransitionType::ContextStart);

        // 2. Switch to Backend (10:10:00)
        let t2 = detector.evaluate_interpretation(
            Some("i2"),
            Some("s1"),
            "2026-09-10T10:10:00Z",
            Some(ctx_b.clone()),
            0.90,
            "High",
            false,
            vec!["Rust handler".to_string()],
        );
        assert_eq!(t2.transition().unwrap().transition_type, ContextTransitionType::ContextSwitch);

        // 3. Return to Frontend (10:20:00, 1200s since 10:00:00)
        let t3 = detector.evaluate_interpretation(
            Some("i3"),
            Some("s1"),
            "2026-09-10T10:20:00Z",
            Some(ctx_a.clone()),
            0.92,
            "Very High",
            false,
            vec!["Frontend wiring".to_string()],
        );
        let ret3 = t3.transition().unwrap().as_context_return().unwrap();
        assert_eq!(ret3.returned_context.id, "ws_a");
        assert_eq!(ret3.previous_context.unwrap().id, "ws_b");
        assert_eq!(ret3.duration_since_previous_activity_seconds, 1200);
        assert_eq!(ret3.duration_since_previous_activity_formatted, "20 minutes");

        // 4. Return to Backend (10:35:00, 1500s since 10:10:00)
        let t4 = detector.evaluate_interpretation(
            Some("i4"),
            Some("s1"),
            "2026-09-10T10:35:00Z",
            Some(ctx_b.clone()),
            0.89,
            "High",
            false,
            vec!["Backend query tuning".to_string()],
        );
        let ret4 = t4.transition().unwrap().as_context_return().unwrap();
        assert_eq!(ret4.returned_context.id, "ws_b");
        assert_eq!(ret4.previous_context.unwrap().id, "ws_a");
        assert_eq!(ret4.duration_since_previous_activity_seconds, 1500);
        assert_eq!(ret4.duration_since_previous_activity_formatted, "25 minutes");

        // 5. Return to Frontend again (10:50:00, 1800s since 10:20:00)
        let t5 = detector.evaluate_interpretation(
            Some("i5"),
            Some("s1"),
            "2026-09-10T10:50:00Z",
            Some(ctx_a.clone()),
            0.93,
            "Very High",
            false,
            vec!["Frontend UI polish".to_string()],
        );
        let ret5 = t5.transition().unwrap().as_context_return().unwrap();
        assert_eq!(ret5.returned_context.id, "ws_a");
        assert_eq!(ret5.previous_context.unwrap().id, "ws_b");
        assert_eq!(ret5.duration_since_previous_activity_seconds, 1800);
        assert_eq!(ret5.duration_since_previous_activity_formatted, "30 minutes");
    }

    #[test]
    fn test_prompt_example_1_portfolio_one_unrelated_item_portfolio() {
        let mut detector = ContextTransitionDetector::new().with_min_consecutive_evidence(2);

        let ctx_portfolio = ContextReference::workspace("ws_portfolio", "Portfolio");
        let ctx_college = ContextReference::workspace("ws_college", "College");

        // 1. Establish in Portfolio
        let res1 = detector.evaluate_interpretation(
            Some("item_p1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_portfolio.clone()),
            0.90,
            "High",
            false,
            vec!["Portfolio design".to_string()],
        );
        assert!(res1.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_portfolio");

        // 2. ONE unrelated item with moderate evidence arrives (College)
        let res2 = detector.evaluate_interpretation(
            Some("item_unrelated"),
            Some("sess_1"),
            "2026-09-10T10:03:00Z",
            Some(ctx_college.clone()),
            0.60,
            "Medium",
            false,
            vec!["Single college lecture note".to_string()],
        );

        assert!(res2.is_pending());
        assert!(!res2.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_portfolio");

        // 3. User continues in Portfolio!
        let res3 = detector.evaluate_interpretation(
            Some("item_p2"),
            Some("sess_1"),
            "2026-09-10T10:06:00Z",
            Some(ctx_portfolio.clone()),
            0.88,
            "High",
            false,
            vec!["Portfolio showcase edit".to_string()],
        );

        assert!(res3.is_continuation());
        assert!(!res3.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_portfolio");
        assert!(detector.pending_transition().is_none());
    }

    #[test]
    fn test_prompt_example_2_portfolio_several_strong_college_signals_college() {
        let mut detector = ContextTransitionDetector::new().with_min_consecutive_evidence(2);

        let ctx_portfolio = ContextReference::workspace("ws_portfolio", "Portfolio");
        let ctx_college = ContextReference::workspace("ws_college", "College");

        // 1. Establish in Portfolio
        detector.evaluate_interpretation(
            Some("item_p1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_portfolio.clone()),
            0.90,
            "High",
            false,
            vec!["Portfolio homepage".to_string()],
        );
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_portfolio");

        // 2. First strong College signal (moderate/strong, conf: 0.70)
        let res2 = detector.evaluate_interpretation(
            Some("item_c1"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_college.clone()),
            0.70,
            "High",
            false,
            vec!["Assignment 3 submission portal".to_string()],
        );
        assert!(res2.is_pending());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_portfolio");

        // 3. Second strong College signal -> Sustained confirmation!
        let res3 = detector.evaluate_interpretation(
            Some("item_c2"),
            Some("sess_1"),
            "2026-09-10T10:08:00Z",
            Some(ctx_college.clone()),
            0.75,
            "High",
            false,
            vec!["Course syllabus pdf".to_string()],
        );

        assert!(res3.is_transition(), "Second sustained signal must confirm the transition");
        let t = res3.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::ContextSwitch);
        assert_eq!(t.from_context.as_ref().unwrap().id, "ws_portfolio");
        assert_eq!(t.to_context.as_ref().unwrap().id, "ws_college");
        assert!(t.evidence.iter().any(|e| e.contains("Confirmed contextual transition to 'College' after 2 sustained observations")));

        assert_eq!(detector.current_context().unwrap().reference.id, "ws_college");
    }

    #[test]
    fn test_fast_track_explicit_context_change() {
        let mut detector = ContextTransitionDetector::new().with_min_consecutive_evidence(3);

        let ctx_a = ContextReference::workspace("ws_a", "Project Alpha");
        let ctx_b = ContextReference::workspace("ws_b", "Project Beta");

        detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Alpha".to_string()],
        );

        let res_fast = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:05:00Z",
            Some(ctx_b.clone()),
            0.92,
            "Very High",
            false,
            vec!["Explicit user move or 92% confidence".to_string()],
        );

        assert!(res_fast.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_b");
    }

    #[test]
    fn test_deduplication_of_transitions() {
        let mut detector = ContextTransitionDetector::new();

        let ctx_a = ContextReference::workspace("ws_a", "Project Alpha");

        let res1 = detector.evaluate_interpretation(
            Some("item_1"),
            Some("sess_1"),
            "2026-09-10T10:00:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Initial Alpha capture".to_string()],
        );
        assert!(res1.is_transition());

        let res2 = detector.evaluate_interpretation(
            Some("item_2"),
            Some("sess_1"),
            "2026-09-10T10:02:00Z",
            Some(ctx_a.clone()),
            0.90,
            "High",
            false,
            vec!["Second Alpha capture".to_string()],
        );
        assert!(res2.is_continuation());
        assert!(!res2.is_transition());
    }

    #[test]
    fn test_scenario_2_context_a_to_inactivity() {
        let mut detector = ContextTransitionDetector::new().with_inactivity_threshold(1800);

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

        let no_end = detector.check_inactivity("2026-09-10T10:15:00Z");
        assert!(no_end.is_none());
        assert!(detector.current_context().is_some());

        let end_trans = detector.check_inactivity("2026-09-10T10:45:00Z");
        assert!(end_trans.is_some());
        let t_end = end_trans.unwrap();
        assert_eq!(t_end.transition_type, ContextTransitionType::ContextEnd);
        assert_eq!(t_end.from_context.as_ref().unwrap().id, "ws_a");
        assert!(t_end.to_context.is_none());
        assert!(t_end.evidence.iter().any(|e| e.contains("Inactivity gap detected")));

        assert!(detector.current_context().is_none());
        assert_eq!(detector.context_history().front().unwrap().reference.id, "ws_a");
    }

    #[test]
    fn test_scenario_3_inactivity_to_context_b() {
        let mut detector = ContextTransitionDetector::new();

        assert!(detector.current_context().is_none());

        let ctx_b = ContextReference::workspace("ws_b", "Finance Dashboard");
        let res = detector.evaluate_interpretation(
            Some("item_10"),
            Some("sess_2"),
            "2026-09-10T11:00:00Z",
            Some(ctx_b.clone()),
            0.88,
            "High",
            false,
            vec!["Browser opened to stripe.com dashboard".to_string()],
        );

        assert!(res.is_transition());
        let t = res.transition().unwrap();
        assert_eq!(t.transition_type, ContextTransitionType::ContextStart);
        assert!(t.from_context.is_none());
        assert_eq!(t.to_context.as_ref().unwrap().id, "ws_b");
        assert_eq!(t.confidence, 0.88);
        assert!(t.evidence.iter().any(|e| e.contains("Started activity in 'Finance Dashboard'")));
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

        let ctx_noise = ContextReference::workspace("ws_other", "Unrelated Noise");
        let res_noise = detector.evaluate_interpretation(
            Some("item_noise"),
            Some("sess_1"),
            "2026-09-10T10:02:00Z",
            Some(ctx_noise),
            0.20,
            "Low",
            false,
            vec!["Weak single domain match".to_string()],
        );

        assert!(!res_noise.is_transition());
        assert_eq!(detector.current_context().unwrap().reference.id, "ws_a");
    }

    #[test]
    fn test_full_database_pipeline_and_membership_invariance() {
        let conn = setup_test_db();

        let ws_a = crate::db::operations::create_workspace(&conn, "Alpha Core").unwrap();
        let ws_b = crate::db::operations::create_workspace(&conn, "Beta UI").unwrap();
        let session = crate::db::operations::create_session(&conn).unwrap();

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

        let ret_details = res4.transition().unwrap().as_context_return().unwrap();
        assert_eq!(ret_details.returned_context.id, ws_a.id);
        assert_eq!(ret_details.previous_context.unwrap().id, ws_b.id);

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
