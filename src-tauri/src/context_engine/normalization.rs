use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::context_engine::evidence::{
    ContextEvidence, ContextEvidenceSet, EvidenceCategory, EvidenceProvenance,
};

/// Specific configuration for normalizing an individual evidence category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategoryNormalizationConfig {
    /// Baseline strength scale/multiplier to bring category into a comparable range [0.0, 1.0].
    pub base_multiplier: f32,
    /// Maximum individual and aggregate cap for this category or correlation group.
    pub max_cap: f32,
    /// Diminishing returns factor applied to successive correlated instances (0.0 to 1.0).
    pub diminishing_factor: f32,
}

impl CategoryNormalizationConfig {
    pub fn new(base_multiplier: f32, max_cap: f32, diminishing_factor: f32) -> Self {
        Self {
            base_multiplier: base_multiplier.clamp(0.0, 1.0),
            max_cap: max_cap.clamp(0.0, 1.0),
            diminishing_factor: diminishing_factor.clamp(0.0, 1.0),
        }
    }
}

/// Comprehensive configuration for `EvidenceNormalizer`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizerConfig {
    /// Category-specific normalization parameters
    pub category_configs: HashMap<EvidenceCategory, CategoryNormalizationConfig>,
    /// Global cap applied when only weak peripheral signals are present
    pub weak_signals_isolated_cap: f32,
    /// Whether conflict detection between feedback and inferred evidence is enabled
    pub enable_conflict_detection: bool,
}

impl Default for NormalizerConfig {
    fn default() -> Self {
        let mut category_configs = HashMap::new();

        // 1. Confirmed Membership: Authoritative user assignment (Very Strong)
        category_configs.insert(
            EvidenceCategory::ConfirmedMembership,
            CategoryNormalizationConfig::new(1.0, 1.0, 0.90),
        );

        // 2. User Feedback: Direct user actions / dismissals / confirmations (Very Strong)
        category_configs.insert(
            EvidenceCategory::UserFeedback,
            CategoryNormalizationConfig::new(0.95, 1.0, 0.85),
        );

        // 3. Same Session: Temporal co-occurrence in an active session (Meaningful)
        category_configs.insert(
            EvidenceCategory::SameSession,
            CategoryNormalizationConfig::new(0.65, 0.75, 0.40),
        );

        // 4. Semantic Similarity: Item embedding cosine similarity (Meaningful when high)
        category_configs.insert(
            EvidenceCategory::SemanticSimilarity,
            CategoryNormalizationConfig::new(0.70, 0.80, 0.50),
        );

        // 5. Workspace History: Past user organization patterns with temporal decay (Meaningful)
        category_configs.insert(
            EvidenceCategory::WorkspaceHistory,
            CategoryNormalizationConfig::new(0.60, 0.70, 0.50),
        );

        // 6. Related Item: Graph connections and co-references (Meaningful)
        category_configs.insert(
            EvidenceCategory::RelatedItem,
            CategoryNormalizationConfig::new(0.55, 0.65, 0.45),
        );

        // 7. Browser Domain: Hostname / origin (Contextual rather than decisive)
        category_configs.insert(
            EvidenceCategory::BrowserDomain,
            CategoryNormalizationConfig::new(0.30, 0.35, 0.35),
        );

        // 8. Source App: Application process / bundle (Weaker)
        category_configs.insert(
            EvidenceCategory::SourceApp,
            CategoryNormalizationConfig::new(0.25, 0.30, 0.30),
        );

        // 9. Browser URL: URL path / query string (Contextual)
        category_configs.insert(
            EvidenceCategory::BrowserUrl,
            CategoryNormalizationConfig::new(0.30, 0.35, 0.35),
        );

        // 10. Temporal Proximity: Raw time closeness (Weak)
        category_configs.insert(
            EvidenceCategory::TemporalProximity,
            CategoryNormalizationConfig::new(0.20, 0.25, 0.25),
        );

        // 11. Source Window: Window title (Weak / Contextual)
        category_configs.insert(
            EvidenceCategory::SourceWindow,
            CategoryNormalizationConfig::new(0.20, 0.25, 0.30),
        );

        // 12. File Metadata: File extension, mime type, size (Minor)
        category_configs.insert(
            EvidenceCategory::FileMetadata,
            CategoryNormalizationConfig::new(0.15, 0.20, 0.25),
        );

        // 13. Content Type: Text, code, url, image (Minor)
        category_configs.insert(
            EvidenceCategory::ContentType,
            CategoryNormalizationConfig::new(0.10, 0.15, 0.20),
        );

        // 14. Capture Method: Clipboard, drop, notch, manual (Minor)
        category_configs.insert(
            EvidenceCategory::CaptureMethod,
            CategoryNormalizationConfig::new(0.10, 0.15, 0.20),
        );

        Self {
            category_configs,
            weak_signals_isolated_cap: 0.24,
            enable_conflict_detection: true,
        }
    }
}

