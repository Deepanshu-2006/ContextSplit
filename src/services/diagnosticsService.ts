import { invoke } from '@tauri-apps/api/core';

export interface DiagnosticsSafetyStatus {
  mode: 'disabled' | 'local_dev' | 'custom';
  is_enabled: boolean;
  total_decision_traces: number;
  total_performance_spans: number;
  max_storage_capacity: number;
  local_first_guaranteed: boolean;
  privacy_guarantees: string[];
}

export type DecisionType =
  | 'context_assignment'
  | 'context_rejection'
  | 'context_transition'
  | 'task_assignment'
  | 'memory_creation'
  | 'memory_importance'
  | 'snapshot_creation'
  | 'lifecycle_transition'
  | 'context_reconstruction'
  | 'resume_readiness'
  | 'feedback_adaptation';

export interface EvidenceConsideredTrace {
  evidence_name: string;
  category: string;
  raw_value_redacted: string;
  source_file?: string;
  app_name?: string;
  domain?: string;
  timestamp?: string;
  strength: number;
  strength_rating?: string;
  weight: number;
  is_contradiction: boolean;
  is_quarantine_trigger: boolean;
  explanation: string;
}

export interface CandidateContextTrace {
  context_id: string;
  context_name: string;
  total_score: number;
  is_winner: boolean;
  score_gap_to_winner: number;
  status: string;
}

export interface FeedbackConsideredTrace {
  feedback_id: string;
  action: string;
  scope: string;
  precedence_tier: number;
  is_applied: boolean;
  rationale: string;
}

export interface ContextDecisionTrace {
  decision_id: string;
  timestamp: string;
  decision_type: DecisionType;
  source_entity: {
    entity_id: string;
    entity_type: string;
    app_name?: string;
    title_hint?: string;
  };
  evidence_considered: EvidenceConsideredTrace[];
  candidate_contexts: CandidateContextTrace[];
  feedback_considered: FeedbackConsideredTrace[];
  final_decision: {
    status: string;
    primary_context_id?: string;
    primary_context_name?: string;
    secondary_context_id?: string;
    secondary_context_name?: string;
    resolution_strategy?: string;
    summary_verdict: string;
  };
  confidence_evaluated: number;
  uncertainty_detected: boolean;
  reason: string;
  processing_duration_ms: number;
}

export interface ContextDecisionExplanation {
  decision_id: string;
  headline: string;
  primary_evidence: string[];
  supporting_evidence: string[];
  contradictory_evidence: string[];
  rejected_alternatives: string[];
  uncertainty_notes: string[];
  final_decision: string;
  summary_paragraph: string;
  disclaimer: string;
}

export interface ContextLifecycleTransitionExplanation {
  workspace_id: string;
  context_name: string;
  previous_state: string;
  current_state: string;
  transition_reason: string;
  why_transitioned: string[];
  active_evidence: string[];
  idle_duration_str?: string;
  transition_trigger: string;
  timestamp: string;
  disclaimer: string;
}

export interface ContextMemoryProvenanceExplanation {
  memory_id: string;
  memory_statement: string;
  context_id: string;
  context_name: string;
  why_stored: string[];
  source: string;
  provenance: string;
  importance_band: string;
  importance_score: number;
  confidence: number;
  timestamp: string;
  disclaimer: string;
}

export interface ContextFeedbackEffectExplanation {
  target_id: string;
  target_label: string;
  context_id?: string;
  context_name: string;
  original_feedback_action?: string;
  scope: string;
  effective_state: string;
  later_conflicting_feedback?: string;
  final_effective_decision: string;
  why_effect_applied: string[];
  precedence_tier_name: string;
  is_overridden: boolean;
  timestamp: string;
  disclaimer: string;
}

export interface HealthIssue {
  issue_id: string;
  category: string;
  severity: 'warning' | 'error';
  title: string;
  description: string;
  affected_entities: string[];
  suggested_action: string;
  timestamp: string;
}

export interface ContextHealthReport {
  status: 'pass' | 'warning' | 'error';
  total_contexts_checked: number;
  total_issues_found: number;
  warning_count: number;
  error_count: number;
  issues: HealthIssue[];
  checked_at: string;
  execution_duration_us: number;
}

export interface ConsistencyIssue {
  issue_id: string;
  system_a: string;
  system_b: string;
  entity_id: string;
  severity: 'warning' | 'error';
  description: string;
  inconsistency_type: string;
  suggested_action: string;
}

