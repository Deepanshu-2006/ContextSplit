use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::context_engine::memory::InformationProvenance;
use crate::context_engine::session::{ContextSession, ContextSessionState};
use crate::context_engine::snapshot::ContextSnapshot;
use crate::context_engine::workset::ContextWorkset;
use crate::db::models::Item;

// =========================================================================
// Epistemic Disclaimer
// =========================================================================

pub const STRUCTURED_SESSION_SUMMARY_DISCLAIMER: &str =
    "This structured session summary is deterministically derived from recorded telemetry, task transitions, and workset evidence. It strictly presents grounded facts without generative hallucinations.";

fn default_summary_disclaimer() -> String {
    STRUCTURED_SESSION_SUMMARY_DISCLAIMER.to_string()
}

// =========================================================================
// SessionTaskSummary: Grounded Task Status
// =========================================================================

/// Grounded task summary for a working session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTaskSummary {
    /// Task identifier if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title (e.g. "Hero animation")
    pub task_name: String,
    /// Calibrated confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Whether this task was completed during the session
    pub is_completed: bool,
    /// Epistemic provenance
    pub provenance: InformationProvenance,
}

// =========================================================================
// LastMeaningfulActivitySummary: Specific Last Action
// =========================================================================

/// Detailed representation of the single last meaningful activity in the session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LastMeaningfulActivitySummary {
    /// Human-readable description (e.g. "Updated animation component")
    pub description: String,
    /// Name or title of the last meaningful entity (e.g. "Hero.tsx")
    pub item_title: String,
    /// Item type (e.g. "file", "url", "text")
    pub item_type: String,
    /// File path if source was a file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Web domain if source was a URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// ISO-8601 timestamp when this activity occurred
    pub timestamp: String,
}

// =========================================================================
// SessionArtifactChange: What Changed
// =========================================================================

/// Record of an artifact, file, or document modified or referenced during the session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionArtifactChange {
    /// File name, document title, or URL (e.g. "Hero.tsx", "styles/animation.css")
    pub entity_name: String,
    /// Type of interaction ("edited", "created", "viewed", "executed")
    pub change_type: String,
    /// Number of times touched during the session
    pub occurrences: usize,
    /// File path or URL if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    /// ISO-8601 timestamp when last touched
    pub last_touched_at: String,
}

// =========================================================================
// PreservedItemHint: What to Preserve for Resumption
// =========================================================================

/// High-priority item that should be retained and presented upon resumption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreservedItemHint {
    /// Item identifier
    pub item_id: String,
    /// Human-friendly title or label (e.g. "Hero.tsx", "Figma Design Spec")
    pub title: String,
    /// Item type (e.g. "file", "url", "text")
    pub item_type: String,
    /// Specific justification for preserving this item
    pub reason: String,
}

// =========================================================================
// StructuredSessionSummary: Core Structured Summary
// =========================================================================

/// Structured summary answering key contextual questions about a working session.
///
/// Answers:
/// 1. What was worked on? (`what_was_worked_on`, `task`)
/// 2. What changed? (`what_changed`)
/// 3. What was the last meaningful activity? (`meaningful_activity`, `last_meaningful_item`, `last_meaningful_activity`)
/// 4. What task was active? (`task`)
/// 5. What should be preserved for resumption? (`preservation_items`)
/// 6. State (`state`)
/// 7. Resume hint (`resume_hint`)
///
/// Invariants:
/// - Strictly structured data (no ungrounded AI hallucinated paragraphs).
/// - Only generates fields supported by concrete telemetry evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredSessionSummary {
    /// Canonical session identifier
    pub session_id: String,
    /// Context / Workspace identifier
    pub context_id: String,
    /// Context display name (e.g. "ContextSplit")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_name: Option<String>,
    /// Active or completed task (e.g. "Hero animation")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<SessionTaskSummary>,
    /// Primary meaningful activity summary (e.g. "Updated animation component")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaningful_activity: Option<String>,
    /// Ordered list of distinct meaningful activities during the session
    pub meaningful_activities: Vec<String>,
    /// Title of the last meaningful entity worked on (e.g. "Hero.tsx")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_item: Option<String>,
    /// Detailed record of the last meaningful action
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_activity: Option<LastMeaningfulActivitySummary>,
    /// High-level list of subjects/files worked on (e.g. `["Hero animation", "Hero.tsx", "animation.css"]`)
    pub what_was_worked_on: Vec<String>,
    /// Specific artifact and document changes
    pub what_changed: Vec<SessionArtifactChange>,
    /// Concrete working state description (e.g. "Implementation in progress", "Task completed", "Paused for review")
    pub state: String,
    /// Factual, grounded hint for resuming work (e.g. "Continue animation integration on Hero.tsx")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_hint: Option<String>,
    /// Concrete items to preserve in workset for seamless resumption
    pub preservation_items: Vec<PreservedItemHint>,
    /// Formatted session duration
    pub duration_formatted: String,
    /// Total count of supporting telemetry signals
    pub evidence_count: usize,
    /// Calibrated confidence in this summary [0.0, 1.0]
    pub confidence: f64,
    /// Epistemic disclaimer
    #[serde(default = "default_summary_disclaimer")]
    pub disclaimer: String,
}