/// A single piece of evidence normalized into a consistent, comparable scale with
/// tracking of deduplication, correlation grouping, saturation caps, and conflict status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedEvidence {
    /// The underlying contextual evidence record
    pub evidence: ContextEvidence,
    /// Original raw strength before normalization
    pub raw_strength: f64,
    /// Calibrated comparable strength [0.0, 1.0] for this category
    pub normalized_strength: f64,
    /// Effective weight after deduplication, correlation grouping, and saturation capping
    pub effective_weight: f64,
    /// Logical correlation group (e.g., "domain:figma.com", "app:Figma", "session:s1")
    pub correlation_group: Option<String>,
    /// Number of identical duplicate signals collapsed into this record
    pub duplicate_count: usize,
    /// Whether this evidence's contribution was capped by saturation limits
    pub is_capped: bool,
    /// Whether this evidence directly conflicts with user feedback or authoritative facts
    pub is_conflicting: bool,
    /// Explanatory notes detailing the normalization and capping decisions
    pub normalization_notes: Vec<String>,
}

impl NormalizedEvidence {
    pub fn id(&self) -> &str {
        &self.evidence.id
    }

    pub fn item_id(&self) -> &str {
        &self.evidence.target_item_id
    }

    pub fn category(&self) -> EvidenceCategory {
        self.evidence.category
    }

    pub fn provenance(&self) -> EvidenceProvenance {
        self.evidence.provenance
    }

    pub fn is_observed(&self) -> bool {
        self.evidence.is_observed()
    }

    pub fn is_inferred(&self) -> bool {
        self.evidence.is_inferred()
    }
}

/// Record of an identified conflict between two or more evidence records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceConflict {
    /// Category of conflict (e.g. "user_feedback_vs_inferred", "conflicting_confirmed_memberships")
    pub conflict_type: String,
    /// Human-readable explanation of why the evidence pieces conflict
    pub description: String,
    /// Identifiers of the opposing evidence records
    pub opposing_evidence_ids: Vec<String>,
    /// Severity or negative penalty associated with this conflict
    pub severity: f32,
}

/// Normalized collection of contextual evidence for an Item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NormalizedEvidenceSet {
    /// Target Item identifier
    pub item_id: String,
    /// All normalized, deduplicated, and capped evidence items
    pub items: Vec<NormalizedEvidence>,
    /// Any conflicting evidence pairs detected during normalization
    pub conflicts: Vec<EvidenceConflict>,
    /// Total effective weight aggregated across all normalized items
    pub total_effective_weight: f64,
}

