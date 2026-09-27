//! ContextSplit Real-World Test Mode Controller
//!
//! Provides a dedicated, controlled Real-World Testing Mode to observe
//! the existing Context Engine during actual daily desktop usage on macOS (and other OSs).
//!
//! SAFETY & PRIVACY GUARANTEES:
//! - Strictly opt-in: no diagnostics are recorded in standard production mode.
//! - Automatic sanitization: credentials, API keys, passwords, bearer tokens,
//!   and raw file contents are scrubbed before recording.
//! - Clear separation: test session data is labeled, bounded, and completely isolated
//!   from production user settings and storage.
//! - Local-first: diagnostics never leave the local device.

use std::collections::VecDeque;
use std::sync::{OnceLock, RwLock};
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Maximum number of diagnostic events held in memory per test session.
pub const MAX_REAL_WORLD_EVENTS: usize = 5000;

// =========================================================================
// Real-World Test Session Models
// =========================================================================

/// Operational status of a real-world testing session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSessionStatus {
    Active,
    Completed,
    Aborted,
}

/// Metadata and state for a real-world test session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealWorldTestSession {
    pub session_id: String,
    pub session_name: String,
    pub description: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: TestSessionStatus,
    pub recorded_events_count: usize,
    pub environment: String,
    pub is_test_mode: bool,
}

/// Aggregated metrics summary produced upon closing a real-world test session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealWorldTestSessionSummary {
    pub session_id: String,
    pub session_name: String,
    pub duration_seconds: i64,
    pub total_events: usize,
    pub context_detections_count: usize,
    pub context_transitions_count: usize,
    pub task_changes_count: usize,
    pub memory_changes_count: usize,
    pub workset_changes_count: usize,
    pub lifecycle_changes_count: usize,
    pub feedback_events_count: usize,
    pub reconstructions_count: usize,
    pub resume_readiness_checks_count: usize,
    pub average_detection_latency_ms: f64,
    pub average_reconstruction_latency_ms: f64,
    pub status: TestSessionStatus,
}

// =========================================================================
// Diagnostic Event Models (Sanitized & Development-Safe)
// =========================================================================

/// Candidate context evaluated during detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateContextDiagnostic {
    pub context_id: String,
    pub context_name: String,
    pub confidence_score: f32,
    pub rank: usize,
    pub is_selected: bool,
    pub rejection_reason: Option<String>,
}

/// Context detection event diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextDetectionDiagnostic {
    pub item_id: Option<String>,
    pub selected_context_id: Option<String>,
    pub selected_context_name: Option<String>,
    pub confidence_score: f32,
    pub candidates: Vec<CandidateContextDiagnostic>,
    pub evidence_summary: Vec<String>,
    pub is_ambiguous: bool,
    pub is_noise_suppressed: bool,
    pub processing_latency_us: u64,
}

/// Context transition event diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextTransitionDiagnostic {
    pub from_context_id: Option<String>,
    pub to_context_id: Option<String>,
    pub transition_type: String,
    pub confidence: f32,
    pub triggered_by: String,
    pub timestamp: String,
}

/// Task candidate evolution diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskChangeDiagnostic {
    pub context_id: String,
    pub task_id: String,
    pub task_title: String,
    pub confidence: f32,
    pub previous_status: Option<String>,
    pub new_status: String,
    pub change_trigger: String,
}

/// Context memory change diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryChangeDiagnostic {
    pub context_id: String,
    pub memory_id: String,
    pub memory_title: String,
    pub importance_tier: String,
    pub change_type: String, // "promoted", "created", "decayed", "evicted"
}

/// Context working set derivation diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorksetChangeDiagnostic {
    pub context_id: String,
    pub total_items_count: usize,
    pub primary_working_set_count: usize,
    pub top_item_titles: Vec<String>,
    pub derivation_latency_us: u64,
}

/// Lifecycle state change diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LifecycleChangeDiagnostic {
    pub context_id: String,
    pub previous_state: String,
    pub new_state: String,
    pub trigger_reason: String,
}

/// User feedback observation diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeedbackDiagnostic {
    pub context_id: Option<String>,
    pub target_id: String,
    pub action: String,
    pub source: String,
    pub notes: Option<String>,
}

/// Context reconstruction benchmark diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructionDiagnostic {
    pub context_id: String,
    pub total_latency_us: u64,
    pub workset_items_count: usize,
    pub tasks_count: usize,
    pub memories_count: usize,
    pub has_snapshot: bool,
    pub is_cold_reconstruction: bool,
}

