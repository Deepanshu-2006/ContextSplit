use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::models::{
    AIContextReasoningResponse, AIFailureReason, AIReasoningError,
    CandidateContextEvidence, CuratedContextSignal, DeterministicEvidenceAssessment, EvidenceMetadata,
    EvidencePackage, ItemRelationshipEvidence, RelatedItemEvidence, SemanticSimilaritySummary,
    SessionEvidence, TargetItemEvidence, TemporalEvidence, WorkspaceProfileSummary, ContextReasoningResult,
};
pub use super::privacy::scrub_private_data;
use super::privacy::{AIPrivacyBoundary, AIPrivacyPolicy};
use super::traits::AIReasoner;

/// Configurable builder that constructs a deterministic, privacy-sanitized
/// EvidencePackage from the existing Context Engine.
///
/// CRITICAL ARCHITECTURAL RULES ENFORCED:
/// 1. Only extracts information necessary for contextual reasoning.
/// 2. Does NOT dump the entire database (applies top-K limits and relevance filters).
/// 3. Does NOT include unnecessary private information (scrubs credentials, tokens, passwords).
/// 4. Does NOT include raw embeddings (`Vec<f32>` arrays are excluded; only summaries are produced).
/// 5. Does NOT include internal database IDs (rowids, signal primary keys, OS process IDs).
/// 6. Fully deterministic output with strict multi-key sorting.
/// 7. Enforces AI Privacy Boundary and user privacy settings (blocks when disabled).
#[derive(Debug, Clone)]
pub struct EvidencePackageBuilder {
    pub max_candidate_contexts: usize,
    pub max_related_items: usize,
    pub max_relationships: usize,
    pub max_semantic_summaries: usize,
    pub max_signals: usize,
    pub max_snippet_length: usize,
    pub min_relationship_strength: f64,
    pub min_candidate_score: f64,
    pub privacy_policy: Option<AIPrivacyPolicy>,
}

impl Default for EvidencePackageBuilder {
    fn default() -> Self {
        Self {
            max_candidate_contexts: 5,
            max_related_items: 3,
            max_relationships: 8,
            max_semantic_summaries: 5,
            max_signals: 6,
            max_snippet_length: 200,
            min_relationship_strength: 0.40,
            min_candidate_score: 0.05,
            privacy_policy: None,
        }
    }
}

