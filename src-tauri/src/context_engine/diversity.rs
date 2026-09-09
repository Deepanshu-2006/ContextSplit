use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::context_engine::evidence::{ContextEvidence, ContextEvidenceSet, EvidenceCategory};
use crate::context_engine::scoring::EvidenceContribution;

/// Independent conceptual groups that aggregate correlated granular evidence categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndependentEvidenceGroup {
    /// Explicit, authoritative user assignment (highest reliability, ground truth)
    ExplicitMembership,
    /// Direct user interaction history and explicit feedback (moves, rejections)
    DirectUserAction,
    /// Temporal & cohesive activity sessions
    ActivitySession,
    /// Explicit graph relationships and temporal proximity
    StructuralGraph,
    /// Semantic vector embedding similarity (item-item and workspace centroid profile)
    SemanticContent,
    /// Operating system application and window context
    ApplicationEnvironment,
    /// Web domain and URL origin
    WebOrigin,
    /// Content classification, file metadata, and capture mechanics
    ContentClassification,
    /// Other unclassified signals
    Other,
}

impl IndependentEvidenceGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            IndependentEvidenceGroup::ExplicitMembership => "confirmed",
            IndependentEvidenceGroup::DirectUserAction => "history",
            IndependentEvidenceGroup::ActivitySession => "session",
            IndependentEvidenceGroup::StructuralGraph => "graph",
            IndependentEvidenceGroup::SemanticContent => "semantic",
            IndependentEvidenceGroup::ApplicationEnvironment => "app",
            IndependentEvidenceGroup::WebOrigin => "web",
            IndependentEvidenceGroup::ContentClassification => "content",
            IndependentEvidenceGroup::Other => "other",
        }
    }

    /// Whether this group represents an intrinsically strong / high-reliability signal.
    pub fn is_strong(&self) -> bool {
        match self {
            IndependentEvidenceGroup::ExplicitMembership
            | IndependentEvidenceGroup::DirectUserAction
            | IndependentEvidenceGroup::ActivitySession
            | IndependentEvidenceGroup::StructuralGraph
            | IndependentEvidenceGroup::SemanticContent => true,
            IndependentEvidenceGroup::ApplicationEnvironment
            | IndependentEvidenceGroup::WebOrigin
            | IndependentEvidenceGroup::ContentClassification
            | IndependentEvidenceGroup::Other => false,
        }
    }

    /// Reliability rating of this independent group [0.0, 1.0].
    pub fn baseline_reliability(&self) -> f32 {
        match self {
            IndependentEvidenceGroup::ExplicitMembership => 1.00,
            IndependentEvidenceGroup::DirectUserAction => 0.85,
            IndependentEvidenceGroup::ActivitySession => 0.85,
            IndependentEvidenceGroup::StructuralGraph => 0.75,
            IndependentEvidenceGroup::SemanticContent => 0.70,
            IndependentEvidenceGroup::WebOrigin => 0.55,
            IndependentEvidenceGroup::ApplicationEnvironment => 0.45,
            IndependentEvidenceGroup::ContentClassification => 0.35,
            IndependentEvidenceGroup::Other => 0.50,
        }
    }
}

/// Comprehensive analysis of evidence quantity, category diversity, and independent group distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceDiversityResult {
    /// Total count of positive supporting evidence items (evidence quantity)
    pub evidence_quantity: usize,
    /// Number of distinct granular evidence categories (e.g. browser_domain, browser_url, source_app)
    pub evidence_categories_count: usize,
    /// Number of distinct independent conceptual evidence groups (e.g. semantic, session, web, graph)
    pub independent_groups_count: usize,
    /// List of distinct independent evidence group names present
    pub independent_groups: Vec<String>,
    /// Normalized diversity score in [0.0, 1.0] reflecting the breadth and strength of distinct groups
    pub diversity_score: f32,
    /// Multiplier applied to confidence calculation to prevent correlated evidence inflation [0.20, 1.0]
    pub diversity_multiplier: f32,
    /// Whether evidence relies solely on a single independent group (requires conservative confidence)
    pub is_single_group: bool,
    /// Human-readable explanation of the diversity state
    pub explanation: String,
}