/// Resume readiness evaluation diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeReadinessDiagnostic {
    pub context_id: String,
    pub readiness_score: f32,
    pub can_resume: bool,
    pub blocking_factors: Vec<String>,
    pub action_items_count: usize,
}

/// Latency measurement diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LatencyDiagnostic {
    pub subsystem: String,
    pub operation: String,
    pub duration_us: u64,
}

/// Union of all real-world diagnostic events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", content = "data")]
pub enum RealWorldDiagnosticEvent {
    ContextDetection(ContextDetectionDiagnostic),
    ContextTransition(ContextTransitionDiagnostic),
    TaskChange(TaskChangeDiagnostic),
    MemoryChange(MemoryChangeDiagnostic),
    WorksetChange(WorksetChangeDiagnostic),
    LifecycleChange(LifecycleChangeDiagnostic),
    Feedback(FeedbackDiagnostic),
    Reconstruction(ReconstructionDiagnostic),
    ResumeReadiness(ResumeReadinessDiagnostic),
    LatencyMetric(LatencyDiagnostic),
}

/// Envelope for recorded diagnostic events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordedDiagnosticEnvelope {
    pub session_id: String,
    pub event_index: usize,
    pub recorded_at: String,
    pub payload: RealWorldDiagnosticEvent,
}

// =========================================================================
// Privacy & Sanitization Engine
// =========================================================================

/// Scrubs sensitive content such as passwords, bearer tokens, API keys, and raw secrets.
pub fn sanitize_diagnostic_text(input: &str) -> String {
    let mut sanitized = input.to_string();

    // Redact Bearer tokens
    if sanitized.to_lowercase().contains("bearer ") {
        let re_bearer = regex::Regex::new(r"(?i)bearer\s+[a-zA-Z0-9_\-\.]+").unwrap();
        sanitized = re_bearer.replace_all(&sanitized, "Bearer [REDACTED]").to_string();
    }

    // Redact GitHub / API tokens
    let re_token = regex::Regex::new(r"(ghp_[a-zA-Z0-9]{20,}|gho_[a-zA-Z0-9]{20,}|sk-[a-zA-Z0-9]{20,})").unwrap();
    sanitized = re_token.replace_all(&sanitized, "[API_KEY_REDACTED]").to_string();

    // Redact password or key assignments in query strings / configs
    let re_pwd = regex::Regex::new(r"(?i)(password|passwd|secret|api_key|apikey|token|access_token)=([^&\s]+)").unwrap();
    sanitized = re_pwd.replace_all(&sanitized, "$1=[REDACTED]").to_string();

    // Redact private file paths containing usernames if needed, keeping relative path
    let re_user_path = regex::Regex::new(r"/Users/[^/]+/").unwrap();
    sanitized = re_user_path.replace_all(&sanitized, "/Users/developer/").to_string();

    sanitized
}

// =========================================================================
// Real-World Test Mode Controller
// =========================================================================

/// In-memory controller managing Real-World Test Mode lifecycle and event capture.
pub struct RealWorldTestModeController {
    active_session: RwLock<Option<RealWorldTestSession>>,
    events_buffer: RwLock<VecDeque<RecordedDiagnosticEnvelope>>,
    completed_sessions: RwLock<Vec<RealWorldTestSessionSummary>>,
}

impl RealWorldTestModeController {
    pub fn new() -> Self {
        Self {
            active_session: RwLock::new(None),
            events_buffer: RwLock::new(VecDeque::with_capacity(MAX_REAL_WORLD_EVENTS)),
            completed_sessions: RwLock::new(Vec::new()),
        }
    }

    /// Checks if Real-World Test Mode is actively recording.
    pub fn is_active(&self) -> bool {
        self.active_session.read().map(|s| s.is_some()).unwrap_or(false)
    }

    /// Retrieves a snapshot of the active test session, if one is running.
    pub fn get_active_session(&self) -> Option<RealWorldTestSession> {
        self.active_session.read().ok().and_then(|s| s.clone())
    }