impl EvidencePackageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_privacy_policy(mut self, policy: AIPrivacyPolicy) -> Self {
        self.privacy_policy = Some(policy);
        self
    }

    pub fn with_max_candidate_contexts(mut self, limit: usize) -> Self {
        self.max_candidate_contexts = limit;
        self
    }

    pub fn with_max_related_items(mut self, limit: usize) -> Self {
        self.max_related_items = limit;
        self
    }

    pub fn with_max_relationships(mut self, limit: usize) -> Self {
        self.max_relationships = limit;
        self
    }

    pub fn with_max_snippet_length(mut self, length: usize) -> Self {
        self.max_snippet_length = length;
        self
    }

    pub fn with_min_relationship_strength(mut self, min: f64) -> Self {
        self.min_relationship_strength = min;
        self
    }

    /// Constructs the EvidencePackage deterministically from the database.
    pub fn build(&self, conn: &Connection, item_id: &str) -> Result<EvidencePackage, String> {
        let now = chrono::Utc::now();

        // 0. Resolve Privacy Policy & Boundary Check
        let policy = match &self.privacy_policy {
            Some(p) => p.clone(),
            None => {
                let settings = crate::db::settings::get_settings(conn).unwrap_or_default();
                AIPrivacyPolicy::from_privacy_settings(&settings)
            }
        };

        let boundary = AIPrivacyBoundary::new(policy.clone());
        boundary.check_permission().map_err(|e| e.to_string())?;

        // Count total items in the database to calculate total_db_items_excluded
        let total_db_items: usize = conn
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap_or(0);

        // -------------------------------------------------------------------
        // 1. Current Target Item Metadata (Sanitized & Scrubbed)
        // -------------------------------------------------------------------
        let item = crate::db::operations::get_item(conn, item_id)
            .map_err(|e| format!("Failed to fetch target item '{}': {}", item_id, e))?;

        let raw_snippet = match item.item_type.as_str() {
            "image" => "[Image data excluded]".to_string(),
            "file" => {
                if let Ok(meta_val) = serde_json::from_str::<Value>(&item.metadata) {
                    let fname = meta_val.get("filename").and_then(|v| v.as_str()).unwrap_or("unnamed_file");
                    format!("[File: {}]", fname)
                } else {
                    "[File content excluded]".to_string()
                }
            }
            _ => {
                let trimmed = item.content.trim();
                if trimmed.chars().count() > self.max_snippet_length {
                    let truncated: String = trimmed.chars().take(self.max_snippet_length).collect();
                    format!("{}...", truncated)
                } else {
                    trimmed.to_string()
                }
            }
        };
        let content_snippet = scrub_private_data(&raw_snippet);
        let title = item.title.as_deref().map(scrub_private_data);

        // Fetch context signals for target item
        let raw_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(item_id))
            .unwrap_or_default();

        let mut app_name: Option<String> = None;
        let mut domain: Option<String> = None;

        if policy.allow_active_app {
            for sig in &raw_signals {
                if sig.signal_type == "active_app" {
                    if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                        if let Some(app) = v.get("app_name").and_then(|a| a.as_str()) {
                            if app_name.is_none() {
                                app_name = Some(app.to_string());
                            }
                        }
                    }
                }
            }
        }

        if policy.allow_browser_domain {
            for sig in &raw_signals {
                if sig.signal_type == "url_metadata" {
                    if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                        if let Some(d) = v.get("domain").and_then(|d| d.as_str()) {
                            if domain.is_none() {
                                domain = Some(d.to_string());
                            }
                        }
                    }
                }
            }
        }

        let session_id = if policy.allow_session_tracking {
            item.session_id.clone()
        } else {
            None
        };

        let target_item = TargetItemEvidence {
            id: item.id.clone(),
            item_type: item.item_type.clone(),
            title,
            content_snippet,
            app_name,
            domain,
            session_id,
            captured_at: item.created_at.clone(),
        };

        // -------------------------------------------------------------------
        // 2. Candidate Contexts (Workspaces & Candidates with Confidence Scores)
        // -------------------------------------------------------------------
        let evaluation = crate::context_engine::scoring::evaluate_item_context(conn, item_id)?;
        let mut candidate_contexts = Vec::new();

        for ws_eval in evaluation.existing_workspaces {
            if ws_eval.score < self.min_candidate_score && candidate_contexts.len() >= 2 {
                continue;
            }

            let ws_obj = crate::db::operations::get_workspace(conn, &ws_eval.workspace_id).ok();
            let lifecycle_state = ws_obj
                .as_ref()
                .map(|w| w.lifecycle_state.clone())
                .unwrap_or_else(|| "active".to_string());

            let item_count = crate::db::operations::list_items(conn, Some(&ws_eval.workspace_id))
                .map(|items| items.len())
                .unwrap_or(0);

            let temporal_recency_score = ws_obj
                .as_ref()
                .and_then(|w| w.last_activity_at.as_deref())
                .map(|ts| {
                    let decay = crate::context_engine::temporal::get_temporal_decay_config(conn);
                    round_to_4_dp(decay.compute_weight(ts, now))
                })
                .unwrap_or(1.0);

            candidate_contexts.push(CandidateContextEvidence {
                context_id: ws_eval.workspace_id,
                name: ws_eval.workspace_name,
                context_type: "workspace".to_string(),
                lifecycle_state,
                deterministic_score: round_to_4_dp(ws_eval.score),
                deterministic_confidence: round_to_4_dp(ws_eval.confidence),
                confidence_band: ws_eval.confidence_band,
                evidence_factors: ws_eval.reasons,
                item_count,
                temporal_recency_score,
            });
        }

        for cand_eval in evaluation.potential_new_contexts {
            if cand_eval.confidence < self.min_candidate_score && !candidate_contexts.is_empty() {
                continue;
            }

            let cid = cand_eval.candidate_id.unwrap_or_else(|| format!("cand_{}", cand_eval.label));
            candidate_contexts.push(CandidateContextEvidence {
                context_id: cid,
                name: cand_eval.label,
                context_type: "candidate".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: round_to_4_dp(cand_eval.confidence),
                deterministic_confidence: round_to_4_dp(cand_eval.confidence),
                confidence_band: cand_eval.confidence_band,
                evidence_factors: cand_eval.reasons,
                item_count: cand_eval.item_count,
                temporal_recency_score: 1.0,
            });
        }

        // Deterministic sort: highest score first, then alphabetical context_id
        candidate_contexts.sort_by(|a, b| {
            b.deterministic_score
                .partial_cmp(&a.deterministic_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.context_id.cmp(&b.context_id))
        });
        candidate_contexts.truncate(self.max_candidate_contexts);

        // Pre-fetch workspace names map
        let all_workspaces = crate::db::operations::list_workspaces(conn).unwrap_or_default();
        let ws_name_map: HashMap<String, String> = all_workspaces
            .into_iter()
            .map(|w| (w.id, w.name))
            .collect();

        // -------------------------------------------------------------------
        // 3. Relevant Related Items & 4. Relationship Strengths
        // -------------------------------------------------------------------
        let raw_relationships = crate::db::operations::get_item_relationships(conn, item_id)
            .unwrap_or_default();

        let effective_min_strength = self.min_relationship_strength.max(policy.min_relationship_strength);
        let effective_max_related_items = self.max_related_items.min(policy.max_related_items);

        let mut filtered_relationships = Vec::new();
        for rel in raw_relationships {
            if rel.strength >= effective_min_strength {
                let other_id = if rel.item_a_id == item.id {
                    rel.item_b_id
                } else {
                    rel.item_a_id
                };
                filtered_relationships.push((other_id, rel.relationship_type, rel.strength));
            }
        }

        // Sort relationships descending by strength, tie-break other_id
        filtered_relationships.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut relationships = Vec::new();
        let mut relevant_related_items = Vec::new();
        let mut semantic_similarity_summaries = Vec::new();
        let mut seen_related_item_ids = HashSet::new();

        for (other_id, rel_type, strength) in &filtered_relationships {
            if relationships.len() < self.max_relationships {
                let formatted_type = rel_type.replace('_', " ");
                relationships.push(ItemRelationshipEvidence {
                    other_item_id: other_id.clone(),
                    other_item_title: None, // populated below if item exists
                    relationship_type: rel_type.clone(),
                    strength: round_to_4_dp(*strength),
                    target_context_id: None,
                    explanation: format!("Strength {:.2} via {}", strength, formatted_type),
                });
            }

            if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
                let other_title = other_item.title.as_deref().map(scrub_private_data);
                let ws_name = other_item.workspace_id.as_ref().and_then(|id| ws_name_map.get(id).cloned());

                // Update relationship details
                if let Some(r) = relationships.iter_mut().find(|r| r.other_item_id == *other_id && r.relationship_type == *rel_type) {
                    r.other_item_title = other_title.clone();
                    r.target_context_id = other_item.workspace_id.clone();
                }

                // Add to relevant related items if not already added
                if seen_related_item_ids.insert(other_id.clone()) && relevant_related_items.len() < effective_max_related_items {
                    let max_snippet = self.max_snippet_length.min(policy.max_snippet_chars);
                    let short_snippet = if other_item.content.chars().count() > max_snippet {
                        let t: String = other_item.content.chars().take(max_snippet).collect();
                        format!("{}...", t)
                    } else {
                        other_item.content.clone()
                    };

                    relevant_related_items.push(RelatedItemEvidence {
                        item_id: other_id.clone(),
                        title: other_title.clone(),
                        item_type: other_item.item_type.clone(),
                        snippet: scrub_private_data(&short_snippet),
                        workspace_id: other_item.workspace_id.clone(),
                        workspace_name: ws_name.clone(),
                        relevance_score: round_to_4_dp(*strength),
                    });
                }

                // If semantic similarity, build human-readable summary (NO raw embeddings!)
                if rel_type == "semantic_similarity" && semantic_similarity_summaries.len() < self.max_semantic_summaries {
                    let band = if *strength >= 0.80 {
                        "Very High"
                    } else if *strength >= 0.60 {
                        "High"
                    } else if *strength >= 0.40 {
                        "Moderate"
                    } else {
                        "Weak"
                    };

                    let title_display = other_title.as_deref().unwrap_or("another item");
                    let ws_display = ws_name.as_deref().unwrap_or("Inbox");
                    let summary = format!(
                        "{} semantic similarity ({:.0}%) with '{}' in {}",
                        band,
                        strength * 100.0,
                        title_display,
                        ws_display
                    );

                    semantic_similarity_summaries.push(SemanticSimilaritySummary {
                        other_item_id: other_id.clone(),
                        other_item_title: other_title,
                        workspace_name: ws_name,
                        similarity_score: round_to_4_dp(*strength),
                        similarity_band: band.to_string(),
                        summary,
                    });
                }
            }
        }

        // -------------------------------------------------------------------
        // 5. Relevant ContextSessions
        // -------------------------------------------------------------------
        let mut relevant_sessions = Vec::new();
        let mut target_session_evidence: Option<SessionEvidence> = None;

        if policy.allow_session_tracking {
            if let Some(sess_id) = &item.session_id {
                let mut sess_stmt = conn.prepare("SELECT workspace_id FROM items WHERE session_id = ?").map_err(|e| e.to_string())?;
                let mut sess_workspaces = HashSet::new();
                let mut count = 0;

                let rows = sess_stmt.query_map([sess_id], |r| r.get::<_, Option<String>>(0)).map_err(|e| e.to_string())?;
                for r in rows.flatten() {
                    count += 1;
                    if let Some(ws_id) = r {
                        if let Some(name) = ws_name_map.get(&ws_id) {
                            sess_workspaces.insert(name.clone());
                        }
                    }
                }

                let mut ws_list: Vec<String> = sess_workspaces.into_iter().collect();
                ws_list.sort();

                let sess_ev = SessionEvidence {
                    session_id: sess_id.clone(),
                    item_count: count,
                    is_target_session: true,
                    associated_workspaces: ws_list,
                };

                target_session_evidence = Some(sess_ev.clone());
                relevant_sessions.push(sess_ev);
            }
        }

        // -------------------------------------------------------------------
        // 6. Workspace Semantic Profile Summaries (NO raw embeddings!)
        // -------------------------------------------------------------------
        let mut workspace_semantic_profiles = Vec::new();
        let item_embedding_opt = crate::db::operations::get_item_embedding(conn, item_id).unwrap_or(None);

        for cand in &candidate_contexts {
            if cand.context_type == "workspace" {
                let ws_id = &cand.context_id;
                let ws_name = &cand.name;

                if let Ok(Some(ws_profile)) = crate::db::operations::get_workspace_embedding(conn, ws_id) {
                    let mut alignment_score = None;
                    let mut alignment_band = None;

                    if let Some(item_emb) = &item_embedding_opt {
                        if item_emb.provider == ws_profile.provider && item_emb.model == ws_profile.model {
                            let sim = crate::semantic::cosine_similarity(&item_emb.embedding, &ws_profile.embedding) as f64;
                            let rounded_sim = round_to_4_dp(sim);
                            alignment_score = Some(rounded_sim);

                            let band = if rounded_sim >= 0.65 {
                                "Strong"
                            } else if rounded_sim >= 0.40 {
                                "Moderate"
                            } else {
                                "Weak"
                            };
                            alignment_band = Some(band.to_string());
                        }
                    }

                    let theme_summary = match alignment_band.as_deref() {
                        Some("Strong") => format!("Aligns strongly with the overall semantic profile of {}.", ws_name),
                        Some("Moderate") => format!("Consistent with the conceptual profile of {}.", ws_name),
                        Some("Weak") => format!("Differs from the primary conceptual focus of {}.", ws_name),
                        _ => format!("Workspace profile exists for {}.", ws_name),
                    };

                    workspace_semantic_profiles.push(WorkspaceProfileSummary {
                        workspace_id: ws_id.clone(),
                        workspace_name: ws_name.clone(),
                        has_profile: true,
                        alignment_score,
                        alignment_band,
                        theme_summary,
                    });
                } else {
                    workspace_semantic_profiles.push(WorkspaceProfileSummary {
                        workspace_id: ws_id.clone(),
                        workspace_name: ws_name.clone(),
                        has_profile: false,
                        alignment_score: None,
                        alignment_band: None,
                        theme_summary: format!("No semantic profile established yet for {}.", ws_name),
                    });
                }
            }
        }

        // -------------------------------------------------------------------
        // 7. Contextual Signals (Curated & Filtered: No PII, No System Dumps)
        // -------------------------------------------------------------------
        let mut contextual_signals = Vec::new();
        for sig in raw_signals {
            if contextual_signals.len() >= self.max_signals {
                break;
            }

            match sig.signal_type.as_str() {
                "active_app" => {
                    if policy.allow_active_app {
                        if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                            if let Some(app) = v.get("app_name").and_then(|a| a.as_str()) {
                                contextual_signals.push(CuratedContextSignal {
                                    signal_type: "active_app".to_string(),
                                    summary: format!("Application: {}", app),
                                });
                            }
                        }
                    }
                }
                "url_metadata" => {
                    if policy.allow_browser_domain {
                        if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                            if let Some(dom) = v.get("domain").and_then(|d| d.as_str()) {
                                contextual_signals.push(CuratedContextSignal {
                                    signal_type: "url_metadata".to_string(),
                                    summary: format!("Domain: {}", dom),
                                });
                            }
                        }
                    }
                }
                "user_action" => {
                    if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                        if let Some(action) = v.get("action").and_then(|a| a.as_str()) {
                            let dest = v.get("destination_workspace").and_then(|d| d.as_str()).and_then(|id| ws_name_map.get(id)).cloned();
                            let summary = if let Some(dname) = dest {
                                format!("User action '{}' associated with workspace '{}'", action, dname)
                            } else {
                                format!("User action '{}'", action)
                            };
                            contextual_signals.push(CuratedContextSignal {
                                signal_type: "user_action".to_string(),
                                summary,
                            });
                        }
                    }
                }
                _ => {} // Strip out unvetted internal system signals
            }
        }

        // Sort signals deterministically
        contextual_signals.sort_by(|a, b| a.signal_type.cmp(&b.signal_type).then_with(|| a.summary.cmp(&b.summary)));

        // -------------------------------------------------------------------
        // 8. Temporal Context
        // -------------------------------------------------------------------
        let decay_config = crate::context_engine::temporal::get_temporal_decay_config(conn);
        let recency_decay_weight = round_to_4_dp(decay_config.compute_weight(&item.created_at, now));

        let age_days = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&item.created_at) {
            let diff = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
            round_to_4_dp((diff.num_seconds() as f64) / 86400.0)
        } else {
            0.0
        };

        let temporal_context = TemporalEvidence {
            age_days: age_days.max(0.0),
            recency_decay_weight,
            is_recent: age_days <= (decay_config.grace_period_hours / 24.0),
        };

        // -------------------------------------------------------------------
        // 9. Deterministic Evidence Assessment (Uncertainty & Ambiguity)
        // -------------------------------------------------------------------
        let active_candidates: Vec<_> = candidate_contexts
            .iter()
            .filter(|c| c.lifecycle_state != "archived")
            .collect();

        let has_actionable_candidate = active_candidates.iter().any(|c| {
            if c.context_type == "workspace" {
                c.deterministic_score >= 0.25
            } else {
                c.item_count > 1 || c.deterministic_score >= 0.60
            }
        });

        let deterministic_assessment = if candidate_contexts.is_empty() {
            DeterministicEvidenceAssessment {
                confidence_band: "none".to_string(),
                is_weak_evidence: true,
                is_ambiguous: false,
                competing_contexts: Vec::new(),
                assessment_summary: "No candidate contexts or matching signals were detected for this item.".to_string(),
            }
        } else if active_candidates.is_empty() {
            let archived_names: Vec<String> = candidate_contexts.iter().map(|c| c.name.clone()).collect();
            DeterministicEvidenceAssessment {
                confidence_band: "weak".to_string(),
                is_weak_evidence: true,
                is_ambiguous: false,
                competing_contexts: Vec::new(),
                assessment_summary: format!(
                    "Matching contexts ({}) are archived by user; contextual signals are suppressed.",
                    archived_names.join(", ")
                ),
            }
        } else if !has_actionable_candidate {
            let top = &active_candidates[0];
            DeterministicEvidenceAssessment {
                confidence_band: "weak".to_string(),
                is_weak_evidence: true,
                is_ambiguous: false,
                competing_contexts: Vec::new(),
                assessment_summary: format!(
                    "Deterministic signals for '{}' are weak or insufficient (< 25% threshold); item remains in Inbox.",
                    top.name
                ),
            }
        } else {
            let top = &active_candidates[0];
            if top.deterministic_score < 0.25 {
                DeterministicEvidenceAssessment {
                    confidence_band: "weak".to_string(),
                    is_weak_evidence: true,
                    is_ambiguous: false,
                    competing_contexts: Vec::new(),
                    assessment_summary: format!(
                        "Deterministic evidence for '{}' is weak ({:.0}% affinity is below the actionable 25% threshold).",
                        top.name,
                        top.deterministic_score * 100.0
                    ),
                }
            } else if active_candidates.len() > 1
                && (top.deterministic_score - active_candidates[1].deterministic_score).abs() <= 0.12
                && active_candidates[1].deterministic_score >= 0.35
            {
                let second = &active_candidates[1];
                DeterministicEvidenceAssessment {
                    confidence_band: "uncertain".to_string(),
                    is_weak_evidence: false,
                    is_ambiguous: true,
                    competing_contexts: vec![top.name.clone(), second.name.clone()],
                    assessment_summary: format!(
                        "Ambiguous signals: competing affinity between '{}' ({:.0}%) and '{}' ({:.0}%).",
                        top.name,
                        top.deterministic_score * 100.0,
                        second.name,
                        second.deterministic_score * 100.0
                    ),
                }
            } else if top.deterministic_score >= 0.70 {
                DeterministicEvidenceAssessment {
                    confidence_band: "high".to_string(),
                    is_weak_evidence: false,
                    is_ambiguous: false,
                    competing_contexts: Vec::new(),
                    assessment_summary: format!(
                        "Strong deterministic evidence for '{}' ({:.0}% affinity).",
                        top.name,
                        top.deterministic_score * 100.0
                    ),
                }
            } else {
                DeterministicEvidenceAssessment {
                    confidence_band: "moderate".to_string(),
                    is_weak_evidence: false,
                    is_ambiguous: false,
                    competing_contexts: Vec::new(),
                    assessment_summary: format!(
                        "Moderate deterministic evidence for '{}' ({:.0}% affinity).",
                        top.name,
                        top.deterministic_score * 100.0
                    ),
                }
            }
        };

        // -------------------------------------------------------------------
        // 10. System Metadata & Excluded Metrics
        // -------------------------------------------------------------------
        let included_items = 1 + relevant_related_items.len();
        let total_db_items_excluded = total_db_items.saturating_sub(included_items);

        let system_metadata = EvidenceMetadata {
            generated_at: now.to_rfc3339(),
            engine_version: "ContextSplit-v0.1.0".to_string(),
            total_db_items_excluded,
            privacy_boundary: "ai_privacy_boundary:strict_curated_evidence_only:v1".to_string(),
        };

        let mut package = EvidencePackage {
            target_item,
            candidate_contexts,
            relevant_related_items,
            relationships,
            semantic_similarity_summaries,
            relevant_sessions,
            session_context: target_session_evidence,
            workspace_semantic_profiles,
            contextual_signals,
            temporal_context,
            deterministic_assessment,
            system_metadata,
        };

        boundary.audit_and_sanitize(&mut package).map_err(|e| e.to_string())?;

        Ok(package)
    }

    /// Constructs an EvidencePackage representing a group of candidate items for AI naming.
    pub fn build_for_item_group(&self, conn: &Connection, item_ids: &[String]) -> Result<EvidencePackage, String> {
        if item_ids.is_empty() {
            return Err("Cannot build EvidencePackage for empty item group".to_string());
        }

        // The primary item is item_ids[0]
        let primary_id = &item_ids[0];
        let mut package = self.build(conn, primary_id)?;

        // Ensure all other items in the group are added into relevant_related_items if not already present
        let existing_ids: HashSet<String> = package
            .relevant_related_items
            .iter()
            .map(|r| r.item_id.clone())
            .collect();

        for other_id in item_ids.iter().skip(1) {
            if !existing_ids.contains(other_id) {
                if let Ok(other_item) = crate::db::operations::get_item(conn, other_id) {
                    let raw_snippet = match other_item.item_type.as_str() {
                        "image" => "[Image data excluded]".to_string(),
                        "file" => {
                            if let Ok(meta_val) = serde_json::from_str::<Value>(&other_item.metadata) {
                                let fname = meta_val.get("filename").and_then(|v| v.as_str()).unwrap_or("unnamed_file");
                                format!("[File: {}]", fname)
                            } else {
                                "[File content excluded]".to_string()
                            }
                        }
                        _ => {
                            let trimmed = other_item.content.trim();
                            if trimmed.chars().count() > self.max_snippet_length {
                                let truncated: String = trimmed.chars().take(self.max_snippet_length).collect();
                                format!("{}...", truncated)
                            } else {
                                trimmed.to_string()
                            }
                        }
                    };
                    package.relevant_related_items.push(super::models::RelatedItemEvidence {
                        item_id: other_item.id.clone(),
                        title: other_item.title.as_deref().map(scrub_private_data),
                        item_type: other_item.item_type.clone(),
                        snippet: scrub_private_data(&raw_snippet),
                        workspace_id: other_item.workspace_id.clone(),
                        workspace_name: None,
                        relevance_score: 1.0,
                    });
                }
            }
        }

        // Fetch signals for all items in the group
        for id in item_ids.iter().skip(1) {
            let item_signals = crate::context_engine::operations::list_context_signals(conn, None, Some(id)).unwrap_or_default();
            for sig in item_signals {
                if sig.signal_type == "active_app" {
                    if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                        if let Some(app) = v.get("app_name").and_then(|a| a.as_str()) {
                            package.contextual_signals.push(super::models::CuratedContextSignal {
                                signal_type: "active_app".to_string(),
                                summary: format!("Application: {}", app),
                            });
                        }
                    }
                } else if sig.signal_type == "url_metadata" {
                    if let Ok(v) = serde_json::from_str::<Value>(&sig.payload) {
                        if let Some(d) = v.get("domain").and_then(|d| d.as_str()) {
                            package.contextual_signals.push(super::models::CuratedContextSignal {
                                signal_type: "url_metadata".to_string(),
                                summary: format!("Domain: {}", d),
                            });
                        }
                    }
                }
            }
        }

        Ok(package)
    }

    /// Constructs an EvidencePackage representing a discovered ContextCandidate.
    pub fn build_for_candidate(&self, conn: &Connection, candidate_id: &str) -> Result<EvidencePackage, String> {
        let mut stmt = conn
            .prepare("SELECT item_id FROM context_candidate_items WHERE candidate_id = ?1 ORDER BY relevance_score DESC")
            .map_err(|e| e.to_string())?;
        let item_ids: Vec<String> = stmt
            .query_map(rusqlite::params![candidate_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        if item_ids.is_empty() {
            return Err(format!("ContextCandidate '{}' has no associated items", candidate_id));
        }

        self.build_for_item_group(conn, &item_ids)
    }
}

/// Helper function to round floating point values to 4 decimal places for determinism.
fn round_to_4_dp(val: f64) -> f64 {
    (val * 10000.0).round() / 10000.0
}

/// Convenience builder function using default builder parameters.
pub fn build_evidence_package(conn: &Connection, item_id: &str) -> Result<EvidencePackage, String> {
    EvidencePackageBuilder::default().build(conn, item_id)
}

/// Convenience builder function for candidate items.
pub fn build_candidate_evidence_package(conn: &Connection, candidate_id: &str) -> Result<EvidencePackage, String> {
    EvidencePackageBuilder::default().build_for_candidate(conn, candidate_id)
}

/// Convenience builder function for item groups.
pub fn build_item_group_evidence_package(conn: &Connection, item_ids: &[String]) -> Result<EvidencePackage, String> {
    EvidencePackageBuilder::default().build_for_item_group(conn, item_ids)
}

/// Advisory reasoning invocation.
/// Builds the EvidencePackage and executes the provided AIReasoner.
/// Returns an advisory interpretation without mutating any database records or state.
pub fn analyze_item_context_ai(
    conn: &Connection,
    item_id: &str,
    reasoner: &dyn AIReasoner,
) -> Result<ContextReasoningResult, String> {
    let package = build_evidence_package(conn, item_id)?;
    let result = reasoner.analyze_context(&package).map_err(|e| e.to_string())?;
    super::validator::validate_explanation_grounding(&result, &package).map_err(|e| e.to_string())?;
    Ok(result)
}

/// AI-assisted naming for a ContextCandidate.
/// If AI reasoning fails, times out, or is disabled, this returns an Err so caller can fallback to deterministic name.
pub fn suggest_candidate_name_ai(
    conn: &Connection,
    candidate_id: &str,
    reasoner: &dyn AIReasoner,
) -> Result<String, String> {
    let package = build_candidate_evidence_package(conn, candidate_id)?;
    reasoner.suggest_candidate_name(&package).map_err(|e| e.to_string())
}

/// AI-assisted naming for an arbitrary group of items.
/// If AI reasoning fails, times out, or is disabled, this returns an Err so caller can fallback to deterministic name.
pub fn suggest_item_group_name_ai(
    conn: &Connection,
    item_ids: &[String],
    reasoner: &dyn AIReasoner,
) -> Result<String, String> {
    let package = build_item_group_evidence_package(conn, item_ids)?;
    reasoner.suggest_candidate_name(&package).map_err(|e| e.to_string())
}

/// Graceful AI analysis that **never propagates a hard error**.
///
/// When AI reasoning is unavailable for any reason (disabled, network failure,
/// timeout, malformed response, rate limit, missing evidence), this function
/// returns a structured `AIContextReasoningResponse` with `ai_unavailable = true`
/// and a human-readable reason.
///
/// Deterministic Context Engine features are unaffected in all failure cases.
pub fn analyze_item_context_ai_graceful(
    conn: &Connection,
    item_id: &str,
    reasoner: &dyn AIReasoner,
) -> AIContextReasoningResponse {
    // Step 1: Check if AI reasoning is disabled in user settings.
    let settings = crate::db::settings::get_settings(conn).unwrap_or_default();
    if !settings.enable_ai_reasoning {
        return AIContextReasoningResponse {
            ai_result: None,
            ai_unavailable: true,
            unavailability_reason: Some(AIFailureReason::Disabled.to_string()),
            failure_reason: Some(AIFailureReason::Disabled),
        };
    }

    // Step 2: Check if provider is available.
    if !reasoner.is_available() {
        return AIContextReasoningResponse {
            ai_result: None,
            ai_unavailable: true,
            unavailability_reason: Some(AIFailureReason::ProviderUnavailable.to_string()),
            failure_reason: Some(AIFailureReason::ProviderUnavailable),
        };
    }

    // Step 3: Build EvidencePackage — treat build failure as EvidenceUnavailable.
    let package = match build_evidence_package(conn, item_id) {
        Ok(p) => p,
        Err(build_err) => {
            // If the error message says AI is disabled via privacy boundary, classify correctly.
            let reason = if build_err.to_lowercase().contains("ai reasoning is disabled")
                || build_err.to_lowercase().contains("disabled")
            {
                AIFailureReason::Disabled
            } else {
                AIFailureReason::EvidenceUnavailable
            };
            return AIContextReasoningResponse {
                ai_result: None,
                ai_unavailable: true,
                unavailability_reason: Some(format!("{}: {}", reason, build_err)),
                failure_reason: Some(reason),
            };
        }
    };

    // Step 4: Run AI inference — map every error variant to AIFailureReason.
    let reasoning_result = match reasoner.analyze_context(&package) {
        Ok(r) => r,
        Err(ai_err) => {
            let reason = AIFailureReason::from_reasoning_error(&ai_err);
            return AIContextReasoningResponse {
                ai_result: None,
                ai_unavailable: true,
                unavailability_reason: Some(format!("{}", reason)),
                failure_reason: Some(reason),
            };
        }
    };

    // Step 5: Post-inference validation (grounding check).
    if let Err(validation_err) = super::validator::validate_explanation_grounding(&reasoning_result, &package) {
        let reason = AIFailureReason::MalformedResponse;
        return AIContextReasoningResponse {
            ai_result: None,
            ai_unavailable: true,
            unavailability_reason: Some(format!("{}: {}", reason, validation_err)),
            failure_reason: Some(reason),
        };
    }

    // All checks passed — return the successful AI result.
    AIContextReasoningResponse {
        ai_result: Some(reasoning_result),
        ai_unavailable: false,
        unavailability_reason: None,
        failure_reason: None,
    }
}
