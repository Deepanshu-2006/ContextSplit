use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::context_engine::evidence::{ContextEvidenceSet, EvidenceCategory};
use crate::context_engine::scoring::{ContradictoryEvidence, EvidenceContribution, WorkspaceScoringContext};

/// Detailed classification of contradiction types between evidence items and candidate contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionKind {
    /// Explicit user feedback or prior dismissal directly contradicts candidate context
    UserCorrection,
    /// Item's active session is firmly associated with a different context
    SessionConflict,
    /// Strong semantic similarity points to one context while user organization history points to another
    SemanticHistoryConflict,
    /// Multiple candidate contexts possess strong, competing evidence with close score margins
    CompetingContexts,
    /// Fresh recent activity contradicts older/decayed historical context associations
    TemporalConflict,
    /// Unrelated streaming media or entertainment (YouTube, Spotify, Netflix) opened during active context
    UnrelatedMediaOrEntertainment,
    /// Unrelated generic web search query lacking semantic or task connection to context
    UnrelatedSearchQuery,
    /// Unrelated code repository or mismatched file opened during context
    UnrelatedRepositoryOrFile,
    /// Multi-purpose host application (Chrome, VS Code, Safari) with no corroborating contextual evidence
    UncorroboratedApplication,
    /// Minor peripheral discrepancy (e.g. application or content type overlap with another workspace)
    PeripheralDiscrepancy,
}

impl ContradictionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContradictionKind::UserCorrection => "user_correction",
            ContradictionKind::SessionConflict => "session_conflict",
            ContradictionKind::SemanticHistoryConflict => "semantic_history_conflict",
            ContradictionKind::CompetingContexts => "competing_contexts",
            ContradictionKind::TemporalConflict => "temporal_conflict",
            ContradictionKind::UnrelatedMediaOrEntertainment => "unrelated_media_or_entertainment",
            ContradictionKind::UnrelatedSearchQuery => "unrelated_search_query",
            ContradictionKind::UnrelatedRepositoryOrFile => "unrelated_repository_or_file",
            ContradictionKind::UncorroboratedApplication => "uncorroborated_application",
            ContradictionKind::PeripheralDiscrepancy => "peripheral_discrepancy",
        }
    }

    /// Default baseline penalty weight applied to confidence [0.0, 1.0].
    pub fn baseline_penalty(&self) -> f32 {
        match self {
            ContradictionKind::UserCorrection => 0.50,
            ContradictionKind::SessionConflict => 0.35,
            ContradictionKind::SemanticHistoryConflict => 0.30,
            ContradictionKind::CompetingContexts => 0.30,
            ContradictionKind::TemporalConflict => 0.20,
            ContradictionKind::UnrelatedMediaOrEntertainment => 0.50,
            ContradictionKind::UnrelatedSearchQuery => 0.45,
            ContradictionKind::UnrelatedRepositoryOrFile => 0.45,
            ContradictionKind::UncorroboratedApplication => 0.30,
            ContradictionKind::PeripheralDiscrepancy => 0.05,
        }
    }
}

/// Structured contradiction assessment for a specific candidate context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextContradictionReport {
    pub context_id: String,
    pub context_name: String,
    /// Identified primary contradiction kind (if any)
    pub contradiction_kind: Option<ContradictionKind>,
    /// Normalized contradiction strength [0.0, 1.0]
    pub contradiction_strength: f32,
    /// Total confidence penalty discount applied due to contradictions [0.0, 1.0]
    pub confidence_penalty: f32,
    /// Primary explanation of the contradiction
    pub contradiction_explanation: Option<String>,
    /// Structured list of contradictory evidence records
    pub contradictory_evidence: Vec<ContradictoryEvidence>,
    /// Competing context ID if the contradiction involves cross-context conflict
    pub competing_context_id: Option<String>,
    pub competing_context_name: Option<String>,
}

/// Comprehensive cross-context contradiction analysis result across all candidate contexts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiContextContradictionAnalysis {
    /// Context-specific contradiction reports keyed by context_id
    pub reports: HashMap<String, ContextContradictionReport>,
    /// Global contradiction/conflict level across candidate contexts [0.0, 1.0]
    pub global_conflict_level: f32,
    /// Whether any critical conflict exists that should block high-confidence auto-assignment
    pub has_critical_conflict: bool,
    /// Key summary explanation of multi-context conflict dynamics
    pub summary: String,
}