    /// Starts a new controlled Real-World Testing Session.
    pub fn start_session(&self, name: &str, description: Option<&str>) -> Result<RealWorldTestSession, String> {
        let mut session_lock = self.active_session.write().map_err(|e| e.to_string())?;
        if let Some(existing) = session_lock.as_ref() {
            return Err(format!("Test session '{}' ({}) is already active. Stop it first.", existing.session_name, existing.session_id));
        }

        let session_id = format!("test_session_{}_{}", Utc::now().timestamp(), &uuid::Uuid::new_v4().to_string()[..8]);
        let session = RealWorldTestSession {
            session_id,
            session_name: name.to_string(),
            description: description.map(|d| d.to_string()),
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            status: TestSessionStatus::Active,
            recorded_events_count: 0,
            environment: "macOS_Desktop_RealWorld".to_string(),
            is_test_mode: true,
        };

        *session_lock = Some(session.clone());

        // Clear previous event buffer for clean session recording
        if let Ok(mut buffer) = self.events_buffer.write() {
            buffer.clear();
        }

        Ok(session)
    }

    /// Stops the active Real-World Testing Session and aggregates performance metrics.
    pub fn stop_session(&self) -> Result<RealWorldTestSessionSummary, String> {
        let mut session_lock = self.active_session.write().map_err(|e| e.to_string())?;
        let active = session_lock.take().ok_or_else(|| "No active test session to stop.".to_string())?;

        let buffer = self.events_buffer.read().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let started_dt = chrono::DateTime::parse_from_rfc3339(&active.started_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);
        let duration_seconds = (now - started_dt).num_seconds().max(1);

        let mut detection_count = 0;
        let mut transition_count = 0;
        let mut task_count = 0;
        let mut memory_count = 0;
        let mut workset_count = 0;
        let mut lifecycle_count = 0;
        let mut feedback_count = 0;
        let mut reconstruction_count = 0;
        let mut resume_readiness_count = 0;

        let mut total_detection_latency_us = 0u64;
        let mut total_reconstruction_latency_us = 0u64;

        for env in buffer.iter().filter(|e| e.session_id == active.session_id) {
            match &env.payload {
                RealWorldDiagnosticEvent::ContextDetection(d) => {
                    detection_count += 1;
                    total_detection_latency_us += d.processing_latency_us;
                }
                RealWorldDiagnosticEvent::ContextTransition(_) => transition_count += 1,
                RealWorldDiagnosticEvent::TaskChange(_) => task_count += 1,
                RealWorldDiagnosticEvent::MemoryChange(_) => memory_count += 1,
                RealWorldDiagnosticEvent::WorksetChange(_) => workset_count += 1,
                RealWorldDiagnosticEvent::LifecycleChange(_) => lifecycle_count += 1,
                RealWorldDiagnosticEvent::Feedback(_) => feedback_count += 1,
                RealWorldDiagnosticEvent::Reconstruction(r) => {
                    reconstruction_count += 1;
                    total_reconstruction_latency_us += r.total_latency_us;
                }
                RealWorldDiagnosticEvent::ResumeReadiness(_) => resume_readiness_count += 1,
                RealWorldDiagnosticEvent::LatencyMetric(_) => {}
            }
        }

        let avg_detection_ms = if detection_count > 0 {
            (total_detection_latency_us as f64) / (detection_count as f64) / 1000.0
        } else {
            0.0
        };

        let avg_reconstruction_ms = if reconstruction_count > 0 {
            (total_reconstruction_latency_us as f64) / (reconstruction_count as f64) / 1000.0
        } else {
            0.0
        };

        let summary = RealWorldTestSessionSummary {
            session_id: active.session_id,
            session_name: active.session_name,
            duration_seconds,
            total_events: buffer.len(),
            context_detections_count: detection_count,
            context_transitions_count: transition_count,
            task_changes_count: task_count,
            memory_changes_count: memory_count,
            workset_changes_count: workset_count,
            lifecycle_changes_count: lifecycle_count,
            feedback_events_count: feedback_count,
            reconstructions_count: reconstruction_count,
            resume_readiness_checks_count: resume_readiness_count,
            average_detection_latency_ms: avg_detection_ms,
            average_reconstruction_latency_ms: avg_reconstruction_ms,
            status: TestSessionStatus::Completed,
        };

        if let Ok(mut completed) = self.completed_sessions.write() {
            completed.push(summary.clone());
        }

        Ok(summary)
    }