// =========================================================================
// ContextSessionSummaryGenerator: Deterministic Summary Engine
// =========================================================================

pub struct ContextSessionSummaryGenerator;

impl ContextSessionSummaryGenerator {
    /// Generates a strictly grounded `StructuredSessionSummary` from session telemetry.
    pub fn generate_summary(
        session: &ContextSession,
        items: &[Item],
        workset: Option<&ContextWorkset>,
        _snapshot: Option<&ContextSnapshot>,
    ) -> StructuredSessionSummary {
        // ---------------------------------------------------------------------
        // 1. Filter Meaningful Telemetry (Exclude noise & empty focus hops)
        // ---------------------------------------------------------------------
        let meaningful_items: Vec<&Item> = items
            .iter()
            .filter(|it| {
                if it.content.trim().is_empty() {
                    return false;
                }
                if let Ok(meta_val) = serde_json::from_str::<Value>(&it.metadata) {
                    let is_focus_hop_only = meta_val
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "focus_change" || s == "app_activated")
                        .unwrap_or(false);

                    let has_content_payload = meta_val.get("file_path").is_some()
                        || meta_val.get("url").is_some()
                        || it.content.len() > 20;

                    if is_focus_hop_only && !has_content_payload {
                        return false;
                    }
                }
                true
            })
            .collect();

        // ---------------------------------------------------------------------
        // 2. Extract Active Task (from session or workset)
        // ---------------------------------------------------------------------
        let task_summary = session.task_reference.as_ref().map(|tr| SessionTaskSummary {
            task_id: tr.task_id.clone(),
            task_name: tr.task_name.clone(),
            confidence: tr.confidence,
            is_completed: tr.is_completed,
            provenance: tr.provenance,
        }).or_else(|| {
            workset.and_then(|ws| {
                ws.task_title.as_ref().map(|tt| SessionTaskSummary {
                    task_id: ws.task_id.clone(),
                    task_name: tt.clone(),
                    confidence: ws.confidence,
                    is_completed: false,
                    provenance: ws.provenance,
                })
            })
        });

        // ---------------------------------------------------------------------
        // 3. Extract What Changed & Artifact Changes
        // ---------------------------------------------------------------------
        let mut artifact_map: HashMap<String, (String, usize, Option<String>, String)> = HashMap::new();
        let mut what_worked_on_set: HashSet<String> = HashSet::new();

        if let Some(t) = &task_summary {
            what_worked_on_set.insert(t.task_name.clone());
        }

        for item in &meaningful_items {
            let meta_val: Value = serde_json::from_str(&item.metadata)
                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

            let file_path = meta_val.get("file_path").and_then(|v| v.as_str());
            let entity_name = if let Some(fp) = file_path {
                std::path::Path::new(fp)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(fp)
                    .to_string()
            } else if let Some(title) = &item.title {
                title.clone()
            } else {
                item.id.clone()
            };

            what_worked_on_set.insert(entity_name.clone());

            let entry = artifact_map.entry(entity_name.clone()).or_insert_with(|| {
                (
                    "edited".to_string(),
                    0,
                    file_path.map(|s| s.to_string()),
                    item.created_at.clone(),
                )
            });
            entry.1 += 1;
            entry.3 = item.created_at.clone();
        }