impl NormalizedEvidenceSet {
    pub fn new(item_id: impl Into<String>) -> Self {
        Self {
            item_id: item_id.into(),
            items: Vec::new(),
            conflicts: Vec::new(),
            total_effective_weight: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn by_category(&self, category: EvidenceCategory) -> Vec<&NormalizedEvidence> {
        self.items.iter().filter(|e| e.category() == category).collect()
    }

    pub fn by_correlation_group(&self, group: &str) -> Vec<&NormalizedEvidence> {
        self.items
            .iter()
            .filter(|e| e.correlation_group.as_deref() == Some(group))
            .collect()
    }

    pub fn observed_items(&self) -> Vec<&NormalizedEvidence> {
        self.items.iter().filter(|e| e.is_observed()).collect()
    }

    pub fn inferred_items(&self) -> Vec<&NormalizedEvidence> {
        self.items.iter().filter(|e| e.is_inferred()).collect()
    }

    pub fn effective_weight_by_category(&self, category: EvidenceCategory) -> f64 {
        self.items
            .iter()
            .filter(|e| e.category() == category && !e.is_conflicting)
            .map(|e| e.effective_weight)
            .sum()
    }

    pub fn effective_weight_for_workspace(&self, workspace_id: &str) -> f64 {
        self.items
            .iter()
            .filter(|e| {
                if e.is_conflicting {
                    return false;
                }
                // Dismissal feedback is inhibitory, not positive workspace support
                if e.category() == EvidenceCategory::UserFeedback {
                    if let Some(fb) = e.evidence.attributes.get("feedback_type").and_then(|v| v.as_str()) {
                        if fb == "user_dismissal" || fb.contains("dismiss") || fb.contains("reject") {
                            return false;
                        }
                    }
                }
                let target_ws = e.evidence.attributes.get("workspace_id").and_then(|v| v.as_str())
                    .or_else(|| e.evidence.attributes.get("workspace_name").and_then(|v| v.as_str()))
                    .or_else(|| e.evidence.source_ref.as_ref().filter(|r| r.entity_type == crate::context_engine::evidence::EntityType::Workspace).map(|r| r.entity_id.as_str()));
                target_ws == Some(workspace_id)
            })
            .map(|e| e.effective_weight)
            .sum()
    }

    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// Pure, deterministic normalizer service for `ContextEvidence`.
///
/// Ensures:
/// 1. Evidence strengths across disparate sources are calibrated into a consistent, comparable range.
/// 2. Weak generic signals (app name, domain, content type) cannot dominate strong signals (membership, feedback, session, semantics).
/// 3. Identical duplicate signals are deduplicated.
/// 4. Correlated signals from the same underlying source (e.g. 10 Figma URLs) are grouped and saturated with diminishing marginal returns.
/// 5. Contradictory evidence (e.g. user dismissal vs inferred affinity) is flagged.
#[derive(Debug, Clone)]
pub struct EvidenceNormalizer {
    pub config: NormalizerConfig,
}

impl Default for EvidenceNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl EvidenceNormalizer {
    pub fn new() -> Self {
        Self {
            config: NormalizerConfig::default(),
        }
    }

    pub fn with_config(config: NormalizerConfig) -> Self {
        Self { config }
    }

    /// Normalizes an entire `ContextEvidenceSet`.
    pub fn normalize(&self, evidence_set: &ContextEvidenceSet) -> NormalizedEvidenceSet {
        self.normalize_slice(&evidence_set.target_item_id, &evidence_set.items)
    }

    /// Normalizes a slice of `ContextEvidence` records for a given item.
    pub fn normalize_slice(&self, item_id: &str, evidences: &[ContextEvidence]) -> NormalizedEvidenceSet {
        let mut result = NormalizedEvidenceSet::new(item_id);
        if evidences.is_empty() {
            return result;
        }

        // ---------------------------------------------------------------------
        // Step 1: Deduplication (Collapse exact duplicate evidence)
        // ---------------------------------------------------------------------
        struct DedupAccumulator {
            evidence: ContextEvidence,
            count: usize,
            max_raw_strength: f64,
        }

        let mut dedup_map: HashMap<String, DedupAccumulator> = HashMap::new();
        // Maintain deterministic order
        let mut dedup_keys: Vec<String> = Vec::new();

        for ev in evidences {
            let key = self.compute_dedup_key(ev);
            if let Some(acc) = dedup_map.get_mut(&key) {
                acc.count += 1;
                if ev.strength > acc.max_raw_strength {
                    acc.max_raw_strength = ev.strength;
                    acc.evidence = ev.clone();
                }
            } else {
                dedup_keys.push(key.clone());
                dedup_map.insert(
                    key,
                    DedupAccumulator {
                        evidence: ev.clone(),
                        count: 1,
                        max_raw_strength: ev.strength,
                    },
                );
            }
        }

        // ---------------------------------------------------------------------
        // Step 2: Category Calibration & Correlation Grouping
        // ---------------------------------------------------------------------
        let default_cfg = CategoryNormalizationConfig::new(0.5, 0.5, 0.5);

        // Track group saturation totals: group_key -> accumulated_effective_weight
        let mut group_accumulated_weight: HashMap<String, f64> = HashMap::new();
        let mut group_instance_index: HashMap<String, usize> = HashMap::new();

        let mut normalized_items: Vec<NormalizedEvidence> = Vec::new();

        for key in &dedup_keys {
            let acc = match dedup_map.get(key) {
                Some(a) => a,
                None => continue,
            };

            let ev = &acc.evidence;
            let cat = ev.category;
            let cat_cfg = self.config.category_configs.get(&cat).unwrap_or(&default_cfg);

            // Calibrate raw strength into comparable normalized strength [0.0, 1.0]
            let normalized_strength = (acc.max_raw_strength * cat_cfg.base_multiplier as f64).clamp(0.0, 1.0);

            // Determine correlation group key
            let correlation_group = self.determine_correlation_group(ev);
            let group_key = correlation_group.clone().unwrap_or_else(|| format!("cat:{:?}", cat));

            let instance_idx = *group_instance_index.get(&group_key).unwrap_or(&0);
            group_instance_index.insert(group_key.clone(), instance_idx + 1);

            // Apply diminishing returns formula for correlated instances:
            // marginal_gain = normalized_strength * (diminishing_factor ^ instance_idx)
            let decay = (cat_cfg.diminishing_factor as f64).powi(instance_idx as i32);
            let marginal_gain = normalized_strength * decay;

            // Apply category / group saturation cap
            let current_group_total = *group_accumulated_weight.get(&group_key).unwrap_or(&0.0);
            let max_cap = cat_cfg.max_cap as f64;

            let (effective_weight, is_capped) = if current_group_total + marginal_gain > max_cap {
                let allowed = (max_cap - current_group_total).max(0.0);
                (allowed, true)
            } else {
                (marginal_gain, false)
            };

            group_accumulated_weight.insert(group_key.clone(), current_group_total + effective_weight);

            let mut notes = Vec::new();
            if acc.count > 1 {
                notes.push(format!("Collapsed {} identical duplicate signals", acc.count));
            }
            if instance_idx > 0 {
                notes.push(format!(
                    "Correlated instance #{} in group '{}'; decayed by factor {:.2}",
                    instance_idx + 1,
                    group_key,
                    decay
                ));
            }
            if is_capped {
                notes.push(format!(
                    "Capped at saturation limit {:.2} for category/group '{}'",
                    max_cap, group_key
                ));
            }

            normalized_items.push(NormalizedEvidence {
                evidence: ev.clone(),
                raw_strength: acc.max_raw_strength,
                normalized_strength,
                effective_weight,
                correlation_group,
                duplicate_count: acc.count,
                is_capped,
                is_conflicting: false, // will be evaluated in Step 3
                normalization_notes: notes,
            });
        }

        // ---------------------------------------------------------------------
        // Step 3: Conflict & Contradiction Detection
        // ---------------------------------------------------------------------
        let mut conflicts: Vec<EvidenceConflict> = Vec::new();

        if self.config.enable_conflict_detection {
            // Check for UserFeedback dismissals contradicting inferred affinity
            let dismissals: Vec<(String, String, Option<String>)> = normalized_items
                .iter()
                .filter(|n| {
                    n.category() == EvidenceCategory::UserFeedback
                        && n.evidence.attributes.get("feedback_type").and_then(|v| v.as_str())
                            == Some("user_dismissal")
                })
                .filter_map(|n| {
                    let ws_id = n
                        .evidence
                        .attributes
                        .get("workspace_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| n.evidence.source_ref.as_ref().map(|r| r.entity_id.as_str()))?;
                    let ws_name = n
                        .evidence
                        .attributes
                        .get("workspace_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    Some((n.id().to_string(), ws_id.to_string(), ws_name))
                })
                .collect();

            for (dismissal_id, ws_id, ws_name) in dismissals {
                // Find any inferred or weak evidence proposing the dismissed workspace
                for item in normalized_items.iter_mut() {
                    if item.id() == dismissal_id {
                        continue;
                    }

                    let target_ws_id = item
                        .evidence
                        .attributes
                        .get("workspace_id")
                        .and_then(|v| v.as_str())
                        .or_else(|| item.evidence.source_ref.as_ref().map(|r| r.entity_id.as_str()));
                    let target_ws_name = item
                        .evidence
                        .attributes
                        .get("workspace_name")
                        .and_then(|v| v.as_str());

                    let is_match = (target_ws_id.is_some() && target_ws_id == Some(ws_id.as_str()))
                        || (ws_name.is_some() && target_ws_name.is_some() && target_ws_name == ws_name.as_deref())
                        || (ws_name.is_some() && target_ws_id.is_some() && target_ws_id == ws_name.as_deref())
                        || (target_ws_name.is_some() && target_ws_name == Some(ws_id.as_str()));

                    if is_match && item.category() != EvidenceCategory::UserFeedback {
                        item.is_conflicting = true;
                        item.normalization_notes.push(format!(
                            "Conflicts with user dismissal of workspace '{}'",
                            ws_id
                        ));

                        conflicts.push(EvidenceConflict {
                            conflict_type: "user_feedback_vs_inferred".to_string(),
                            description: format!(
                                "User previously dismissed workspace '{}', conflicting with inferred {} evidence",
                                ws_id,
                                item.category().as_str()
                            ),
                            opposing_evidence_ids: vec![dismissal_id.clone(), item.id().to_string()],
                            severity: 0.80,
                        });
                    }
                }
            }

            // Check for Conflicting Confirmed Memberships
            let confirmed_memberships: Vec<_> = normalized_items
                .iter()
                .filter(|n| n.category() == EvidenceCategory::ConfirmedMembership)
                .collect();

            if confirmed_memberships.len() > 1 {
                let distinct_workspaces: HashSet<_> = confirmed_memberships
                    .iter()
                    .filter_map(|m| {
                        m.evidence.attributes.get("workspace_id").and_then(|v| v.as_str())
                            .or_else(|| m.evidence.source_ref.as_ref().map(|r| r.entity_id.as_str()))
                    })
                    .collect();

                if distinct_workspaces.len() > 1 {
                    conflicts.push(EvidenceConflict {
                        conflict_type: "conflicting_confirmed_memberships".to_string(),
                        description: format!(
                            "Item has conflicting confirmed memberships across multiple workspaces: {:?}",
                            distinct_workspaces
                        ),
                        opposing_evidence_ids: confirmed_memberships.iter().map(|m| m.id().to_string()).collect(),
                        severity: 1.0,
                    });
                }
            }
        }

        // ---------------------------------------------------------------------
        // Step 4: Aggregate Total Effective Weight
        // ---------------------------------------------------------------------
        let total_effective_weight: f64 = normalized_items
            .iter()
            .filter(|n| !n.is_conflicting)
            .map(|n| n.effective_weight)
            .sum();

        result.items = normalized_items;
        result.conflicts = conflicts;
        result.total_effective_weight = total_effective_weight;

        result
    }

    /// Computes a deterministic deduplication key for an evidence record.
    fn compute_dedup_key(&self, ev: &ContextEvidence) -> String {
        let mut key = format!("cat:{}:", ev.category.as_str());

        if let Some(ref r) = ev.source_ref {
            key.push_str(&format!("ref:{}:{}:", r.entity_type.as_str(), r.entity_id));
        }

        match ev.category {
            EvidenceCategory::BrowserDomain => {
                if let Some(domain) = ev.attributes.get("domain").and_then(|v| v.as_str()) {
                    key.push_str(&format!("dom:{}", domain.to_lowercase()));
                }
                if let Some(url) = ev.attributes.get("url").or_else(|| ev.attributes.get("url_origin")).and_then(|v| v.as_str()) {
                    key.push_str(&format!("url:{}", url));
                }
            }
            EvidenceCategory::BrowserUrl => {
                if let Some(url) = ev.attributes.get("url").or_else(|| ev.attributes.get("url_origin")).and_then(|v| v.as_str()) {
                    key.push_str(&format!("url:{}", url));
                }
            }
            EvidenceCategory::SourceApp => {
                if let Some(app) = ev.attributes.get("app_name").and_then(|v| v.as_str()) {
                    key.push_str(&format!("app:{}", app.to_lowercase()));
                }
            }
            EvidenceCategory::SameSession => {
                if let Some(session_id) = ev.attributes.get("session_id").and_then(|v| v.as_str()) {
                    key.push_str(&format!("sess:{}", session_id));
                }
            }
            EvidenceCategory::RelatedItem | EvidenceCategory::TemporalProximity => {
                if let Some(other_id) = ev.attributes.get("other_item_id").and_then(|v| v.as_str()) {
                    key.push_str(&format!("other:{}", other_id));
                }
            }
            EvidenceCategory::WorkspaceHistory | EvidenceCategory::ConfirmedMembership => {
                if let Some(ws_id) = ev.attributes.get("workspace_id").and_then(|v| v.as_str()) {
                    key.push_str(&format!("ws:{}", ws_id));
                }
            }
            EvidenceCategory::ContentType => {
                if let Some(c_type) = ev.attributes.get("content_type").and_then(|v| v.as_str()) {
                    key.push_str(&format!("type:{}", c_type));
                }
            }
            EvidenceCategory::UserFeedback => {
                if let Some(ws_id) = ev.attributes.get("workspace_id").and_then(|v| v.as_str()) {
                    key.push_str(&format!("ws:{}", ws_id));
                }
                if let Some(fb_type) = ev.attributes.get("feedback_type").and_then(|v| v.as_str()) {
                    key.push_str(&format!("fb:{}", fb_type));
                }
            }
            _ => {
                key.push_str(&ev.id);
            }
        }

        key
    }

    /// Determines the correlation group for an evidence record (e.g. "domain:figma.com", "app:figma").
    fn determine_correlation_group(&self, ev: &ContextEvidence) -> Option<String> {
        match ev.category {
            EvidenceCategory::BrowserDomain => {
                ev.attributes.get("domain")
                    .and_then(|v| v.as_str())
                    .map(|d| format!("domain:{}", d.to_lowercase()))
            }
            EvidenceCategory::BrowserUrl => {
                if let Some(domain) = ev.attributes.get("domain").and_then(|v| v.as_str()) {
                    Some(format!("domain:{}", domain.to_lowercase()))
                } else if let Some(url) = ev.attributes.get("url").and_then(|v| v.as_str()) {
                    if let Ok(parsed) = url::Url::parse(url) {
                        parsed.host_str().map(|h| format!("domain:{}", h.to_lowercase()))
                    } else {
                        Some("domain:general".to_string())
                    }
                } else {
                    Some("domain:general".to_string())
                }
            }
            EvidenceCategory::SourceApp => {
                ev.attributes.get("app_name")
                    .and_then(|v| v.as_str())
                    .map(|a| format!("app:{}", a.to_lowercase()))
            }
            EvidenceCategory::SameSession => {
                ev.attributes.get("session_id")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("session:{}", s))
            }
            EvidenceCategory::TemporalProximity => {
                Some("temporal_proximity".to_string())
            }
            EvidenceCategory::ContentType => {
                ev.attributes.get("content_type")
                    .and_then(|v| v.as_str())
                    .map(|c| format!("content_type:{}", c))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::evidence::ContextEvidence;

    #[test]
    fn test_normalization_strength_comparability() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_comp";

        // Confirmed membership (authoritative)
        let ev_confirmed = ContextEvidence::observed_confirmed_membership(item_id, "ws_alpha", Some("Alpha"), None);
        // Direct user feedback (authoritative)
        let ev_feedback = ContextEvidence::observed_user_dismissal(item_id, "ws_beta", Some("Beta"), None);
        // Same session (meaningful)
        let ev_session = ContextEvidence::observed_session(item_id, "sess_1", None);
        // Semantic similarity (meaningful)
        let ev_semantic = ContextEvidence::inferred_semantic_similarity(item_id, "other_1", 0.90, Some("Note"), Some("Alpha"), None);
        // Source app (weak)
        let ev_app = ContextEvidence::observed_app(item_id, "VSCode", None);
        // Browser domain (contextual)
        let ev_domain = ContextEvidence::observed_domain(item_id, "github.com", None, None);
        // Temporal proximity (weak)
        let ev_temporal = ContextEvidence::inferred_temporal_proximity(item_id, "other_2", 30, 0.90, None);

        let evidences = vec![
            ev_confirmed,
            ev_feedback,
            ev_session,
            ev_semantic,
            ev_app,
            ev_domain,
            ev_temporal,
        ];

        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        let norm_confirmed = normalized_set.by_category(EvidenceCategory::ConfirmedMembership)[0];
        let norm_feedback = normalized_set.by_category(EvidenceCategory::UserFeedback)[0];
        let norm_session = normalized_set.by_category(EvidenceCategory::SameSession)[0];
        let norm_semantic = normalized_set.by_category(EvidenceCategory::SemanticSimilarity)[0];
        let norm_app = normalized_set.by_category(EvidenceCategory::SourceApp)[0];
        let norm_domain = normalized_set.by_category(EvidenceCategory::BrowserDomain)[0];
        let norm_temporal = normalized_set.by_category(EvidenceCategory::TemporalProximity)[0];

        // 1. Confirmed membership and feedback are very strong (>= 0.90)
        assert!(norm_confirmed.normalized_strength >= 0.95);
        assert!(norm_feedback.normalized_strength >= 0.90);

        // 2. Session and high semantic similarity are meaningful (>= 0.60)
        assert!(norm_session.normalized_strength >= 0.60);
        assert!(norm_semantic.normalized_strength >= 0.60);

        // 3. App, domain, temporal are weak / contextual (<= 0.35)
        assert!(norm_domain.normalized_strength <= 0.35);
        assert!(norm_app.normalized_strength <= 0.30);
        assert!(norm_temporal.normalized_strength <= 0.25);

        // 4. Strict relative ordering: Confirmed > Semantic > Domain > App > Temporal
        assert!(norm_confirmed.normalized_strength > norm_semantic.normalized_strength);
        assert!(norm_semantic.normalized_strength > norm_domain.normalized_strength);
        assert!(norm_domain.normalized_strength > norm_app.normalized_strength);
        assert!(norm_app.normalized_strength > norm_temporal.normalized_strength);
    }

    #[test]
    fn test_duplicate_evidence_deduplication() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_dedup";

        // Create 5 identical source app signals for "Figma"
        let mut evidences = Vec::new();
        for _ in 0..5 {
            evidences.push(ContextEvidence::observed_app(item_id, "Figma", None));
        }

        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        // Deduplication must collapse the 5 identical records into 1
        assert_eq!(normalized_set.items.len(), 1);
        let item = &normalized_set.items[0];
        assert_eq!(item.duplicate_count, 5);
        assert!(item.normalization_notes.iter().any(|n| n.contains("Collapsed 5 identical duplicate signals")));
    }

    #[test]
    fn test_correlated_evidence_saturation_capping() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_figma_urls";

        // Create 10 distinct Figma URLs all belonging to the same underlying domain (figma.com)
        let mut evidences = Vec::new();
        for i in 1..=10 {
            let url = format!("https://figma.com/file/abc{}/design-project", i);
            evidences.push(ContextEvidence::observed_domain(item_id, "figma.com", Some(&url), None));
        }

        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        // 10 distinct URLs -> 10 normalized items
        assert_eq!(normalized_set.items.len(), 10);

        // Total effective weight for domain:figma.com MUST NOT exceed domain max_cap (0.35)
        let figma_total: f64 = normalized_set.by_correlation_group("domain:figma.com")
            .iter()
            .map(|e| e.effective_weight)
            .sum();

        assert!(
            figma_total <= 0.35001,
            "10 Figma URLs must saturate at domain cap (0.35), actual: {}",
            figma_total
        );

        // Verify later items have diminishing marginal gains and get capped
        let first_item = &normalized_set.items[0];
        let second_item = &normalized_set.items[1];
        let last_item = &normalized_set.items[9];

        assert!(first_item.effective_weight > second_item.effective_weight);
        assert!(last_item.is_capped || last_item.effective_weight < 0.01);
    }

    #[test]
    fn test_conflicting_evidence_detection() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_conflict";

        // User explicitly dismissed suggestions for "ws_rejected"
        let ev_dismiss = ContextEvidence::observed_user_dismissal(item_id, "ws_rejected", Some("Rejected WS"), None);

        // But semantic and history signals suggest "ws_rejected"
        let ev_hist = ContextEvidence::inferred_workspace_history(item_id, "ws_rejected", "Rejected WS", 0.90, None);
        let ev_sem = ContextEvidence::inferred_semantic_similarity(item_id, "other_item", 0.85, Some("Title"), Some("Rejected WS"), None);

        // And a non-conflicting signal for "ws_valid"
        let ev_valid = ContextEvidence::inferred_workspace_history(item_id, "ws_valid", "Valid WS", 0.80, None);

        let evidences = vec![ev_dismiss, ev_hist.clone(), ev_sem.clone(), ev_valid.clone()];
        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        // Conflicts must be detected
        assert!(normalized_set.has_conflicts());
        assert_eq!(normalized_set.conflicts.len(), 2);
        assert!(normalized_set.conflicts.iter().all(|c| c.conflict_type == "user_feedback_vs_inferred"));

        // The conflicting items must be marked is_conflicting = true
        let hist_item = normalized_set.items.iter().find(|i| i.id() == ev_hist.id).unwrap();
        let sem_item = normalized_set.items.iter().find(|i| i.id() == ev_sem.id).unwrap();
        let valid_item = normalized_set.items.iter().find(|i| i.id() == ev_valid.id).unwrap();

        assert!(hist_item.is_conflicting);
        assert!(sem_item.is_conflicting);
        assert!(!valid_item.is_conflicting);

        // Conflicting items do not contribute to effective weight
        assert_eq!(normalized_set.effective_weight_for_workspace("ws_rejected"), 0.0);
        assert!(normalized_set.effective_weight_for_workspace("ws_valid") > 0.0);
    }

    #[test]
    fn test_capped_evidence_contributions_multi_category() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_multi_cap";

        let mut evidences = Vec::new();
        // 5 apps (Chrome)
        for _ in 0..5 {
            evidences.push(ContextEvidence::observed_app(item_id, "Google Chrome", None));
        }
        // 5 domains (google.com)
        for i in 0..5 {
            let url = format!("https://google.com/search?q={}", i);
            evidences.push(ContextEvidence::observed_domain(item_id, "google.com", Some(&url), None));
        }
        // 5 temporal proximity connections
        for i in 0..5 {
            let other_id = format!("item_{}", i);
            evidences.push(ContextEvidence::inferred_temporal_proximity(item_id, &other_id, 10 + i * 5, 0.9, None));
        }

        let normalized_set = normalizer.normalize_slice(item_id, &evidences);

        let app_weight = normalized_set.effective_weight_by_category(EvidenceCategory::SourceApp);
        let domain_weight = normalized_set.effective_weight_by_category(EvidenceCategory::BrowserDomain);
        let temporal_weight = normalized_set.effective_weight_by_category(EvidenceCategory::TemporalProximity);

        // App cap = 0.30
        assert!(app_weight <= 0.30001, "App category weight must not exceed cap (0.30), actual: {}", app_weight);
        // Domain cap = 0.35
        assert!(domain_weight <= 0.35001, "Domain category weight must not exceed cap (0.35), actual: {}", domain_weight);
        // Temporal cap = 0.25
        assert!(temporal_weight <= 0.25001, "Temporal category weight must not exceed cap (0.25), actual: {}", temporal_weight);
    }

    #[test]
    fn test_deterministic_reproducibility() {
        let normalizer = EvidenceNormalizer::default();
        let item_id = "test_item_det";

        let ev1 = ContextEvidence::observed_app(item_id, "Slack", None);
        let ev2 = ContextEvidence::observed_domain(item_id, "slack.com", None, None);
        let ev3 = ContextEvidence::observed_session(item_id, "sess_abc", None);
        let ev4 = ContextEvidence::inferred_workspace_history(item_id, "ws_team", "Team", 0.85, None);

        let evidences = vec![ev1, ev2, ev3, ev4];

        let set1 = normalizer.normalize_slice(item_id, &evidences);
        let set2 = normalizer.normalize_slice(item_id, &evidences);

        assert_eq!(set1, set2, "Normalization must be 100% deterministic and reproducible");
    }
}