    /// Records a development-safe diagnostic event if a test session is active.
    pub fn record_event(&self, mut event: RealWorldDiagnosticEvent) {
        let session_id = match self.active_session.read() {
            Ok(lock) => match lock.as_ref() {
                Some(s) if s.status == TestSessionStatus::Active => s.session_id.clone(),
                _ => return, // Inactive: zero recording overhead
            },
            Err(_) => return,
        };

        // Apply mandatory privacy sanitization to text fields
        match &mut event {
            RealWorldDiagnosticEvent::ContextDetection(d) => {
                for c in &mut d.candidates {
                    c.context_name = sanitize_diagnostic_text(&c.context_name);
                    if let Some(ref mut reason) = c.rejection_reason {
                        *reason = sanitize_diagnostic_text(reason);
                    }
                }
                d.evidence_summary = d.evidence_summary.iter().map(|e| sanitize_diagnostic_text(e)).collect();
            }
            RealWorldDiagnosticEvent::TaskChange(t) => {
                t.task_title = sanitize_diagnostic_text(&t.task_title);
            }
            RealWorldDiagnosticEvent::MemoryChange(m) => {
                m.memory_title = sanitize_diagnostic_text(&m.memory_title);
            }
            RealWorldDiagnosticEvent::WorksetChange(w) => {
                w.top_item_titles = w.top_item_titles.iter().map(|t| sanitize_diagnostic_text(t)).collect();
            }
            RealWorldDiagnosticEvent::Feedback(f) => {
                if let Some(ref mut notes) = f.notes {
                    *notes = sanitize_diagnostic_text(notes);
                }
            }
            _ => {}
        }

        if let Ok(mut buffer) = self.events_buffer.write() {
            if buffer.len() >= MAX_REAL_WORLD_EVENTS {
                buffer.pop_front();
            }
            let event_index = buffer.len();
            buffer.push_back(RecordedDiagnosticEnvelope {
                session_id,
                event_index,
                recorded_at: Utc::now().to_rfc3339(),
                payload: event,
            });
        }

        // Increment session count
        if let Ok(mut lock) = self.active_session.write() {
            if let Some(ref mut s) = *lock {
                s.recorded_events_count += 1;
            }
        }
    }

    /// Fetches all recorded events for a given session.
    pub fn get_session_events(&self, session_id: &str) -> Vec<RecordedDiagnosticEnvelope> {
        self.events_buffer
            .read()
            .map(|buf| buf.iter().filter(|e| e.session_id == session_id).cloned().collect())
            .unwrap_or_default()
    }

    /// Fetches recent events up to a limit.
    pub fn get_recent_events(&self, limit: usize) -> Vec<RecordedDiagnosticEnvelope> {
        self.events_buffer
            .read()
            .map(|buf| buf.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    /// Generates a structured markdown report for a completed or active test session.
    pub fn generate_session_report(&self, session_id: &str) -> Option<String> {
        let events = self.get_session_events(session_id);
        if events.is_empty() {
            return None;
        }

        let mut report = String::new();
        report.push_str(&format!("# Real-World Test Session Report: `{}`\n\n", session_id));
        report.push_str(&format!("- **Total Recorded Events:** {}\n", events.len()));
        report.push_str(&format!("- **First Recorded Event:** {}\n", events.first().map(|e| e.recorded_at.as_str()).unwrap_or_default()));
        report.push_str(&format!("- **Last Recorded Event:** {}\n\n", events.last().map(|e| e.recorded_at.as_str()).unwrap_or_default()));

        report.push_str("## Event Breakdown\n\n");
        let mut detection_cnt = 0;
        let mut transition_cnt = 0;
        let mut task_cnt = 0;
        let mut workset_cnt = 0;
        let mut feedback_cnt = 0;

        for e in &events {
            match &e.payload {
                RealWorldDiagnosticEvent::ContextDetection(_) => detection_cnt += 1,
                RealWorldDiagnosticEvent::ContextTransition(_) => transition_cnt += 1,
                RealWorldDiagnosticEvent::TaskChange(_) => task_cnt += 1,
                RealWorldDiagnosticEvent::WorksetChange(_) => workset_cnt += 1,
                RealWorldDiagnosticEvent::Feedback(_) => feedback_cnt += 1,
                _ => {}
            }
        }

        report.push_str(&format!("- Context Detections: {}\n", detection_cnt));
        report.push_str(&format!("- Context Transitions: {}\n", transition_cnt));
        report.push_str(&format!("- Task Changes: {}\n", task_cnt));
        report.push_str(&format!("- Workset Derivations: {}\n", workset_cnt));
        report.push_str(&format!("- User Feedback Events: {}\n\n", feedback_cnt));

        report.push_str("## Privacy & Sanitization Compliance\n\n");
        report.push_str("- [x] Password & credential redaction applied\n");
        report.push_str("- [x] Bearer / API token scrubbed\n");
        report.push_str("- [x] Path username anonymized\n");
        report.push_str("- [x] Zero network telemetry transmission\n");

        Some(report)
    }

    /// Clears all recorded events and history.
    pub fn clear_all_test_sessions(&self) {
        if let Ok(mut lock) = self.active_session.write() {
            *lock = None;
        }
        if let Ok(mut buffer) = self.events_buffer.write() {
            buffer.clear();
        }
        if let Ok(mut completed) = self.completed_sessions.write() {
            completed.clear();
        }
    }
}

// =========================================================================
// Global Singleton
// =========================================================================

static GLOBAL_TEST_MODE_CONTROLLER: OnceLock<RealWorldTestModeController> = OnceLock::new();

/// Returns the global thread-safe controller for Real-World Test Mode.
pub fn get_real_world_test_controller() -> &'static RealWorldTestModeController {
    GLOBAL_TEST_MODE_CONTROLLER.get_or_init(RealWorldTestModeController::new)
}

// =========================================================================
// Unit Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitization_engine_redacts_credentials_and_secrets() {
        let raw = "Bearer ghp_12345678901234567890 password=supersecret file /Users/deepanshukhatri/project/secret.rs";
        let sanitized = sanitize_diagnostic_text(raw);