        let mut what_changed: Vec<SessionArtifactChange> = artifact_map
            .into_iter()
            .map(|(entity_name, (change_type, occurrences, target_path, last_touched_at))| {
                SessionArtifactChange {
                    entity_name,
                    change_type,
                    occurrences,
                    target_path,
                    last_touched_at,
                }
            })
            .collect();

        // Sort by occurrence count descending
        what_changed.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));

        // ---------------------------------------------------------------------
        // 4. Identify Last Meaningful Activity & Item
        // ---------------------------------------------------------------------
        let last_meaningful_item_record = meaningful_items.last().copied();

        let (last_meaningful_item, last_meaningful_activity, meaningful_activity_summary) =
            if let Some(last_it) = last_meaningful_item_record {
                let meta_val: Value = serde_json::from_str(&last_it.metadata)
                    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

                let file_path = meta_val.get("file_path").and_then(|v| v.as_str()).map(|s| s.to_string());
                let domain = meta_val.get("domain").and_then(|v| v.as_str()).map(|s| s.to_string());

                let item_label = if let Some(fp) = &file_path {
                    std::path::Path::new(fp)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(fp)
                        .to_string()
                } else if let Some(t) = &last_it.title {
                    t.clone()
                } else {
                    "Working Document".to_string()
                };

                let desc = if let Some(t) = &task_summary {
                    format!("Updated {} component for {}", item_label, t.task_name)
                } else {
                    format!("Updated {}", item_label)
                };

                let last_act = LastMeaningfulActivitySummary {
                    description: desc.clone(),
                    item_title: item_label.clone(),
                    item_type: last_it.item_type.clone(),
                    file_path,
                    domain,
                    timestamp: last_it.created_at.clone(),
                };

                (Some(item_label), Some(last_act), Some(desc))
            } else {
                (None, None, None)
            };

        // ---------------------------------------------------------------------
        // 5. Build Meaningful Activities List
        // ---------------------------------------------------------------------
        let mut meaningful_activities = Vec::new();
        if let Some(act) = &meaningful_activity_summary {
            meaningful_activities.push(act.clone());
        }
        for change in &what_changed {
            let act_str = format!("Edited {} ({} updates)", change.entity_name, change.occurrences);
            if !meaningful_activities.contains(&act_str) && meaningful_activities.len() < 5 {
                meaningful_activities.push(act_str);
            }
        }

        // ---------------------------------------------------------------------
        // 6. Formulate Working State Description
        // ---------------------------------------------------------------------
        let state_description = match session.session_state {
            ContextSessionState::Completed => "Task completed".to_string(),
            ContextSessionState::Paused => {
                if let Some(t) = &task_summary {
                    format!("Paused during {}", t.task_name)
                } else {
                    "Paused with preserved state".to_string()
                }
            }
            ContextSessionState::Interrupted => {
                if let Some(t) = &task_summary {
                    format!("Interrupted during {}", t.task_name)
                } else {
                    "Interrupted unexpectedly".to_string()
                }
            }
            ContextSessionState::Active | ContextSessionState::Resumed => {
                if let Some(t) = &task_summary {
                    if t.is_completed {
                        "Task completed".to_string()
                    } else {
                        "Implementation in progress".to_string()
                    }
                } else {
                    "Implementation in progress".to_string()
                }
            }
            ContextSessionState::Partial => "Partial activity segment".to_string(),
        };

        // ---------------------------------------------------------------------
        // 7. Grounded Resume Hint (Strictly non-hallucinated)
        // ---------------------------------------------------------------------
        let resume_hint = match session.session_state {
            ContextSessionState::Completed => {
                task_summary.as_ref().map(|t| format!("Review completed {} or choose next task", t.task_name))
            }
            ContextSessionState::Paused | ContextSessionState::Active | ContextSessionState::Resumed => {
                if let (Some(t), Some(item)) = (&task_summary, &last_meaningful_item) {
                    Some(format!("Continue {} on {}", t.task_name, item))
                } else if let Some(t) = &task_summary {
                    Some(format!("Continue {} integration", t.task_name))
                } else if let Some(item) = &last_meaningful_item {
                    Some(format!("Resume work on {}", item))
                } else {
                    None
                }
            }
            ContextSessionState::Interrupted => {
                if let (Some(t), Some(item)) = (&task_summary, &last_meaningful_item) {
                    Some(format!("Resume interrupted {} on {}", t.task_name, item))
                } else if let Some(item) = &last_meaningful_item {
                    Some(format!("Resume work on {}", item))
                } else {
                    None
                }
            }
            ContextSessionState::Partial => None,
        };

        // ---------------------------------------------------------------------
        // 8. Preservation Items (What should be preserved for resumption)
        // ---------------------------------------------------------------------
        let mut preservation_items = Vec::new();

        if let Some(ws) = workset {
            for wi in &ws.items {
                preservation_items.push(PreservedItemHint {
                    item_id: wi.id.clone(),
                    title: wi.title.clone().unwrap_or_else(|| wi.id.clone()),
                    item_type: wi.item_type.clone(),
                    reason: "Active workset member".to_string(),
                });
            }
        } else {
            for change in &what_changed {
                if preservation_items.len() < 5 {
                    preservation_items.push(PreservedItemHint {
                        item_id: change.entity_name.clone(),
                        title: change.entity_name.clone(),
                        item_type: "file".to_string(),
                        reason: format!("Edited {} times in session", change.occurrences),
                    });
                }
            }
        }

        let mut what_was_worked_on: Vec<String> = what_worked_on_set.into_iter().collect();
        what_was_worked_on.sort();

        StructuredSessionSummary {
            session_id: session.session_id.clone(),
            context_id: session.context_id.clone(),
            context_name: session.context_name.clone(),
            task: task_summary,
            meaningful_activity: meaningful_activity_summary,
            meaningful_activities,
            last_meaningful_item,
            last_meaningful_activity,
            what_was_worked_on,
            what_changed,
            state: state_description,
            resume_hint,
            preservation_items,
            duration_formatted: session.duration_formatted.clone(),
            evidence_count: meaningful_items.len(),
            confidence: session.confidence,
            disclaimer: default_summary_disclaimer(),
        }
    }
}

