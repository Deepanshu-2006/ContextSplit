use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub lifecycle_state: String,
    pub last_activity_at: Option<String>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub item_type: String, // "text", "image", "url", "file"
    pub content: String,
    pub title: Option<String>,
    pub metadata: String, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextSignal {
    pub id: String,
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    pub signal_type: String,
    pub session_id: Option<String>,
    pub payload: String, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub ended_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ItemRelationship {
    pub id: String,
    pub item_a_id: String,
    pub item_b_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub metadata: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemEmbedding {
    pub item_id: String,
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemEmbeddingMetadata {
    pub item_id: String,
    pub provider: String,
    pub model: String,
    pub created_at: String,
    pub dimensions: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceEmbedding {
    pub workspace_id: String,
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemSemanticStatus {
    pub item_id: String,
    pub status: String,
    pub retries: i32,
    pub model_version: Option<String>,
    pub updated_at: String,
}

/// A detected potential context grouping. Separate from Workspace.
/// A Workspace is user-confirmed; a ContextCandidate is a suggestion from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCandidate {
    pub id: String,
    /// "detected" | "suggested" | "accepted" | "dismissed" | "expired"
    pub status: String,
    /// 0.0 – 1.0
    pub confidence: f64,
    /// Human-readable inferred label (e.g. "KitKat project"), may be null
    pub label: Option<String>,
    /// JSON: detection_reason, item_count, signal_summary, etc.
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Join table: which Items belong to a ContextCandidate, with per-item relevance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextCandidateItem {
    pub candidate_id: String,
    pub item_id: String,
    /// How central this item is to the candidate (0.0 – 1.0)
    pub relevance_score: f64,
    pub added_at: String,
}

/// Full candidate + its member items, returned by the discovery commands.
#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateWithItems {
    pub candidate: ContextCandidate,
    pub items: Vec<CandidateMember>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateMember {
    pub membership: ContextCandidateItem,
    pub item: Item,
}

// ---------------------------------------------------------------------------
// Candidate Scoring Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrongestRelationship {
    pub item_a_id: String,
    pub item_b_id: String,
    pub relationship_type: String,
    pub strength: f64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributingItem {
    pub item_id: String,
    pub relevance_score: f64,
    pub title: Option<String>,
    pub item_type: String,
    pub primary_signal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirements {
    pub min_item_count: usize,
    pub min_avg_relationship_strength: f64,
    pub min_semantic_coherence: f64,
    pub min_independent_signals: usize,
    pub min_confidence: f64,
}

impl Default for EvidenceRequirements {
    fn default() -> Self {
        Self {
            min_item_count: 2,
            min_avg_relationship_strength: 0.50,
            min_semantic_coherence: 0.40,
            min_independent_signals: 2,
            min_confidence: 0.45,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScoringWeights {
    pub item_count_weight: f64,
    pub item_count_saturation: usize,
    pub relationship_strength_weight: f64,
    pub semantic_similarity_weight: f64,
    pub shared_session_weight: f64,
    pub temporal_proximity_weight: f64,
    pub common_domain_weight: f64,
    pub common_app_weight: f64,
    pub unassigned_items_weight: f64,
    pub user_history_weight: f64,
    pub workspace_suppression_threshold: f64,
    pub evidence_requirements: EvidenceRequirements,
}

impl Default for CandidateScoringWeights {
    fn default() -> Self {
        Self {
            item_count_weight: 0.15,
            item_count_saturation: 5,
            relationship_strength_weight: 0.35,
            semantic_similarity_weight: 0.15,
            shared_session_weight: 0.10,
            temporal_proximity_weight: 0.10,
            common_domain_weight: 0.05,
            common_app_weight: 0.05,
            unassigned_items_weight: 0.05,
            user_history_weight: 0.00,
            workspace_suppression_threshold: 0.60,
            evidence_requirements: EvidenceRequirements::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSuppressionMatch {
    pub workspace_id: String,
    pub workspace_name: String,
    pub match_score: f64,
    /// "strong_match" (suppressed) | "ambiguous_match" | "novel"
    pub match_type: String,
    pub is_suppressed: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScore {
    pub confidence: f64,
    pub confidence_percentage: String,
    pub evidence: Vec<String>,
    pub strongest_relationships: Vec<StrongestRelationship>,
    pub contributing_items: Vec<ContributingItem>,
    pub component_scores: std::collections::HashMap<String, f64>,
    pub workspace_match: Option<WorkspaceSuppressionMatch>,
    pub is_evidence_sufficient: bool,
    pub insufficient_evidence_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextMembership {
    pub id: String,
    pub item_id: String,
    pub workspace_id: String,
    /// "confirmed" | "suggested" | "dismissed"
    pub membership_type: String,
    pub confidence: f64,
    /// "user" | "context_engine" | "context_discovery"
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingWorkspaceEvaluation {
    pub workspace_id: String,
    pub workspace_name: String,
    pub score: f64,
    pub confidence: f64,
    pub confidence_percentage: String,
    pub confidence_band: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PotentialNewContextEvaluation {
    pub candidate_id: Option<String>,
    pub label: String,
    pub confidence: f64,
    pub confidence_percentage: String,
    pub confidence_band: String,
    pub item_count: usize,
    pub reasons: Vec<String>,
}

/// Canonical first-class context evaluation outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextInterpretationKind {
    /// Strong, reliable alignment to an existing workspace/context
    ExistingContext,
    /// Cohesive cluster representing an emerging context candidate
    NewContextCandidate,
    /// Competing affinity across multiple candidate contexts without a clear winner
    Ambiguous,
    /// Insufficient or weak contextual evidence; item remains unassigned in Inbox
    NoContext,
}

impl ContextInterpretationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextInterpretationKind::ExistingContext => "existing_context",
            ContextInterpretationKind::NewContextCandidate => "new_context_candidate",
            ContextInterpretationKind::Ambiguous => "ambiguous",
            ContextInterpretationKind::NoContext => "no_context",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ContextInterpretationKind::ExistingContext => "Existing Context",
            ContextInterpretationKind::NewContextCandidate => "New Context Candidate",
            ContextInterpretationKind::Ambiguous => "Ambiguous",
            ContextInterpretationKind::NoContext => "No Context",
        }
    }

    pub fn from_str_lenient(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "existing_context" | "existing_workspace" | "workspace" => ContextInterpretationKind::ExistingContext,
            "new_context_candidate" | "new_context" | "candidate" => ContextInterpretationKind::NewContextCandidate,
            "ambiguous" | "competing" | "uncertain" => ContextInterpretationKind::Ambiguous,
            _ => ContextInterpretationKind::NoContext,
        }
    }
}

/// Explicit condition explaining why an item has no confident context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoContextReason {
    /// Zero contextual evidence or capture metadata available
    ZeroEvidence,
    /// Overall contextual evidence is below minimum required threshold
    InsufficientEvidence,
    /// Only isolated weak peripheral signals (app, domain, content type) observed
    WeakEvidence,
    /// Strong contradictory evidence or prior user dismissal conflicts with candidate contexts
    ConflictingContexts,
    /// Item has no meaningful graph relationships with items in existing workspaces
    NoMeaningfulRelationships,
    /// Semantic similarity to workspace contents and profiles is too low
    LowSemanticRelevance,
    /// Evidence lacks independent signal categories; cannot reliably infer context without diversity
    InsufficientEvidenceDiversity,
}

impl NoContextReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            NoContextReason::ZeroEvidence => "zero_evidence",
            NoContextReason::InsufficientEvidence => "insufficient_evidence",
            NoContextReason::WeakEvidence => "weak_evidence",
            NoContextReason::ConflictingContexts => "conflicting_contexts",
            NoContextReason::NoMeaningfulRelationships => "no_meaningful_relationships",
            NoContextReason::LowSemanticRelevance => "low_semantic_relevance",
            NoContextReason::InsufficientEvidenceDiversity => "insufficient_evidence_diversity",
        }
    }

    pub fn display_title(&self) -> &'static str {
        match self {
            NoContextReason::ZeroEvidence => "Zero Contextual Evidence",
            NoContextReason::InsufficientEvidence => "Insufficient Evidence",
            NoContextReason::WeakEvidence => "Weak Peripheral Evidence",
            NoContextReason::ConflictingContexts => "Conflicting Contexts",
            NoContextReason::NoMeaningfulRelationships => "No Meaningful Relationships",
            NoContextReason::LowSemanticRelevance => "Low Semantic Relevance",
            NoContextReason::InsufficientEvidenceDiversity => "Insufficient Evidence Diversity",
        }
    }

    pub fn explanation(&self) -> &'static str {
        match self {
            NoContextReason::ZeroEvidence => "No contextual evidence or capture metadata is available for this item.",
            NoContextReason::InsufficientEvidence => "Available contextual evidence is below minimum required threshold to suggest a workspace.",
            NoContextReason::WeakEvidence => "Only isolated weak peripheral signals (source application or web domain) were observed without confirming corroboration.",
            NoContextReason::ConflictingContexts => "Strong contradictory evidence or previous user feedback conflicts with candidate contexts.",
            NoContextReason::NoMeaningfulRelationships => "Item has no meaningful contextual graph relationships with items in existing workspaces.",
            NoContextReason::LowSemanticRelevance => "Semantic similarity to workspace contents and profiles is too low for a reliable match.",
            NoContextReason::InsufficientEvidenceDiversity => "Evidence lacks independent signal categories; cannot reliably infer context without diversity.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedContextInterpretation {
    pub item_id: String,
    pub existing_workspaces: Vec<ExistingWorkspaceEvaluation>,
    pub potential_new_contexts: Vec<PotentialNewContextEvaluation>,
    /// "existing_context" (or legacy "existing_workspace") | "new_context_candidate" (or legacy "new_context") | "ambiguous" | "no_context" (or legacy "unassigned")
    pub primary_interpretation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpretation_kind: Option<ContextInterpretationKind>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_context_reason: Option<NoContextReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItemSummary {
    pub id: String,
    pub title: Option<String>,
    pub item_type: String,
    pub content_snippet: String,
    pub captured_at: String,
    pub session_id: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub membership_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSession {
    pub id: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub item_count: usize,
    pub items: Vec<TimelineItemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineUserAction {
    pub signal_id: String,
    pub action_type: String,
    pub item_id: Option<String>,
    pub item_title: Option<String>,
    pub timestamp: String,
    pub description: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: String,
    /// "item_captured" | "session_started" | "session_ended" | "user_action" | "relationship_formed"
    pub event_type: String,
    pub timestamp: String,
    pub title: String,
    pub description: String,
    pub item_id: Option<String>,
    pub session_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTimeline {
    pub workspace_id: String,
    pub workspace_name: String,
    pub created_at: String,
    pub total_items: usize,
    pub total_sessions: usize,
    pub first_activity_at: Option<String>,
    pub latest_activity_at: Option<String>,
    pub sessions: Vec<TimelineSession>,
    pub items: Vec<TimelineItemSummary>,
    pub major_actions: Vec<TimelineUserAction>,
    pub chronological_activity: Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextEvidenceItem {
    pub category: String,
    pub explanation: String,
    pub weight: f64,
    pub related_item_ids: Vec<String>,
    #[serde(default = "default_provenance_observed")]
    pub provenance: String,
    #[serde(default)]
    pub raw_strength: f64,
    #[serde(default = "default_true")]
    pub is_positive: bool,
}

fn default_provenance_observed() -> String {
    "observed".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContradictoryEvidenceItem {
    pub category: String,
    pub description: String,
    pub penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextExplanation {
    pub item_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub membership_type: String,
    pub confidence: f64,
    pub confidence_percentage: String,
    #[serde(default)]
    pub confidence_band: String,
    pub summary: String,
    pub explanations: Vec<String>,
    pub evidence: Vec<ContextEvidenceItem>,
    #[serde(default)]
    pub why_reasons: Vec<String>,
    #[serde(default)]
    pub uncertainty_reasons: Vec<String>,
    #[serde(default)]
    pub supporting_evidence: Vec<ContextEvidenceItem>,
    #[serde(default)]
    pub contradictory_evidence: Vec<ContradictoryEvidenceItem>,
    #[serde(default)]
    pub observed_evidence: Vec<ContextEvidenceItem>,
    #[serde(default)]
    pub inferred_evidence: Vec<ContextEvidenceItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambiguity_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUsageSummary {
    pub name: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainUsageSummary {
    pub domain: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextOverview {
    pub workspace_id: String,
    pub name: String,
    pub lifecycle_state: String,
    pub item_count: usize,
    pub recent_activity: Option<String>,
    pub session_count: usize,
    pub related_applications: Vec<AppUsageSummary>,
    pub important_domains: Vec<DomainUsageSummary>,
    pub recent_items: Vec<TimelineItemSummary>,
    pub timeline: ContextTimeline,
}

/// Day 12: Context-Aware Inbox State Machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxState {
    New,
    Processing,
    Understood,
    Suggested,
    NeedsReview,
    Organized,
    Dismissed,
}

impl InboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxState::New => "new",
            InboxState::Processing => "processing",
            InboxState::Understood => "understood",
            InboxState::Suggested => "suggested",
            InboxState::NeedsReview => "needs_review",
            InboxState::Organized => "organized",
            InboxState::Dismissed => "dismissed",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "new" => Ok(InboxState::New),
            "processing" => Ok(InboxState::Processing),
            "understood" => Ok(InboxState::Understood),
            "suggested" => Ok(InboxState::Suggested),
            "needs_review" => Ok(InboxState::NeedsReview),
            "organized" => Ok(InboxState::Organized),
            "dismissed" => Ok(InboxState::Dismissed),
            other => Err(format!("Unknown inbox state: '{}'", other)),
        }
    }

    /// Determines if transition from `self` to `target` is valid.
    pub fn can_transition_to(&self, target: InboxState) -> bool {
        match (self, target) {
            // Self-transitions are permitted (idempotent state updates)
            (a, b) if a == &b => true,

            // From New:
            // -> Processing (engine starts analysis)
            // -> Dismissed (user dismisses directly)
            // -> Organized (user immediately drags/moves to workspace manually)
            (InboxState::New, InboxState::Processing) => true,
            (InboxState::New, InboxState::Dismissed) => true,
            (InboxState::New, InboxState::Organized) => true,

            // From Processing:
            // -> Understood (analysis succeeds and extract signals/candidates)
            // -> NeedsReview (processing meets ambiguity or fallback)
            // -> Dismissed (user dismisses during processing)
            // -> Organized (user moves manually during processing)
            (InboxState::Processing, InboxState::Understood) => true,
            (InboxState::Processing, InboxState::NeedsReview) => true,
            (InboxState::Processing, InboxState::Dismissed) => true,
            (InboxState::Processing, InboxState::Organized) => true,

            // From Understood:
            // -> Suggested (confident workspace candidate identified)
            // -> NeedsReview (low confidence / multiple ambiguous contexts)
            // -> Organized (user assigns to workspace)
            // -> Dismissed (user dismisses)
            (InboxState::Understood, InboxState::Suggested) => true,
            (InboxState::Understood, InboxState::NeedsReview) => true,
            (InboxState::Understood, InboxState::Organized) => true,
            (InboxState::Understood, InboxState::Dismissed) => true,

            // From Suggested:
            // -> Organized (user accepts suggestion or assigns to workspace)
            // -> NeedsReview (suggestion demoted / conflicting signals arrive)
            // -> Dismissed (user dismisses / rejects suggestion)
            // -> Understood (suggestion invalidated / recalculated)
            (InboxState::Suggested, InboxState::Organized) => true,
            (InboxState::Suggested, InboxState::NeedsReview) => true,
            (InboxState::Suggested, InboxState::Dismissed) => true,
            (InboxState::Suggested, InboxState::Understood) => true,

            // From NeedsReview:
            // -> Suggested (subsequent evidence clarifies target workspace)
            // -> Organized (user manually resolves review and files item)
            // -> Dismissed (user rejects/dismisses item)
            // -> Processing (user triggers re-analysis)
            (InboxState::NeedsReview, InboxState::Suggested) => true,
            (InboxState::NeedsReview, InboxState::Organized) => true,
            (InboxState::NeedsReview, InboxState::Dismissed) => true,
            (InboxState::NeedsReview, InboxState::Processing) => true,

            // From Dismissed:
            // -> New (restore/reopen item)
            // -> NeedsReview (restore into review queue)
            // -> Organized (user files dismissed item)
            (InboxState::Dismissed, InboxState::New) => true,
            (InboxState::Dismissed, InboxState::NeedsReview) => true,
            (InboxState::Dismissed, InboxState::Organized) => true,

            // From Organized:
            // -> New (item moved back to Inbox)
            // -> Understood (item returned to Inbox while retaining context)
            (InboxState::Organized, InboxState::New) => true,
            (InboxState::Organized, InboxState::Understood) => true,

            // All other transitions are invalid
            _ => false,
        }
    }
}

impl std::fmt::Display for InboxState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Day 12: Tracks the lifecycle state of an item in the Inbox.
/// Day 12: Tracks the lifecycle state of an item in the Inbox.
/// Kept strictly separate from Item data and Workspace membership.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxItemState {
    pub item_id: String,
    pub state: InboxState,
    pub previous_state: Option<InboxState>,
    pub reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Explicit classification of contextual transitions between activity states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTransitionType {
    /// Clean transition between two distinct confident contexts (e.g. Workspace A -> Workspace B)
    ContextSwitch,
    /// Return to a previously active context after an interruption or detour (e.g. Workspace A -> Workspace B -> Workspace A)
    ContextReturn,
    /// Initial context emergence or startup of contextual activity (e.g. None -> Workspace A)
    ContextStart,
    /// Explicit context conclusion or session completion (e.g. Workspace A -> None)
    ContextEnd,
    /// Uncertain context shift, ambiguous split, or competing candidate possibilities
    AmbiguousTransition,
}

impl ContextTransitionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextTransitionType::ContextSwitch => "context_switch",
            ContextTransitionType::ContextReturn => "context_return",
            ContextTransitionType::ContextStart => "context_start",
            ContextTransitionType::ContextEnd => "context_end",
            ContextTransitionType::AmbiguousTransition => "ambiguous_transition",
        }
    }

    pub fn from_str_lenient(s: &str) -> Option<Self> {
        let clean = s.trim().to_lowercase().replace('-', "_").replace(' ', "_");
        match clean.as_str() {
            "context_switch" | "switch" => Some(ContextTransitionType::ContextSwitch),
            "context_return" | "return" => Some(ContextTransitionType::ContextReturn),
            "context_start" | "start" | "begin" => Some(ContextTransitionType::ContextStart),
            "context_end" | "end" | "conclude" => Some(ContextTransitionType::ContextEnd),
            "ambiguous_transition" | "ambiguous" | "uncertain" => Some(ContextTransitionType::AmbiguousTransition),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ContextTransitionType::ContextSwitch => "Context Switch",
            ContextTransitionType::ContextReturn => "Return to Context",
            ContextTransitionType::ContextStart => "Context Start",
            ContextTransitionType::ContextEnd => "Context End",
            ContextTransitionType::AmbiguousTransition => "Ambiguous Transition",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ContextTransitionType::ContextSwitch => "User transitioned work from one context to a different context",
            ContextTransitionType::ContextReturn => "User returned to a prior context after an interruption or detour",
            ContextTransitionType::ContextStart => "Initial activity detected for a context",
            ContextTransitionType::ContextEnd => "Activity concluded for a context",
            ContextTransitionType::AmbiguousTransition => "Activity transition occurred with ambiguous or competing context assignments",
        }
    }

    pub fn is_confident(&self) -> bool {
        !matches!(self, ContextTransitionType::AmbiguousTransition)
    }
}

impl std::fmt::Display for ContextTransitionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Domain model representing a detected contextual transition between states.
///
/// Invariant: A transition is purely an activity inference. It must NEVER modify
/// `Item` or `Workspace` membership.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextTransition {
    pub id: String,
    pub from_context_id: Option<String>,
    pub to_context_id: Option<String>,
    pub transition_type: ContextTransitionType,
    pub confidence: f64,
    pub detected_at: String,
    pub session_id: Option<String>,
    pub trigger_item_id: Option<String>,
    pub trigger_signal_id: Option<String>,
    pub metadata: String,
    pub created_at: String,
}

impl ContextTransition {
    pub fn new(
        from_context_id: Option<String>,
        to_context_id: Option<String>,
        transition_type: ContextTransitionType,
        confidence: f64,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from_context_id,
            to_context_id,
            transition_type,
            confidence: confidence.clamp(0.0, 1.0),
            detected_at: now.clone(),
            session_id: None,
            trigger_item_id: None,
            trigger_signal_id: None,
            metadata: "{}".to_string(),
            created_at: now,
        }
    }

    pub fn with_detected_at(mut self, timestamp: impl Into<String>) -> Self {
        self.detected_at = timestamp.into();
        self
    }

    pub fn with_session(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_trigger_item(mut self, item_id: Option<String>) -> Self {
        self.trigger_item_id = item_id;
        self
    }

    pub fn with_trigger_signal(mut self, signal_id: Option<String>) -> Self {
        self.trigger_signal_id = signal_id;
        self
    }

    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = metadata.into();
        self
    }
}