/// Dedicated calculator for evaluating evidence diversity and independence.
#[derive(Debug, Clone, Default)]
pub struct EvidenceDiversityCalculator;

impl EvidenceDiversityCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Maps a granular category string (or EvidenceCategory string representation) to an IndependentEvidenceGroup.
    pub fn map_category_to_group(category: &str) -> IndependentEvidenceGroup {
        match category {
            "confirmed_membership" => IndependentEvidenceGroup::ExplicitMembership,
            "workspace_history" | "user_feedback" => IndependentEvidenceGroup::DirectUserAction,
            "same_session" => IndependentEvidenceGroup::ActivitySession,
            "related_item" | "related_items" | "temporal_proximity" => IndependentEvidenceGroup::StructuralGraph,
            "semantic_similarity" | "semantic_similarity_items" | "semantic_profile" => {
                IndependentEvidenceGroup::SemanticContent
            }
            "source_app" | "source_window" => IndependentEvidenceGroup::ApplicationEnvironment,
            "browser_domain" | "browser_url" => IndependentEvidenceGroup::WebOrigin,
            "content_type" | "file_metadata" | "capture_method" => {
                IndependentEvidenceGroup::ContentClassification
            }
            _ => IndependentEvidenceGroup::Other,
        }
    }

    /// Evaluates diversity metrics from a list of EvidenceContribution items.
    pub fn calculate_from_contributions(
        contributions: &[EvidenceContribution],
    ) -> EvidenceDiversityResult {
        let pos_contribs: Vec<&EvidenceContribution> = contributions
            .iter()
            .filter(|c| c.is_positive && c.weighted_score > 0.0)
            .collect();

        Self::compute_diversity(
            pos_contribs.len(),
            pos_contribs.iter().map(|c| c.category.as_str()),
            pos_contribs.iter().map(|c| (c.category.as_str(), c.weighted_score)),
        )
    }

    /// Evaluates diversity metrics directly from a ContextEvidenceSet.
    pub fn calculate_from_evidence_set(
        evidence_set: &ContextEvidenceSet,
    ) -> EvidenceDiversityResult {
        let pos_items: Vec<&ContextEvidence> = evidence_set
            .items
            .iter()
            .filter(|e| e.strength > 0.0 && e.category != EvidenceCategory::UserFeedback)
            .collect();

        Self::compute_diversity(
            pos_items.len(),
            pos_items.iter().map(|e| e.category.as_str()),
            pos_items.iter().map(|e| (e.category.as_str(), e.strength as f32)),
        )
    }

    fn compute_diversity<'a>(
        evidence_quantity: usize,
        categories: impl Iterator<Item = &'a str>,
        category_weights: impl Iterator<Item = (&'a str, f32)>,
    ) -> EvidenceDiversityResult {
        if evidence_quantity == 0 {
            return EvidenceDiversityResult {
                evidence_quantity: 0,
                evidence_categories_count: 0,
                independent_groups_count: 0,
                independent_groups: Vec::new(),
                diversity_score: 0.0,
                diversity_multiplier: 0.0,
                is_single_group: false,
                explanation: "No evidence available".to_string(),
            };
        }

        let mut unique_categories: HashSet<String> = HashSet::new();
        for cat in categories {
            unique_categories.insert(cat.to_string());
        }
        let evidence_categories_count = unique_categories.len();

        let mut group_weights: HashMap<IndependentEvidenceGroup, f32> = HashMap::new();
        for (cat, weight) in category_weights {
            let group = Self::map_category_to_group(cat);
            *group_weights.entry(group).or_insert(0.0) += weight;
        }

        let independent_groups_count = group_weights.len();
        let mut group_names: Vec<String> = group_weights
            .keys()
            .map(|g| g.as_str().to_string())
            .collect();
        group_names.sort();

        let has_confirmed = group_weights.contains_key(&IndependentEvidenceGroup::ExplicitMembership);
        let strong_groups_count = group_weights.keys().filter(|g| g.is_strong()).count();

        // Calculate Diversity Multiplier and Diversity Score
        let (diversity_multiplier, diversity_score, is_single_group, explanation) = if has_confirmed {
            (
                1.0,
                1.0,
                independent_groups_count <= 1,
                "Explicit authoritative assignment".to_string(),
            )
        } else {
            match independent_groups_count {
                0 => (0.0, 0.0, false, "No positive evidence groups".to_string()),
                1 => {
                    let group = group_weights.keys().next().unwrap();
                    let (mult, score, label) = match group {
                        IndependentEvidenceGroup::SemanticContent => (
                            0.45,
                            0.35,
                            "Single semantic similarity group; lacks corroborating independent context (sessions, graph, history)",
                        ),
                        IndependentEvidenceGroup::DirectUserAction => (
                            0.45,
                            0.40,
                            "Single workspace history group; requires corroborating session or semantic context",
                        ),
                        IndependentEvidenceGroup::ActivitySession => (
                            0.45,
                            0.40,
                            "Single activity session group; requires corroborating semantic or history context",
                        ),
                        IndependentEvidenceGroup::StructuralGraph => (
                            0.45,
                            0.35,
                            "Single relationship graph group; requires corroborating session or semantic context",
                        ),
                        IndependentEvidenceGroup::WebOrigin => (
                            0.35,
                            0.20,
                            "Isolated web domain/URL evidence; peripheral signal without semantic or session backing",
                        ),
                        IndependentEvidenceGroup::ApplicationEnvironment => (
                            0.35,
                            0.20,
                            "Isolated application evidence; peripheral signal without semantic or session backing",
                        ),
                        IndependentEvidenceGroup::ContentClassification => (
                            0.30,
                            0.15,
                            "Isolated content type/file metadata evidence; minor signal",
                        ),
                        _ => (0.35, 0.20, "Single isolated evidence group"),
                    };

                    (mult, score, true, label.to_string())
                }
                2 => {
                    let (mult, score, label) = if strong_groups_count == 2 {
                        (0.85, 0.80, "Supported by 2 strong independent evidence groups")
                    } else if strong_groups_count == 1 {
                        (0.80, 0.70, "Supported by 1 strong and 1 peripheral independent evidence group")
                    } else {
                        (0.50, 0.40, "Supported only by 2 peripheral weak evidence groups (app/web/content)")
                    };

                    (mult, score, false, label.to_string())
                }
                3 => (
                    1.00,
                    0.92,
                    false,
                    "High diversity: supported by 3 independent evidence groups".to_string(),
                ),
                _ => (
                    1.00,
                    1.00,
                    false,
                    format!("Very high diversity: supported by {} independent evidence groups", independent_groups_count),
                ),
            }
        };

        EvidenceDiversityResult {
            evidence_quantity,
            evidence_categories_count,
            independent_groups_count,
            independent_groups: group_names,
            diversity_score,
            diversity_multiplier,
            is_single_group,
            explanation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_evidence_same_domain_does_not_inflate_diversity() {
        // 10 URLs from the exact same domain
        let mut contributions = Vec::new();
        for i in 1..=10 {
            contributions.push(EvidenceContribution {
                category: "browser_domain".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.05,
                description: format!("Same github.com domain item {}", i),
                is_positive: true,
            });
        }

        let result = EvidenceDiversityCalculator::calculate_from_contributions(&contributions);

        assert_eq!(result.evidence_quantity, 10, "Quantity should be 10");
        assert_eq!(result.evidence_categories_count, 1, "Only 1 granular category (browser_domain)");
        assert_eq!(result.independent_groups_count, 1, "Only 1 independent group (WebOrigin)");
        assert_eq!(result.independent_groups, vec!["web"]);
        assert!(result.is_single_group);
        assert!(
            result.diversity_score <= 0.30,
            "Diversity score must remain low for single group repeated evidence (actual: {})",
            result.diversity_score
        );
        assert!(
            result.diversity_multiplier <= 0.35,
            "Diversity multiplier must be damped (actual: {})",
            result.diversity_multiplier
        );
    }

    #[test]
    fn test_correlated_evidence_grouped_correctly() {
        // Correlated evidence: browser domain + browser url + source app
        let contributions = vec![
            EvidenceContribution {
                category: "browser_domain".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.10,
                description: "Same github.com domain".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "browser_url".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.05,
                description: "Matching URL pattern".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "source_app".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.05,
                description: "Same Google Chrome source app".to_string(),
                is_positive: true,
            },
        ];

        let result = EvidenceDiversityCalculator::calculate_from_contributions(&contributions);

        assert_eq!(result.evidence_quantity, 3);
        assert_eq!(result.evidence_categories_count, 3);
        // browser_domain + browser_url belong to "web", source_app belongs to "app" -> 2 independent groups
        assert_eq!(result.independent_groups_count, 2);
        assert_eq!(result.independent_groups, vec!["app", "web"]);
        assert!(!result.is_single_group);
        // Both are peripheral weak groups -> multiplier capped around 0.50
        assert_eq!(result.diversity_multiplier, 0.50);
        assert_eq!(result.diversity_score, 0.40);
    }

    #[test]
    fn test_independent_evidence_multi_group_yields_high_diversity() {
        // 4 strong independent groups: session + semantic + graph + history
        let contributions = vec![
            EvidenceContribution {
                category: "same_session".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.30,
                description: "Same active work session".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_similarity_items".to_string(),
                raw_strength: 0.92,
                weighted_score: 0.40,
                description: "Semantic similarity 0.92".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "related_items".to_string(),
                raw_strength: 0.85,
                weighted_score: 0.40,
                description: "2 related items in workspace".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "workspace_history".to_string(),
                raw_strength: 1.0,
                weighted_score: 0.40,
                description: "User previously organized similar items".to_string(),
                is_positive: true,
            },
        ];

        let result = EvidenceDiversityCalculator::calculate_from_contributions(&contributions);

        assert_eq!(result.evidence_quantity, 4);
        assert_eq!(result.evidence_categories_count, 4);
        assert_eq!(result.independent_groups_count, 4);
        assert_eq!(
            result.independent_groups,
            vec!["graph", "history", "semantic", "session"]
        );
        assert!(!result.is_single_group);
        assert_eq!(result.diversity_score, 1.00);
        assert_eq!(result.diversity_multiplier, 1.00);
    }

    #[test]
    fn test_single_strong_semantic_group_diversity_is_bounded() {
        let contributions = vec![
            EvidenceContribution {
                category: "semantic_similarity_items".to_string(),
                raw_strength: 0.98,
                weighted_score: 0.40,
                description: "High semantic similarity".to_string(),
                is_positive: true,
            },
            EvidenceContribution {
                category: "semantic_profile".to_string(),
                raw_strength: 0.95,
                weighted_score: 0.45,
                description: "Matches workspace profile".to_string(),
                is_positive: true,
            },
        ];

        let result = EvidenceDiversityCalculator::calculate_from_contributions(&contributions);

        assert_eq!(result.evidence_quantity, 2);
        assert_eq!(result.evidence_categories_count, 2);
        // Both semantic categories belong to "semantic" group -> 1 independent group
        assert_eq!(result.independent_groups_count, 1);
        assert_eq!(result.independent_groups, vec!["semantic"]);
        assert!(result.is_single_group);
        assert_eq!(result.diversity_multiplier, 0.45);
        assert_eq!(result.diversity_score, 0.35);
    }
}