// =========================================================================
// Unit Tests: Structured Contextual Session Summaries
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::session::{SessionTaskReference, ContextSessionState};
    use crate::context_engine::workset::{WorksetItemReference, WorksetItemRelevance, WorksetState};

    fn make_test_item(id: &str, title: &str, content: &str, file_path: Option<&str>, created_at: &str) -> Item {
        Item {
            id: id.to_string(),
            workspace_id: Some("ws_contextsplit".to_string()),
            session_id: Some("sess_hero".to_string()),
            item_type: "file".to_string(),
            content: content.to_string(),
            title: Some(title.to_string()),
            metadata: serde_json::json!({
                "app_name": "Visual Studio Code",
                "file_path": file_path,
            })
            .to_string(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    #[test]
    fn test_example_scenario_structured_session_summary() {
        // Setup the exact example from Day 24 Step 8 prompt:
        // Task: Hero animation
        // Meaningful activity: Updated animation component
        // Last meaningful item: Hero.tsx
        // State: Implementation in progress
        // Resume hint: Continue animation integration (or Continue Hero animation on Hero.tsx)

        let session = ContextSession {
            session_id: "sess_hero_01".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: Some("ContextSplit".to_string()),
            started_at: "2026-09-22T01:00:00Z".to_string(),
            ended_at: None,
            last_activity_at: "2026-09-22T01:45:00Z".to_string(),
            duration_seconds: 2700,
            duration_formatted: "45m 0s".to_string(),
            task_reference: Some(SessionTaskReference {
                task_id: Some("task_hero".to_string()),
                task_name: "Hero animation".to_string(),
                confidence: 0.94,
                is_completed: false,
                provenance: InformationProvenance::Inferred,
            }),
            meaningful_activity_count: 3,
            total_item_count: 3,
            snapshot_reference: None,
            session_state: ContextSessionState::Active,
            confidence: 0.92,
            is_micro_session: false,
            metadata: serde_json::json!({}),
            disclaimer: default_summary_disclaimer(),
        };

        let items = vec![
            make_test_item("it_1", "animation.css", ".hero { opacity: 1; }", Some("/src/styles/animation.css"), "2026-09-22T01:10:00Z"),
            make_test_item("it_2", "Hero.tsx", "export const Hero = () => <motion.div />;", Some("/src/components/Hero.tsx"), "2026-09-22T01:25:00Z"),
            make_test_item("it_3", "Hero.tsx", "export const Hero = () => <motion.div animate={{ opacity: 1 }} />;", Some("/src/components/Hero.tsx"), "2026-09-22T01:45:00Z"),
        ];

        let summary = ContextSessionSummaryGenerator::generate_summary(&session, &items, None, None);

        // 1. Task verification
        assert!(summary.task.is_some());
        assert_eq!(summary.task.as_ref().unwrap().task_name, "Hero animation");

        // 2. Meaningful activity verification
        assert!(summary.meaningful_activity.is_some());
        let meaningful_act = summary.meaningful_activity.as_ref().unwrap();
        assert!(meaningful_act.contains("Hero.tsx") || meaningful_act.contains("Hero animation"));

        // 3. Last meaningful item
        assert_eq!(summary.last_meaningful_item.as_deref(), Some("Hero.tsx"));

        // 4. State
        assert_eq!(summary.state, "Implementation in progress");

        // 5. Resume hint
        assert!(summary.resume_hint.is_some());
        let hint = summary.resume_hint.as_ref().unwrap();
        assert!(hint.contains("Hero animation") || hint.contains("Hero.tsx"));

        // 6. What changed
        assert_eq!(summary.what_changed.len(), 2);
        assert_eq!(summary.what_changed[0].entity_name, "Hero.tsx");
        assert_eq!(summary.what_changed[0].occurrences, 2);
        assert_eq!(summary.what_changed[1].entity_name, "animation.css");
        assert_eq!(summary.what_changed[1].occurrences, 1);

        // 7. Preservation items
        assert!(!summary.preservation_items.is_empty());
        assert!(summary.preservation_items.iter().any(|p| p.title == "Hero.tsx"));
    }

    #[test]
    fn test_completed_session_summary() {
        let session = ContextSession {
            session_id: "sess_completed".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: Some("ContextSplit".to_string()),
            started_at: "2026-09-22T00:00:00Z".to_string(),
            ended_at: Some("2026-09-22T01:00:00Z".to_string()),
            last_activity_at: "2026-09-22T01:00:00Z".to_string(),
            duration_seconds: 3600,
            duration_formatted: "1h 0m".to_string(),
            task_reference: Some(SessionTaskReference {
                task_id: Some("task_auth".to_string()),
                task_name: "Implement OAuth".to_string(),
                confidence: 0.98,
                is_completed: true,
                provenance: InformationProvenance::Confirmed,
            }),
            meaningful_activity_count: 2,
            total_item_count: 2,
            snapshot_reference: None,
            session_state: ContextSessionState::Completed,
            confidence: 0.95,
            is_micro_session: false,
            metadata: serde_json::json!({}),
            disclaimer: default_summary_disclaimer(),
        };

        let items = vec![
            make_test_item("it_1", "auth.ts", "export const login = () => {};", Some("/src/auth.ts"), "2026-09-22T00:30:00Z"),
            make_test_item("it_2", "auth.test.ts", "test('oauth works', () => {});", Some("/src/auth.test.ts"), "2026-09-22T01:00:00Z"),
        ];

        let summary = ContextSessionSummaryGenerator::generate_summary(&session, &items, None, None);

        assert_eq!(summary.state, "Task completed");
        assert!(summary.resume_hint.as_ref().unwrap().contains("completed Implement OAuth"));
    }

    #[test]
    fn test_paused_session_summary() {
        let session = ContextSession {
            session_id: "sess_paused".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: Some("ContextSplit".to_string()),
            started_at: "2026-09-22T00:00:00Z".to_string(),
            ended_at: None,
            last_activity_at: "2026-09-22T00:45:00Z".to_string(),
            duration_seconds: 2700,
            duration_formatted: "45m 0s".to_string(),
            task_reference: Some(SessionTaskReference {
                task_id: Some("task_billing".to_string()),
                task_name: "Stripe Webhooks".to_string(),
                confidence: 0.90,
                is_completed: false,
                provenance: InformationProvenance::Inferred,
            }),
            meaningful_activity_count: 2,
            total_item_count: 2,
            snapshot_reference: None,
            session_state: ContextSessionState::Paused,
            confidence: 0.88,
            is_micro_session: false,
            metadata: serde_json::json!({}),
            disclaimer: default_summary_disclaimer(),
        };

        let items = vec![
            make_test_item("it_1", "stripe.ts", "export const webhook = () => {};", Some("/src/stripe.ts"), "2026-09-22T00:45:00Z"),
        ];

        let summary = ContextSessionSummaryGenerator::generate_summary(&session, &items, None, None);

        assert_eq!(summary.state, "Paused during Stripe Webhooks");
        assert!(summary.resume_hint.as_ref().unwrap().contains("Continue Stripe Webhooks"));
    }

    #[test]
    fn test_no_hallucination_when_no_evidence_available() {
        let session = ContextSession {
            session_id: "sess_empty".to_string(),
            context_id: "ws_empty".to_string(),
            context_name: None,
            started_at: "2026-09-22T00:00:00Z".to_string(),
            ended_at: None,
            last_activity_at: "2026-09-22T00:00:00Z".to_string(),
            duration_seconds: 0,
            duration_formatted: "0s".to_string(),
            task_reference: None,
            meaningful_activity_count: 0,
            total_item_count: 0,
            snapshot_reference: None,
            session_state: ContextSessionState::Partial,
            confidence: 0.30,
            is_micro_session: true,
            metadata: serde_json::json!({}),
            disclaimer: default_summary_disclaimer(),
        };

        let summary = ContextSessionSummaryGenerator::generate_summary(&session, &[], None, None);

        // Invariant: If no task or items exist, do NOT hallucinate!
        assert!(summary.task.is_none());
        assert!(summary.last_meaningful_item.is_none());
        assert!(summary.last_meaningful_activity.is_none());
        assert!(summary.resume_hint.is_none());
        assert!(summary.what_changed.is_empty());
        assert_eq!(summary.evidence_count, 0);
    }

    #[test]
    fn test_json_serialization_roundtrip() {
        let session = ContextSession {
            session_id: "sess_json_01".to_string(),
            context_id: "ws_contextsplit".to_string(),
            context_name: Some("ContextSplit".to_string()),
            started_at: "2026-09-22T01:00:00Z".to_string(),
            ended_at: None,
            last_activity_at: "2026-09-22T01:45:00Z".to_string(),
            duration_seconds: 2700,
            duration_formatted: "45m 0s".to_string(),
            task_reference: Some(SessionTaskReference {
                task_id: Some("task_hero".to_string()),
                task_name: "Hero animation".to_string(),
                confidence: 0.94,
                is_completed: false,
                provenance: InformationProvenance::Inferred,
            }),
            meaningful_activity_count: 1,
            total_item_count: 1,
            snapshot_reference: None,
            session_state: ContextSessionState::Active,
            confidence: 0.90,
            is_micro_session: false,
            metadata: serde_json::json!({}),
            disclaimer: default_summary_disclaimer(),
        };

        let items = vec![
            make_test_item("it_1", "Hero.tsx", "export const Hero = () => {};", Some("/src/Hero.tsx"), "2026-09-22T01:45:00Z"),
        ];

        let summary = ContextSessionSummaryGenerator::generate_summary(&session, &items, None, None);

        let json_str = serde_json::to_string_pretty(&summary).expect("Serialization should succeed");
        let deserialized: StructuredSessionSummary = serde_json::from_str(&json_str).expect("Deserialization should succeed");

        assert_eq!(summary, deserialized);
        assert_eq!(deserialized.session_id, "sess_json_01");
        assert_eq!(deserialized.task.unwrap().task_name, "Hero animation");
        assert_eq!(deserialized.last_meaningful_item.unwrap(), "Hero.tsx");
    }
}