/// Dedicated analyzer for detecting contradictory context evidence and computing confidence penalties.
#[derive(Debug, Clone, Default)]
pub struct ContradictionAnalyzer;

impl ContradictionAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates contradictory evidence for a single candidate workspace within the broader context pool.
    pub fn analyze_candidate(
        &self,
        evidence_set: &ContextEvidenceSet,
        ws: &WorkspaceScoringContext,
        all_workspaces: &[WorkspaceScoringContext],
        raw_score: f32,
        contributions: &[EvidenceContribution],
        runner_up: Option<(&WorkspaceScoringContext, f32)>,
    ) -> ContextContradictionReport {
        let mut contradictions: Vec<ContradictoryEvidence> = Vec::new();
        let mut primary_kind: Option<ContradictionKind> = None;
        let mut max_strength: f32 = 0.0;
        let mut primary_explanation: Option<String> = None;
        let mut competing_id: Option<String> = None;
        let mut competing_name: Option<String> = None;

        // ---------------------------------------------------------------------
        // 1. Explicit User Feedback / Correction against this Workspace
        // ---------------------------------------------------------------------
        for ev in evidence_set.by_category(EvidenceCategory::UserFeedback) {
            let is_match = ev.source_ref.as_ref().map(|r| r.entity_id == ws.id).unwrap_or(false)
                || ev.attributes.get("workspace_id").and_then(|v| v.as_str()) == Some(&ws.id);

            if is_match {
                let feedback_type = ev
                    .attributes
                    .get("feedback_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user_dismissal");
                let desc = format!("User previously rejected suggestion ({}) for {}", feedback_type, ws.name);
                contradictions.push(ContradictoryEvidence {
                    category: "user_feedback".to_string(),
                    description: desc.clone(),
                    penalty: ContradictionKind::UserCorrection.baseline_penalty(),
                });

                if 0.95 > max_strength {
                    max_strength = 0.95;
                    primary_kind = Some(ContradictionKind::UserCorrection);
                    primary_explanation = Some(desc);
                }
            }
        }

        // ---------------------------------------------------------------------
        // 2. Session Conflict: Active session belongs firmly to another workspace
        // ---------------------------------------------------------------------
        let mut session_ids_in_evidence: Vec<&str> = Vec::new();
        for ev in evidence_set.by_category(EvidenceCategory::SameSession) {
            if let Some(s_id) = ev.attributes.get("session_id").and_then(|v| v.as_str())
                .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str())) {
                session_ids_in_evidence.push(s_id);
            }
        }

        if !session_ids_in_evidence.is_empty() {
            let ws_has_session = session_ids_in_evidence.iter().any(|&s| ws.sessions.iter().any(|ws_s| ws_s == s));
            if !ws_has_session {
                // Find other workspace that owns this session
                for other in all_workspaces {
                    if other.id != ws.id {
                        let other_has_session = session_ids_in_evidence.iter().any(|&s| other.sessions.iter().any(|os| os == s));
                        if other_has_session {
                            let desc = format!(
                                "Item capture session belongs to '{}', contradicting assignment to '{}'",
                                other.name, ws.name
                            );
                            let strength = 0.80_f32;
                            contradictions.push(ContradictoryEvidence {
                                category: "session_conflict".to_string(),
                                description: desc.clone(),
                                penalty: ContradictionKind::SessionConflict.baseline_penalty(),
                            });

                            if strength > max_strength {
                                max_strength = strength;
                                primary_kind = Some(ContradictionKind::SessionConflict);
                                primary_explanation = Some(desc);
                                competing_id = Some(other.id.clone());
                                competing_name = Some(other.name.clone());
                            }
                            break;
                        }
                    }
                }
            }
        }

        // ---------------------------------------------------------------------
        // 3. Semantic Similarity vs User History Conflict
        // ---------------------------------------------------------------------
        let has_semantic_for_this = contributions.iter().any(|c| {
            (c.category == "semantic_similarity_items" || c.category == "semantic_profile")
                && c.is_positive
                && c.weighted_score >= 0.25
        });
        let has_history_for_this = contributions.iter().any(|c| {
            c.category == "workspace_history" && c.is_positive && c.weighted_score >= 0.25
        });

        // Check if another workspace has strong history while this one has semantic, or vice-versa
        for ev in evidence_set.by_category(EvidenceCategory::WorkspaceHistory) {
            let history_ws_id = ev.attributes.get("workspace_id").and_then(|v| v.as_str())
                .or_else(|| ev.source_ref.as_ref().map(|r| r.entity_id.as_str()));

            if let Some(h_id) = history_ws_id {
                if h_id != ws.id && has_semantic_for_this && !has_history_for_this {
                    let other_name = all_workspaces
                        .iter()
                        .find(|w| w.id == h_id)
                        .map(|w| w.name.as_str())
                        .unwrap_or("another workspace");

                    let desc = format!(
                        "Semantic similarity points to '{}' while user organization history points to '{}'",
                        ws.name, other_name
                    );

                    let has_session_for_this = contributions.iter().any(|c| c.category == "same_session" && c.is_positive);
                    let (strength, penalty) = if has_session_for_this {
                        (0.40_f32, 0.12_f32)
                    } else {
                        (0.80_f32, ContradictionKind::SemanticHistoryConflict.baseline_penalty())
                    };

                    contradictions.push(ContradictoryEvidence {
                        category: "semantic_history_conflict".to_string(),
                        description: desc.clone(),
                        penalty,
                    });

                    if strength > max_strength {
                        max_strength = strength;
                        primary_kind = Some(ContradictionKind::SemanticHistoryConflict);
                        primary_explanation = Some(desc);
                        competing_id = Some(h_id.to_string());
                        competing_name = Some(other_name.to_string());
                    }
                }
            }
        }

        // ---------------------------------------------------------------------
        // 4. Competing Contexts (Close score margin with runner-up)
        // ---------------------------------------------------------------------
        if let Some((other_ws, other_score)) = runner_up {
            if raw_score >= 0.35 && other_score >= 0.30 {
                let margin = (raw_score - other_score).abs();
                if margin <= 0.15 {
                    let margin_factor = (1.0 - (margin / 0.15)).clamp(0.0, 1.0);
                    let strength = (margin_factor * 0.60 + 0.40 * (other_score / 1.0).clamp(0.0, 1.0)).clamp(0.10, 0.95);
                    let desc = format!(
                        "Competing evidence between '{}' ({:.2}) and candidate '{}' ({:.2}) with narrow margin ({:.2})",
                        ws.name, raw_score, other_ws.name, other_score, margin
                    );

                    contradictions.push(ContradictoryEvidence {
                        category: "competing_contexts".to_string(),
                        description: desc.clone(),
                        penalty: ContradictionKind::CompetingContexts.baseline_penalty() * margin_factor,
                    });

                    if strength > max_strength {
                        max_strength = strength;
                        primary_kind = Some(ContradictionKind::CompetingContexts);
                        primary_explanation = Some(desc);
                        competing_id = Some(other_ws.id.clone());
                        competing_name = Some(other_ws.name.clone());
                    }
                }
            }
        }

        // ---------------------------------------------------------------------
        // 5. Temporal Conflict: Recent Activity vs Stale Evidence
        // ---------------------------------------------------------------------
        let has_recent_temporal_for_other = evidence_set.items.iter().any(|e| {
            (e.category == EvidenceCategory::TemporalProximity || e.category == EvidenceCategory::SameSession)
                && e.strength >= 0.80
                && e.attributes.get("workspace_id").and_then(|v| v.as_str()).map(|id| id != ws.id).unwrap_or(false)
        });

        if has_recent_temporal_for_other && has_history_for_this && !has_semantic_for_this {
            let desc = format!(
                "Recent active signals contradict older historical association with '{}'",
                ws.name
            );
            let strength = 0.60_f32;
            contradictions.push(ContradictoryEvidence {
                category: "temporal_conflict".to_string(),
                description: desc.clone(),
                penalty: ContradictionKind::TemporalConflict.baseline_penalty(),
            });

            if strength > max_strength {
                max_strength = strength;
                primary_kind = Some(ContradictionKind::TemporalConflict);
                primary_explanation = Some(desc);
            }
        }

        // ---------------------------------------------------------------------
        // 6. Anti-Contamination: Entertainment, Unrelated Searches, Repos & Uncorroborated Apps
        // ---------------------------------------------------------------------
        for ev in evidence_set.by_category(EvidenceCategory::BrowserDomain) {
            if let Some(domain) = ev.attributes.get("domain").and_then(|v| v.as_str()) {
                let d_lower = domain.to_lowercase();
                let is_media = d_lower.contains("youtube.com")
                    || d_lower.contains("youtu.be")
                    || d_lower.contains("netflix.com")
                    || d_lower.contains("spotify.com")
                    || d_lower.contains("twitch.tv")
                    || d_lower.contains("tiktok.com");

                let ws_name_lower = ws.name.to_lowercase();
                let ws_has_media = ws_name_lower.contains("youtube")
                    || ws_name_lower.contains("video")
                    || ws_name_lower.contains("media")
                    || ws_name_lower.contains("music")
                    || ws_name_lower.contains("audio");

                if is_media && !ws_has_media && !has_semantic_for_this {
                    let desc = format!(
                        "Entertainment activity on '{}' contradicts focus of '{}'",
                        domain, ws.name
                    );
                    let strength = 0.85_f32;
                    contradictions.push(ContradictoryEvidence {
                        category: "unrelated_media_or_entertainment".to_string(),
                        description: desc.clone(),
                        penalty: ContradictionKind::UnrelatedMediaOrEntertainment.baseline_penalty(),
                    });

                    if strength > max_strength {
                        max_strength = strength;
                        primary_kind = Some(ContradictionKind::UnrelatedMediaOrEntertainment);
                        primary_explanation = Some(desc);
                    }
                }

                // Check generic search engine without topic overlap
                let is_search = d_lower.contains("google.com")
                    || d_lower.contains("bing.com")
                    || d_lower.contains("duckduckgo.com");

                if is_search && !has_semantic_for_this && !has_history_for_this {
                    let desc = format!(
                        "Generic search on '{}' lacks contextual connection to '{}'",
                        domain, ws.name
                    );
                    let strength = 0.65_f32;
                    contradictions.push(ContradictoryEvidence {
                        category: "unrelated_search_query".to_string(),
                        description: desc.clone(),
                        penalty: ContradictionKind::UnrelatedSearchQuery.baseline_penalty(),
                    });

                    if strength > max_strength {
                        max_strength = strength;
                        primary_kind = Some(ContradictionKind::UnrelatedSearchQuery);
                        primary_explanation = Some(desc);
                    }
                }

                // Check code repository in non-coding workspace
                let is_repo = d_lower.contains("github.com") || d_lower.contains("gitlab.com");
                let ws_is_code = ws_name_lower.contains("code")
                    || ws_name_lower.contains("dev")
                    || ws_name_lower.contains("api")
                    || ws_name_lower.contains("system")
                    || ws_name_lower.contains("os")
                    || ws_name_lower.contains("engine")
                    || ws_name_lower.contains("app");

                if is_repo && !ws_is_code && !has_semantic_for_this {
                    let desc = format!(
                        "Code repository on '{}' is unrelated to non-software context '{}'",
                        domain, ws.name
                    );
                    let strength = 0.75_f32;
                    contradictions.push(ContradictoryEvidence {
                        category: "unrelated_repository_or_file".to_string(),
                        description: desc.clone(),
                        penalty: ContradictionKind::UnrelatedRepositoryOrFile.baseline_penalty(),
                    });

                    if strength > max_strength {
                        max_strength = strength;
                        primary_kind = Some(ContradictionKind::UnrelatedRepositoryOrFile);
                        primary_explanation = Some(desc);
                    }
                }
            }
        }

        // Check common application alone without corroboration
        let only_app_contribution = contributions.len() == 1
            && contributions.iter().any(|c| c.category == "source_app" && c.is_positive);

        if only_app_contribution && !has_semantic_for_this && !has_history_for_this {
            let desc = format!(
                "Application identity alone without contextual corroboration for '{}'",
                ws.name
            );
            let strength = 0.50_f32;
            contradictions.push(ContradictoryEvidence {
                category: "uncorroborated_application".to_string(),
                description: desc.clone(),
                penalty: ContradictionKind::UncorroboratedApplication.baseline_penalty(),
            });

            if strength > max_strength {
                max_strength = strength;
                primary_kind = Some(ContradictionKind::UncorroboratedApplication);
                primary_explanation = Some(desc);
            }
        }

        // ---------------------------------------------------------------------
        // 7. Weak Contradiction: Peripheral Discrepancy
        // ---------------------------------------------------------------------
        if max_strength == 0.0 && raw_score >= 0.40 {
            // Check if source app or content type matches another workspace with items
            for ev in evidence_set.by_category(EvidenceCategory::SourceApp) {
                if let Some(app) = ev.attributes.get("app_name").and_then(|v| v.as_str()) {
                    for other in all_workspaces {
                        if other.id != ws.id && other.app_names.iter().any(|a| a == app) && !ws.app_names.iter().any(|a| a == app) {
                            let strength = 0.20_f32;
                            let desc = format!("Shares application '{}' with '{}'", app, other.name);
                            contradictions.push(ContradictoryEvidence {
                                category: "peripheral_discrepancy".to_string(),
                                description: desc.clone(),
                                penalty: ContradictionKind::PeripheralDiscrepancy.baseline_penalty(),
                            });

                            if strength > max_strength {
                                max_strength = strength;
                                primary_kind = Some(ContradictionKind::PeripheralDiscrepancy);
                                primary_explanation = Some(desc);
                                competing_id = Some(other.id.clone());
                                competing_name = Some(other.name.clone());
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Calculate total confidence penalty
        let confidence_penalty = contradictions
            .iter()
            .map(|c| c.penalty)
            .sum::<f32>()
            .clamp(0.0, 0.75);

        ContextContradictionReport {
            context_id: ws.id.clone(),
            context_name: ws.name.clone(),
            contradiction_kind: primary_kind,
            contradiction_strength: max_strength.clamp(0.0, 1.0),
            confidence_penalty,
            contradiction_explanation: primary_explanation,
            contradictory_evidence: contradictions,
            competing_context_id: competing_id,
            competing_context_name: competing_name,
        }
    }

    /// Evaluates contradictions across all candidate workspaces simultaneously.
    pub fn analyze_all(
        &self,
        evidence_set: &ContextEvidenceSet,
        workspaces: &[WorkspaceScoringContext],
        candidate_scores: &[(String, f32, Vec<EvidenceContribution>)],
    ) -> MultiContextContradictionAnalysis {
        let mut reports: HashMap<String, ContextContradictionReport> = HashMap::new();
        let mut max_global_conflict: f32 = 0.0;
        let mut has_critical = false;

        // Sort candidate scores descending to identify top and runner-up pairs
        let mut sorted_scores = candidate_scores.to_vec();
        sorted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for ws in workspaces.iter() {
            let (raw_score, contribs) = candidate_scores
                .iter()
                .find(|(id, _, _)| id == &ws.id)
                .map(|(_, s, c)| (*s, c.as_slice()))
                .unwrap_or((0.0, &[]));

            // Determine runner-up context for this workspace
            let runner_up = if sorted_scores.len() > 1 {
                if sorted_scores[0].0 == ws.id {
                    let r_id = &sorted_scores[1].0;
                    let r_score = sorted_scores[1].1;
                    workspaces.iter().find(|w| &w.id == r_id).map(|w| (w, r_score))
                } else {
                    let top_id = &sorted_scores[0].0;
                    let top_score = sorted_scores[0].1;
                    workspaces.iter().find(|w| &w.id == top_id).map(|w| (w, top_score))
                }
            } else {
                None
            };

            let report = self.analyze_candidate(
                evidence_set,
                ws,
                workspaces,
                raw_score,
                contribs,
                runner_up,
            );

            if report.contradiction_strength > max_global_conflict {
                max_global_conflict = report.contradiction_strength;
            }

            if report.contradiction_strength >= 0.70 {
                has_critical = true;
            }

            reports.insert(ws.id.clone(), report);
        }

        let summary = if has_critical {
            "Critical contradictions detected between candidate contexts; confidence is suppressed."
                .to_string()
        } else if max_global_conflict > 0.0 {
            format!(
                "Minor context discrepancies detected (max conflict strength: {:.2}).",
                max_global_conflict
            )
        } else {
            "No contradictions detected across candidate contexts.".to_string()
        };

        MultiContextContradictionAnalysis {
            reports,
            global_conflict_level: max_global_conflict,
            has_critical_conflict: has_critical,
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::evidence::ContextEvidence;

    #[test]
    fn test_clear_winner_has_zero_contradictions() {
        // Clear winner: Context A has session, semantic, and history evidence; Context B has none.
        let mut evidence_set = ContextEvidenceSet::new("target_clear");
        evidence_set.add(ContextEvidence::observed_session("target_clear", "sess_a", None));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_clear",
            "item_a",
            0.92,
            Some("Doc A"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_clear",
            "ws_a",
            "Context A",
            1.0,
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.sessions.push("sess_a".to_string());
        ws_a.item_ids.push("item_a".to_string());

        let ws_b = WorkspaceScoringContext::new("ws_b", "Context B");

        let analyzer = ContradictionAnalyzer::new();
        let report_a = analyzer.analyze_candidate(
            &evidence_set,
            &ws_a,
            &[ws_a.clone(), ws_b.clone()],
            0.85,
            &[EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.3,
                description: "Session match".to_string(),
                is_positive: true,
            }],
            Some((&ws_b, 0.0)),
        );

        assert_eq!(report_a.contradiction_strength, 0.0);
        assert_eq!(report_a.confidence_penalty, 0.0);
        assert_eq!(report_a.contradiction_kind, None);
        assert!(report_a.contradictory_evidence.is_empty());
    }

    #[test]
    fn test_two_competing_contexts_detected() {
        // Two competing contexts: Both Context A and Context B have strong evidence with a narrow margin.
        let mut evidence_set = ContextEvidenceSet::new("target_competing");
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_competing",
            "item_a",
            0.88,
            Some("Doc A"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_competing",
            "item_b",
            0.86,
            Some("Doc B"),
            Some("Context B"),
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.item_ids.push("item_a".to_string());

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Context B");
        ws_b.item_ids.push("item_b".to_string());

        let analyzer = ContradictionAnalyzer::new();
        let report_a = analyzer.analyze_candidate(
            &evidence_set,
            &ws_a,
            &[ws_a.clone(), ws_b.clone()],
            0.85,
            &[],
            Some((&ws_b, 0.82)),
        );

        assert_eq!(report_a.contradiction_kind, Some(ContradictionKind::CompetingContexts));
        assert!(report_a.contradiction_strength >= 0.70);
        assert!(report_a.confidence_penalty >= 0.20);
        assert!(report_a.contradiction_explanation.unwrap().contains("Competing evidence"));
    }

    #[test]
    fn test_semantic_vs_history_conflict_detected() {
        // Semantic points to Context A while user history points to Context B.
        let mut evidence_set = ContextEvidenceSet::new("target_sem_hist");
        evidence_set.add(ContextEvidence::inferred_semantic_similarity(
            "target_sem_hist",
            "item_a",
            0.90,
            Some("Research Paper"),
            Some("Context A"),
            None,
        ));
        evidence_set.add(ContextEvidence::inferred_workspace_history(
            "target_sem_hist",
            "ws_b",
            "Context B",
            1.0,
            None,
        ));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.item_ids.push("item_a".to_string());

        let ws_b = WorkspaceScoringContext::new("ws_b", "Context B");

        let analyzer = ContradictionAnalyzer::new();
        let report_a = analyzer.analyze_candidate(
            &evidence_set,
            &ws_a,
            &[ws_a.clone(), ws_b.clone()],
            0.75,
            &[EvidenceContribution {
                category: "semantic_similarity_items".to_string(),
                raw_strength: 0.90,
                weighted_score: 0.40,
                description: "Semantic match".to_string(),
                is_positive: true,
            }],
            None,
        );

        assert_eq!(report_a.contradiction_kind, Some(ContradictionKind::SemanticHistoryConflict));
        assert_eq!(report_a.contradiction_strength, 0.80);
        assert!(report_a.confidence_penalty >= 0.30);
        assert!(report_a.contradiction_explanation.unwrap().contains("user organization history points to 'Context B'"));
    }

    #[test]
    fn test_explicit_user_correction_detected() {
        // Explicit user correction / dismissal against Context A.
        let mut evidence_set = ContextEvidenceSet::new("target_dismissed");
        evidence_set.add(ContextEvidence::observed_app("target_dismissed", "Safari", None));
        evidence_set.add(ContextEvidence::observed_user_dismissal("target_dismissed", "ws_a", Some("Context A"), None));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.app_names.push("Safari".to_string());

        let analyzer = ContradictionAnalyzer::new();
        let report_a = analyzer.analyze_candidate(
            &evidence_set,
            &ws_a,
            &[ws_a.clone()],
            0.35,
            &[],
            None,
        );

        assert_eq!(report_a.contradiction_kind, Some(ContradictionKind::UserCorrection));
        assert_eq!(report_a.contradiction_strength, 0.95);
        assert_eq!(report_a.confidence_penalty, 0.50);
        assert!(report_a.contradiction_explanation.unwrap().contains("User previously rejected"));
    }

    #[test]
    fn test_weak_contradiction_detected() {
        // Context A has strong primary signal, but minor app overlap with Context B.
        let mut evidence_set = ContextEvidenceSet::new("target_weak");
        evidence_set.add(ContextEvidence::observed_app("target_weak", "Slack", None));

        let mut ws_a = WorkspaceScoringContext::new("ws_a", "Context A");
        ws_a.app_names.push("VSCode".to_string());

        let mut ws_b = WorkspaceScoringContext::new("ws_b", "Context B");
        ws_b.app_names.push("Slack".to_string());

        let analyzer = ContradictionAnalyzer::new();
        let report_a = analyzer.analyze_candidate(
            &evidence_set,
            &ws_a,
            &[ws_a.clone(), ws_b.clone()],
            0.50,
            &[],
            None,
        );

        assert_eq!(report_a.contradiction_kind, Some(ContradictionKind::PeripheralDiscrepancy));
        assert_eq!(report_a.contradiction_strength, 0.20);
        assert!(report_a.confidence_penalty <= 0.10);
    }

    #[test]
    fn test_unrelated_media_entertainment_contradiction() {
        let mut evidence_set = ContextEvidenceSet::new("target_yt");
        evidence_set.add(ContextEvidence::observed_domain("target_yt", "youtube.com", None, None));

        let ws_payx = WorkspaceScoringContext::new("ws_payx", "PayX Billing Engine");
        let analyzer = ContradictionAnalyzer::new();
        let report = analyzer.analyze_candidate(
            &evidence_set,
            &ws_payx,
            &[ws_payx.clone()],
            0.20,
            &[],
            None,
        );

        assert_eq!(report.contradiction_kind, Some(ContradictionKind::UnrelatedMediaOrEntertainment));
        assert_eq!(report.contradiction_strength, 0.85);
        assert!(report.confidence_penalty >= 0.50);
        assert!(report.contradiction_explanation.unwrap().contains("Entertainment activity"));
    }

    #[test]
    fn test_unrelated_search_contradiction() {
        let mut evidence_set = ContextEvidenceSet::new("target_search");
        evidence_set.add(ContextEvidence::observed_domain("target_search", "google.com", None, None));

        let ws_os = WorkspaceScoringContext::new("ws_os", "University OS assignment");
        let analyzer = ContradictionAnalyzer::new();
        let report = analyzer.analyze_candidate(
            &evidence_set,
            &ws_os,
            &[ws_os.clone()],
            0.15,
            &[],
            None,
        );

        assert_eq!(report.contradiction_kind, Some(ContradictionKind::UnrelatedSearchQuery));
        assert_eq!(report.contradiction_strength, 0.65);
        assert!(report.confidence_penalty >= 0.45);
    }

    #[test]
    fn test_unrelated_repo_in_non_coding_workspace_contradiction() {
        let mut evidence_set = ContextEvidenceSet::new("target_repo");
        evidence_set.add(ContextEvidence::observed_domain("target_repo", "github.com", None, None));

        let ws_tokyo = WorkspaceScoringContext::new("ws_tokyo", "Tokyo Trip");
        let analyzer = ContradictionAnalyzer::new();
        let report = analyzer.analyze_candidate(
            &evidence_set,
            &ws_tokyo,
            &[ws_tokyo.clone()],
            0.20,
            &[],
            None,
        );

        assert_eq!(report.contradiction_kind, Some(ContradictionKind::UnrelatedRepositoryOrFile));
        assert_eq!(report.contradiction_strength, 0.75);
        assert!(report.confidence_penalty >= 0.45);
    }

    #[test]
    fn test_uncorroborated_app_contradiction() {
        let evidence_set = ContextEvidenceSet::new("target_app");
        let ws_payx = WorkspaceScoringContext::new("ws_payx", "PayX");
        let analyzer = ContradictionAnalyzer::new();
        let report = analyzer.analyze_candidate(
            &evidence_set,
            &ws_payx,
            &[ws_payx.clone()],
            0.20,
            &[EvidenceContribution {
                category: "source_app".to_string(),
                raw_strength: 0.50,
                weighted_score: 0.20,
                description: "Same Chrome source app".to_string(),
                is_positive: true,
            }],
            None,
        );

        assert_eq!(report.contradiction_kind, Some(ContradictionKind::UncorroboratedApplication));
        assert_eq!(report.contradiction_strength, 0.50);
        assert!(report.confidence_penalty >= 0.30);
    }
}