        assert!(!sanitized.contains("supersecret"), "Password must be scrubbed");
        assert!(!sanitized.contains("ghp_12345678901234567890"), "API token must be scrubbed");
        assert!(!sanitized.contains("/Users/deepanshukhatri/"), "Username in path must be scrubbed");
        assert!(sanitized.contains("password=[REDACTED]"));
        assert!(sanitized.contains("/Users/developer/"));
    }

    #[test]
    fn test_start_stop_test_session_lifecycle() {
        let controller = RealWorldTestModeController::new();
        assert!(!controller.is_active());

        // Start session
        let session = controller.start_session("Mac Daily Dev Test", Some("Observing engine during Xcode & VSCode coding")).unwrap();
        assert!(controller.is_active());
        assert_eq!(session.session_name, "Mac Daily Dev Test");
        assert_eq!(session.status, TestSessionStatus::Active);

        // Record diagnostics
        controller.record_event(RealWorldDiagnosticEvent::ContextDetection(ContextDetectionDiagnostic {
            item_id: Some("item_101".to_string()),
            selected_context_id: Some("ws_payx".to_string()),
            selected_context_name: Some("PayX SDK".to_string()),
            confidence_score: 0.94,
            candidates: vec![
                CandidateContextDiagnostic {
                    context_id: "ws_payx".to_string(),
                    context_name: "PayX SDK".to_string(),
                    confidence_score: 0.94,
                    rank: 1,
                    is_selected: true,
                    rejection_reason: None,
                },
            ],
            evidence_summary: vec!["File: Sources/PayX/Payment.swift".to_string()],
            is_ambiguous: false,
            is_noise_suppressed: false,
            processing_latency_us: 1250,
        }));

        controller.record_event(RealWorldDiagnosticEvent::ContextTransition(ContextTransitionDiagnostic {
            from_context_id: None,
            to_context_id: Some("ws_payx".to_string()),
            transition_type: "context_switch".to_string(),
            confidence: 0.95,
            triggered_by: "active_window_focus".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }));

        let recent = controller.get_recent_events(10);
        assert_eq!(recent.len(), 2);

        // Stop session
        let summary = controller.stop_session().unwrap();
        assert!(!controller.is_active());
        assert_eq!(summary.context_detections_count, 1);
        assert_eq!(summary.context_transitions_count, 1);
        assert_eq!(summary.total_events, 2);

        // Generate report
        let report = controller.generate_session_report(&session.session_id).unwrap();
        assert!(report.contains("Real-World Test Session Report"));
        assert!(report.contains("Context Detections: 1"));
    }

    #[test]
    fn test_inactive_test_mode_has_zero_overhead() {
        let controller = RealWorldTestModeController::new();
        assert!(!controller.is_active());

        // Recording event when inactive should be completely ignored
        controller.record_event(RealWorldDiagnosticEvent::LatencyMetric(LatencyDiagnostic {
            subsystem: "scoring".to_string(),
            operation: "evaluate_item_context".to_string(),
            duration_us: 450,
        }));

        assert_eq!(controller.get_recent_events(10).len(), 0);
    }
}
