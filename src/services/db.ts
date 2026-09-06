import { invoke } from "@tauri-apps/api/core";

export interface Workspace {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  lifecycle_state?: "active" | "dormant" | "archived";
  last_activity_at?: string | null;
  archived_at?: string | null;
}

export interface Item {
  id: string;
  workspace_id: string | null;
  session_id: string | null;
  item_type: string;
  content: string;
  title: string | null;
  metadata: string;
  created_at: string;
  updated_at: string;
}

export type InboxState =
  | "new"
  | "processing"
  | "understood"
  | "suggested"
  | "needs_review"
  | "organized"
  | "dismissed";

export interface InboxItemState {
  item_id: string;
  state: InboxState;
  previous_state?: InboxState | null;
  reason?: string | null;
  created_at: string;
  updated_at: string;
}

export type AttentionLevel = "none" | "low" | "medium" | "high";

export interface AttentionFactors {
  is_organized: boolean;
  is_dismissed: boolean;
  contextual_confidence: number;
  is_ambiguous: boolean;
  ambiguity_margin?: number | null;
  has_new_candidate: boolean;
  candidate_id?: string | null;
  candidate_confidence?: number | null;
  has_suggested_membership: boolean;
  suggested_workspace_id?: string | null;
  suggested_workspace_name?: string | null;
  dismissal_count: number;
  similar_dismissals_count: number;
  item_age_hours: number;
  historical_rejection_penalty: number;
}

export interface ItemAttention {
  item_id: string;
  attention_required: boolean;
  attention_level: AttentionLevel;
  reason: string;
  score: number;
  factors: AttentionFactors;
}

export interface AttentionConfig {
  high_attention_threshold?: number;
  medium_attention_threshold?: number;
  low_attention_threshold?: number;
  ambiguity_threshold?: number;
  candidate_bonus?: number;
  suggested_membership_bonus?: number;
  ambiguity_bonus?: number;
  dismissal_penalty_per_count?: number;
  similar_dismissal_penalty?: number;
  fresh_item_window_hours?: number;
  stale_item_threshold_days?: number;
}