export interface CrossSystemConsistencyReport {
  status: 'pass' | 'warning' | 'error';
  systems_checked: string[];
  total_comparisons: number;
  inconsistency_count: number;
  issues: ConsistencyIssue[];
  checked_at: string;
  execution_duration_us: number;
}

export interface CategoryPerformanceSummary {
  category: string;
  operation_count: number;
  total_duration_us: number;
  average_duration_us: number;
  max_duration_us: number;
  expensive_operations_count: number;
  total_db_queries: number;
}

export interface PerformanceSpan {
  span_id: string;
  category: string;
  name: string;
  context_id?: string;
  duration_us: number;
  db_queries_count: number;
  items_processed?: number;
  cache_hit?: boolean;
  fingerprint?: string;
  timestamp: string;
  tags: Record<string, string>;
  is_expensive: boolean;
  is_excessive_db: boolean;
}

export interface PerformanceDiagnosticsReport {
  generated_at: string;
  total_spans_analyzed: number;
  category_summaries: Record<string, CategoryPerformanceSummary>;
  n_plus_one_warnings: Array<{
    operation_name: string;
    context_id?: string;
    total_query_count: number;
    description: string;
  }>;
  unnecessary_recomputations: Array<{
    operation_name: string;
    fingerprint: string;
    occurrence_count: number;
    total_wasted_duration_us: number;
    description: string;
  }>;
  repeated_semantic_processing: Array<{
    operation_name: string;
    fingerprint: string;
    repeat_count: number;
    description: string;
  }>;
  expensive_operations: PerformanceSpan[];
}

export interface TaskCandidate {
  id: string;
  context_id: string;
  title: string;
  confidence: number;
  status: string;
  first_detected_at: string;
  last_active_at: string;
  created_at: string;
  updated_at: string;
}

export const diagnosticsService = {
  getSafetyStatus: async (): Promise<DiagnosticsSafetyStatus> => {
    return await invoke<DiagnosticsSafetyStatus>('get_diagnostics_safety_status');
  },

  setDiagnosticsEnabled: async (enabled: boolean): Promise<DiagnosticsSafetyStatus> => {
    return await invoke<DiagnosticsSafetyStatus>('set_diagnostics_enabled', { enabled });
  },

  purgeAllDiagnostics: async (): Promise<void> => {
    return await invoke<void>('purge_all_diagnostics');
  },

  getRecentDecisionTraces: async (limit?: number, decisionType?: string): Promise<ContextDecisionTrace[]> => {
    return await invoke<ContextDecisionTrace[]>('get_recent_decision_traces', {
      limit: limit ?? 50,
      decisionType: decisionType || null,
    });
  },

  explainDecisionTrace: async (decisionId: string): Promise<ContextDecisionExplanation | null> => {
    return await invoke<ContextDecisionExplanation | null>('explain_decision_trace', { decisionId });
  },

  explainWorkspaceLifecycle: async (workspaceId: string): Promise<ContextLifecycleTransitionExplanation> => {
    return await invoke<ContextLifecycleTransitionExplanation>('explain_workspace_lifecycle', { workspaceId });
  },

  explainWorkspaceMemories: async (workspaceId: string): Promise<ContextMemoryProvenanceExplanation[]> => {
    return await invoke<ContextMemoryProvenanceExplanation[]>('explain_workspace_memories', { workspaceId });
  },

  explainWorkspaceFeedback: async (workspaceId: string, targetId?: string): Promise<ContextFeedbackEffectExplanation> => {
    return await invoke<ContextFeedbackEffectExplanation>('explain_workspace_feedback', {
      workspaceId,
      targetId: targetId || null,
    });
  },

  runContextHealthChecks: async (): Promise<ContextHealthReport> => {
    return await invoke<ContextHealthReport>('run_context_health_checks');
  },

  runCrossSystemConsistencyChecks: async (): Promise<CrossSystemConsistencyReport> => {
    return await invoke<CrossSystemConsistencyReport>('run_cross_system_consistency_checks');
  },

  getPerformanceTracesSummary: async (): Promise<PerformanceDiagnosticsReport> => {
    return await invoke<PerformanceDiagnosticsReport>('get_performance_traces_summary');
  },

  getRecentPerformanceSpans: async (limit?: number): Promise<PerformanceSpan[]> => {
    return await invoke<PerformanceSpan[]>('get_recent_performance_spans', { limit: limit ?? 100 });
  },

  listAllTaskCandidates: async (contextId?: string, status?: string): Promise<TaskCandidate[]> => {
    return await invoke<TaskCandidate[]>('list_all_task_candidates', {
      contextId: contextId || null,
      status: status || null,
    });
  },
};