export interface ContextSignal {
  id: string;
  workspace_id: string | null;
  item_id: string | null;
  signal_type: string;
  session_id: string | null;
  payload: string;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceScore {
  workspace_id: string;
  total_score: number;
  confidence: number;
  confidence_band: string;
  reasons: string[];
}

export interface ItemSemanticStatus {
  item_id: string;
  status: "pending" | "processing" | "ready" | "failed";
  retries: number;
  model_version?: string;
  updated_at: string;
}

export interface ItemEmbeddingMetadata {
  item_id: string;
  provider: string;
  model: string;
  created_at: string;
  dimensions: number;
}

export interface PrivacySettings {
  enable_active_app: boolean;
  enable_window_title: boolean;
  enable_browser_domain: boolean;
  enable_session_tracking: boolean;
  enable_semantic_processing: boolean;
  semantic_provider: string;
  enable_semantic_suggestions: boolean;
  enable_ai_reasoning: boolean;
  capture_paused?: boolean;
}

export interface ItemRelationship {
  id: string;
  item_a_id: string;
  item_b_id: string;
  relationship_type: string;
  strength: number;
  created_at: string;
  metadata: string | null;
}

// --- Context Discovery ---

export type CandidateStatus = "detected" | "suggested" | "accepted" | "dismissed" | "expired";

export interface ContextCandidate {
  id: string;
  /** "detected" | "suggested" | "accepted" | "dismissed" | "expired" */
  status: CandidateStatus;
  /** 0.0 – 1.0 */
  confidence: number;
  /** Human-readable inferred label, e.g. "KitKat project" */
  label: string | null;
  /** JSON string with detection metadata */
  metadata: string;
  created_at: string;
  updated_at: string;
}

export interface ContextCandidateItem {
  candidate_id: string;
  item_id: string;
  /** How central this item is to the candidate (0.0 – 1.0) */
  relevance_score: number;
  added_at: string;
}

export interface CandidateMember {
  membership: ContextCandidateItem;
  item: Item;
}

export interface CandidateWithItems {
  candidate: ContextCandidate;
  items: CandidateMember[];
}

export interface StrongestRelationship {
  item_a_id: string;
  item_b_id: string;
  relationship_type: string;
  strength: number;
  description?: string | null;
}

export interface ContributingItem {
  item_id: string;
  relevance_score: number;
  title: string | null;
  item_type: string;
  primary_signal: string | null;
}

export interface EvidenceRequirements {
  min_item_count: number;
  min_avg_relationship_strength: number;
  min_semantic_coherence: number;
  min_independent_signals: number;
  min_confidence: number;
}

export interface CandidateScoringWeights {
  item_count_weight: number;
  item_count_saturation: number;
  relationship_strength_weight: number;
  semantic_similarity_weight: number;
  shared_session_weight: number;
  temporal_proximity_weight: number;
  common_domain_weight: number;
  common_app_weight: number;
  unassigned_items_weight: number;
  user_history_weight: number;
  workspace_suppression_threshold: number;
  evidence_requirements?: EvidenceRequirements;
}

export interface WorkspaceSuppressionMatch {
  workspace_id: string;
  workspace_name: string;
  match_score: number;
  match_type: "strong_match" | "ambiguous_match" | "novel";
  is_suppressed: boolean;
  reasons: string[];
}

export interface CandidateScore {
  confidence: number;
  confidence_percentage: string;
  evidence: string[];
  strongest_relationships: StrongestRelationship[];
  contributing_items: ContributingItem[];
  component_scores: Record<string, number>;
  workspace_match?: WorkspaceSuppressionMatch | null;
  is_evidence_sufficient: boolean;
  insufficient_evidence_reasons: string[];
}

// --- Context Membership ---

export type MembershipType = "confirmed" | "suggested" | "dismissed";
export type MembershipSource = "user" | "context_engine" | "context_discovery";

export interface ContextMembership {
  id: string;
  item_id: string;
  workspace_id: string;
  membership_type: MembershipType;
  confidence: number;
  source: MembershipSource;
  created_at: string;
  updated_at: string;
}

export interface ExistingWorkspaceEvaluation {
  workspace_id: string;
  workspace_name: string;
  score: number;
  confidence: number;
  confidence_percentage: string;
  confidence_band: string;
  reasons: string[];
}

export interface PotentialNewContextEvaluation {
  candidate_id?: string | null;
  label: string;
  confidence: number;
  confidence_percentage: string;
  confidence_band: string;
  item_count: number;
  reasons: string[];
}

export interface UnifiedContextInterpretation {
  item_id: string;
  existing_workspaces: ExistingWorkspaceEvaluation[];
  potential_new_contexts: PotentialNewContextEvaluation[];
  primary_interpretation: "existing_workspace" | "new_context" | "unassigned" | "ambiguous";
  summary: string;
}

export interface TimelineItemSummary {
  id: string;
  title?: string | null;
  item_type: string;
  content_snippet: string;
  captured_at: string;
  session_id?: string | null;
  app_name?: string | null;
  domain?: string | null;
  membership_type: string;
}

export interface TimelineSession {
  id: string;
  started_at: string;
  last_activity_at: string;
  ended_at?: string | null;
  status: string;
  item_count: number;
  items: TimelineItemSummary[];
}

export interface TimelineUserAction {
  signal_id: string;
  action_type: string;
  item_id?: string | null;
  item_title?: string | null;
  timestamp: string;
  description: string;
  metadata: Record<string, unknown>;
}

export interface TimelineEvent {
  id: string;
  event_type: string;
  timestamp: string;
  title: string;
  description: string;
  item_id?: string | null;
  session_id?: string | null;
  metadata: Record<string, unknown>;
}

export interface ContextTimeline {
  workspace_id: string;
  workspace_name: string;
  created_at: string;
  total_items: number;
  total_sessions: number;
  first_activity_at?: string | null;
  latest_activity_at?: string | null;
  sessions: TimelineSession[];
  items: TimelineItemSummary[];
  major_actions: TimelineUserAction[];
  chronological_activity: TimelineEvent[];
}

export interface AppUsageSummary {
  name: string;
  item_count: number;
}

export interface DomainUsageSummary {
  domain: string;
  item_count: number;
}

export interface ContextOverview {
  workspace_id: string;
  name: string;
  lifecycle_state: "active" | "dormant" | "archived";
  item_count: number;
  recent_activity?: string | null;
  session_count: number;
  related_applications: AppUsageSummary[];
  important_domains: DomainUsageSummary[];
  recent_items: TimelineItemSummary[];
  timeline: ContextTimeline;
}

export interface ContextEvidenceItem {
  category: string;
  explanation: string;
  weight: number;
  related_item_ids: string[];
}

export interface ContextExplanation {
  item_id: string;
  workspace_id: string;
  workspace_name: string;
  membership_type: string;
  confidence: number;
  confidence_percentage: string;
  summary: string;
  explanations: string[];
  evidence: ContextEvidenceItem[];
}

export interface TemporalDecayConfig {
  strategy: string;
  half_life_days: number;
  min_weight_floor: number;
  grace_period_hours: number;
}

// --- AI Context Reasoning Models ---

export interface TargetItemEvidence {
  id: string;
  item_type: string;
  title?: string | null;
  content_snippet: string;
  app_name?: string | null;
  domain?: string | null;
  session_id?: string | null;
  captured_at: string;
}

export interface CandidateContextEvidence {
  context_id: string;
  name: string;
  context_type: "workspace" | "candidate";
  lifecycle_state: "active" | "dormant" | "archived";
  deterministic_score: number;
  deterministic_confidence: number;
  confidence_band: string;
  evidence_factors: string[];
  item_count: number;
  temporal_recency_score: number;
}

export interface RelatedItemEvidence {
  item_id: string;
  title?: string | null;
  item_type: string;
  snippet: string;
  workspace_id?: string | null;
  workspace_name?: string | null;
  relevance_score: number;
}

export interface ItemRelationshipEvidence {
  other_item_id: string;
  other_item_title?: string | null;
  relationship_type: string;
  strength: number;
  target_context_id?: string | null;
  explanation: string;
}

export interface SemanticSimilaritySummary {
  other_item_id: string;
  other_item_title?: string | null;
  workspace_name?: string | null;
  similarity_score: number;
  similarity_band: string;
  summary: string;
}

export interface SessionEvidence {
  session_id: string;
  item_count: number;
  is_target_session: boolean;
  associated_workspaces: string[];
}

export interface WorkspaceProfileSummary {
  workspace_id: string;
  workspace_name: string;
  has_profile: boolean;
  alignment_score?: number | null;
  alignment_band?: string | null;
  theme_summary: string;
}

export interface CuratedContextSignal {
  signal_type: string;
  summary: string;
}

export interface RawCapturePayload {
  content: string;
  data_type?: string | null;
  title?: string | null;
  source_method?: "clipboard" | "drag_drop" | "manual" | "file_watch" | "hotkey" | "api" | string;
  platform?: "macos" | "windows" | "linux" | "generic";
  app_name?: string | null;
  window_title?: string | null;
  process_info?: string | null;
  file_paths?: string[];
  mime_type?: string | null;
  original_url?: string | null;
  size_bytes?: number | null;
  workspace_id?: string | null;
  raw_metadata?: Record<string, unknown> | null;
}

export interface CaptureResult {
  item: Item;
  signals_emitted: number;
  session_id: string | null;
  semantic_status: string;
  is_duplicate: boolean;
}

export interface TemporalEvidence {
  age_days: number;
  recency_decay_weight: number;
  is_recent: boolean;
}

export interface EvidenceMetadata {
  generated_at: string;
  engine_version: string;
  total_db_items_excluded: number;
  privacy_boundary: string;
}

export interface DeterministicEvidenceAssessment {
  confidence_band: "high" | "moderate" | "uncertain" | "weak" | "none" | string;
  is_weak_evidence: boolean;
  is_ambiguous: boolean;
  competing_contexts?: string[];
  assessment_summary: string;
}

export interface EvidencePackage {
  target_item: TargetItemEvidence;
  candidate_contexts: CandidateContextEvidence[];
  relevant_related_items: RelatedItemEvidence[];
  relationships: ItemRelationshipEvidence[];
  semantic_similarity_summaries: SemanticSimilaritySummary[];
  relevant_sessions: SessionEvidence[];
  session_context?: SessionEvidence | null;
  workspace_semantic_profiles: WorkspaceProfileSummary[];
  contextual_signals: CuratedContextSignal[];
  temporal_context: TemporalEvidence;
  deterministic_assessment: DeterministicEvidenceAssessment;
  system_metadata: EvidenceMetadata;
}

export interface AlternativeContextReasoning {
  context_id: string;
  context_name: string;
  confidence: number;
  rationale: string;
}

export interface ReasoningProviderInfo {
  provider_id: string;
  provider_type: string;
  model_version: string;
  timestamp: string;
}

export type ContextReasoningDecision =
  | "existing_context"
  | "new_context"
  | "uncertain"
  | "no_context";

export interface ContextReasoningResult {
  decision: ContextReasoningDecision;
  suggested_context_reference?: string | null;
  suggested_name?: string | null;
  confidence: number;
  explanation: string;
  evidence_references: string[];
  uncertainty_reason?: string | null;
  provider_info?: ReasoningProviderInfo | null;
  alternative_interpretations?: AlternativeContextReasoning[];
}

/** All structured reasons the AI layer can fail. Maps 1-to-1 with Rust AIFailureReason. */
export type AIFailureReason =
  | 'disabled'
  | 'provider_unavailable'
  | 'timeout'
  | 'provider_error'
  | 'malformed_response'
  | 'evidence_unavailable'
  | 'internal_error'
  | { gated: string };

/**
 * Top-level response from the `analyze_item_context_ai` command.
 * Always succeeds — ai_result is null when AI was unavailable.
 */
export interface AIContextReasoningResponse {
  /** The AI reasoning result. null when AI was unavailable. */
  ai_result: ContextReasoningResult | null;
  /** True when AI reasoning could not be completed for any reason. */
  ai_unavailable: boolean;
  /** Human-readable reason suitable for UI display. */
  unavailability_reason: string | null;
  /** Structured failure category. */
  failure_reason: AIFailureReason | null;
}

export const db = {
  async createWorkspace(name: string): Promise<Workspace> {
    return invoke("create_workspace", { name });
  },

  async getWorkspace(id: string): Promise<Workspace> {
    return invoke("get_workspace", { id });
  },

  async listWorkspaces(): Promise<Workspace[]> {
    return invoke("list_workspaces");
  },

  async updateWorkspace(id: string, name: string): Promise<Workspace> {
    return invoke("update_workspace", { id, name });
  },

  async deleteWorkspace(id: string): Promise<void> {
    return invoke("delete_workspace", { id });
  },

  // Item operations
  async createItem(workspaceId: string | null, itemType: string, content: string, title: string | null = null, metadata: string = "{}"): Promise<Item> {
    return invoke("create_item", { workspaceId, itemType, content, title, metadata });
  },

  async getItem(id: string): Promise<Item> {
    return invoke("get_item", { id });
  },

  async listItems(workspaceId: string | null): Promise<Item[]> {
    return invoke("list_items", { workspaceId });
  },

  async updateItem(id: string, content?: string, title?: string, metadata?: string): Promise<Item> {
    return invoke("update_item", { id, content, title, metadata });
  },

  async moveItem(id: string, workspaceId: string | null): Promise<Item> {
    return invoke("move_item", { id, workspaceId });
  },

  async deleteItem(id: string): Promise<void> {
    return invoke("delete_item", { id });
  },

  async emptyInbox(): Promise<number> {
    return invoke("empty_inbox");
  },

  // Context Signal operations
  async createContextSignal(
    workspaceId: string | null,
    itemId: string | null,
    signalType: string,
    sessionId: string | null,
    payload: string
  ): Promise<ContextSignal> {
    return invoke("create_context_signal", { workspaceId, itemId, signalType, sessionId, payload });
  },

  async getContextSignal(id: string): Promise<ContextSignal> {
    return invoke("get_context_signal", { id });
  },

  async listContextSignals(workspaceId: string | null = null, itemId: string | null = null): Promise<ContextSignal[]> {
    return invoke("list_context_signals", { workspaceId, itemId });
  },

  // Context Engine
  async scoreItem(itemId: string): Promise<WorkspaceScore[]> {
    return await invoke("score_item", { itemId });
  },

  async evaluateItemContext(itemId: string): Promise<UnifiedContextInterpretation> {
    return await invoke("evaluate_item_context", { itemId });
  },

  async getStrongestRelationships(itemId: string, limit: number = 5): Promise<ItemRelationship[]> {
    return await invoke("get_strongest_relationships", { itemId, limit });
  },

  async getItemSemanticStatus(id: string): Promise<ItemSemanticStatus | null> {
    return await invoke("get_item_semantic_status", { id });
  },

  async getItemEmbeddingMetadata(id: string): Promise<ItemEmbeddingMetadata | null> {
    return await invoke("get_item_embedding_metadata", { id });
  },

  // Settings
  getSettings: () => invoke<PrivacySettings>('get_settings'),
  updateSettings: (settings: PrivacySettings) => invoke<void>('update_settings', { settings }),
  
  deleteSemanticData: () => invoke<void>('delete_semantic_data'),
  rebuildSemanticData: () => invoke<void>('rebuild_semantic_data'),

  // --- Context Discovery ---
  /** Trigger the discovery algorithm. Returns newly created candidates only. */
  runContextDiscovery: () => invoke<ContextCandidate[]>('run_context_discovery'),

  /** List all candidates, optionally filtered by status. */
  listContextCandidates: (statusFilter?: CandidateStatus) =>
    invoke<ContextCandidate[]>('list_context_candidates', { statusFilter: statusFilter ?? null }),

  /** Get a single candidate by ID. */
  getContextCandidate: (id: string) =>
    invoke<ContextCandidate>('get_context_candidate', { id }),

  /** Get a candidate together with its member items. */
  getCandidateItems: (candidateId: string) =>
    invoke<CandidateWithItems>('get_candidate_items', { candidateId }),

  /** Remove an individual item from a candidate group. */
  removeCandidateItem: (candidateId: string, itemId: string) =>
    invoke<void>('remove_candidate_item', { candidateId, itemId }),

  /** Advance the candidate status (e.g. detected → suggested → accepted). */
  updateCandidateStatus: (id: string, status: CandidateStatus) =>
    invoke<ContextCandidate>('update_candidate_status', { id, status }),

  /** Update/edit the candidate label (name). */
  updateCandidateLabel: (id: string, label: string) =>
    invoke<ContextCandidate>('update_candidate_label', { id, label }),

  /** Atomically create a workspace from a candidate and assign included items. */
  createWorkspaceFromCandidate: (candidateId: string, customName?: string) =>
    invoke<Workspace>('create_workspace_from_candidate', { candidateId, customName: customName ?? null }),

  /** Deterministically generate a candidate name from a group of item IDs. */
  generateCandidateName: (itemIds: string[]) =>
    invoke<string>('generate_candidate_name', { itemIds }),

  /** Expire stale detected/suggested candidates older than maxAgeSecs (default 72h). */
  expireStateCandidates: (maxAgeSecs?: number) =>
    invoke<number>('expire_stale_candidates', { maxAgeSecs: maxAgeSecs ?? null }),

  /** Score an existing context candidate by its candidate ID. */
  scoreContextCandidate: (candidateId: string) =>
    invoke<CandidateScore>('score_context_candidate', { candidateId }),

  /** Score an arbitrary group of item IDs on the fly. */
  scoreCandidateGroup: (itemIds: string[]) =>
    invoke<CandidateScore>('score_candidate_group', { itemIds }),

  // --- Context Membership ---
  createContextMembership: (
    itemId: string,
    workspaceId: string,
    membershipType: MembershipType,
    confidence: number = 1.0,
    source: MembershipSource = "user"
  ) =>
    invoke<ContextMembership>("create_context_membership", {
      itemId,
      workspaceId,
      membershipType,
      confidence,
      source,
    }),

  getContextMembership: (id: string) =>
    invoke<ContextMembership>("get_context_membership", { id }),

  getItemWorkspaceMembership: (itemId: string, workspaceId: string) =>
    invoke<ContextMembership | null>("get_item_workspace_context_membership", {
      itemId,
      workspaceId,
    }),

  listItemMemberships: (itemId: string) =>
    invoke<ContextMembership[]>("list_item_context_memberships", { itemId }),

  listWorkspaceMemberships: (
    workspaceId: string,
    membershipType?: MembershipType
  ) =>
    invoke<ContextMembership[]>("list_workspace_context_memberships", {
      workspaceId,
      membershipType: membershipType ?? null,
    }),

  confirmContextMembership: (itemId: string, workspaceId: string) =>
    invoke<ContextMembership>("confirm_context_membership", {
      itemId,
      workspaceId,
    }),

  dismissContextMembership: (itemId: string, workspaceId: string) =>
    invoke<ContextMembership>("dismiss_context_membership", {
      itemId,
      workspaceId,
    }),

  deleteContextMembership: (id: string) =>
    invoke<void>("delete_context_membership", { id }),

  recalculateWorkspaceMemberships: (workspaceId: string, triggerItemId?: string) =>
    invoke<ContextMembership[]>("recalculate_workspace_suggested_memberships", {
      workspaceId,
      triggerItemId: triggerItemId ?? null,
    }),

  // Context Timeline
  getContextTimeline: (workspaceId: string) =>
    invoke<ContextTimeline>("get_context_timeline", { workspaceId }),

  // Context Explanation
  explainContextAssociation: (itemId: string, workspaceId: string) =>
    invoke<ContextExplanation>("explain_context_association", { itemId, workspaceId }),

  // Temporal Context Weighting
  getTemporalDecayConfig: () =>
    invoke<TemporalDecayConfig>("get_temporal_decay_config"),

  updateTemporalDecayConfig: (config: TemporalDecayConfig) =>
    invoke<void>("update_temporal_decay_config", { config }),

  // Workspace Lifecycle
  updateWorkspaceLifecycleState: (id: string, state: "active" | "dormant" | "archived") =>
    invoke<Workspace>("update_workspace_lifecycle_state", { id, state }),

  archiveWorkspace: (id: string) =>
    invoke<Workspace>("archive_workspace", { id }),

  unarchiveWorkspace: (id: string) =>
    invoke<Workspace>("unarchive_workspace", { id }),

  evaluateWorkspaceLifecycle: (id: string, inactivityDaysThreshold?: number) =>
    invoke<string>("evaluate_workspace_lifecycle", {
      id,
      inactivityDaysThreshold: inactivityDaysThreshold ?? null,
    }),

  // Context Overview
  getContextOverview: (workspaceId: string) =>
    invoke<ContextOverview>("get_context_overview", { workspaceId }),

  // AI Context Reasoning
  analyzeItemContextAI: (itemId: string, isUserExplicit: boolean = false) =>
    invoke<AIContextReasoningResponse>("analyze_item_context_ai", { itemId, isUserExplicit }),

  getEvidencePackage: (itemId: string) =>
    invoke<EvidencePackage>("get_evidence_package", { itemId }),

  getCandidateEvidencePackage: (candidateId: string) =>
    invoke<EvidencePackage>("get_candidate_evidence_package", { candidateId }),

  suggestCandidateNameAI: (candidateId: string) =>
    invoke<string>("suggest_candidate_name_ai", { candidateId }),

  suggestItemGroupNameAI: (itemIds: string[]) =>
    invoke<string>("suggest_item_group_name_ai", { itemIds }),

  clearAiReasoningCache: () =>
    invoke<number>("clear_ai_reasoning_cache"),

  // Cross-Platform Capture
  captureRawEvent: (payload: RawCapturePayload) =>
    invoke<CaptureResult>("capture_raw_event", { payload }),

  // Global Capture Pause
  toggleCapturePause: () =>
    invoke<boolean>("toggle_capture_pause"),

  setCapturePaused: (paused: boolean) =>
    invoke<boolean>("set_capture_paused", { paused }),

  isCapturePaused: () =>
    invoke<boolean>("is_capture_paused"),

  // Context-Aware Inbox State Machine (Day 12)
  getInboxItemState: (itemId: string) =>
    invoke<InboxItemState | null>("get_inbox_item_state", { itemId }),

  transitionInboxItemState: (itemId: string, targetState: InboxState, reason?: string) =>
    invoke<InboxItemState>("transition_inbox_item_state", {
      itemId,
      targetState,
      reason: reason ?? null,
    }),

  listInboxItemStates: (stateFilter?: InboxState) =>
    invoke<InboxItemState[]>("list_inbox_item_states", {
      stateFilter: stateFilter ?? null,
    }),

  // Inbox Attention Engine (Day 12)
  evaluateInboxItemAttention: (itemId: string, config?: AttentionConfig) =>
    invoke<ItemAttention>("evaluate_inbox_item_attention", {
      itemId,
      config: config ?? null,
    }),

  evaluateInboxAttentionBatch: (config?: AttentionConfig) =>
    invoke<ItemAttention[]>("evaluate_inbox_attention_batch", {
      config: config ?? null,
    }),
};


