import { invoke } from '@tauri-apps/api/core';

export type ReconstructionSectionStatus = 'available' | 'partial' | 'uncertain' | 'unavailable';

export interface ReconstructionContextSection {
  status: ReconstructionSectionStatus;
  id?: string | null;
  name: string;
  lifecycle_state?: string | null;
  provenance: string;
  confidence: number;
}

export interface ReconstructionTaskRef {
  id: string;
  title: string;
  action_verb?: string | null;
  target_subject?: string | null;
  confidence: number;
  confidence_band: string;
  status: string;
  provenance: string;
  first_seen_at?: string | null;
  last_active_at?: string | null;
}

export interface ReconstructionTaskSection {
  status: ReconstructionSectionStatus;
  title?: string | null;
  action_verb?: string | null;
  target_subject?: string | null;
  confidence?: number | null;
  provenance?: string | null;
  task_ref?: ReconstructionTaskRef | null;
  reasons: string[];
  competing_candidates?: ReconstructionTaskRef[];
  conflict_reason?: string | null;
}

export interface ReconstructionStateSection {
  status: ReconstructionSectionStatus;
  state_value?: 'active' | 'recently_active' | 'paused' | 'dormant' | 'uncertain' | 'insufficient_evidence' | null;
  freshness?: 'fresh' | 'moderate' | 'decaying' | 'cold' | 'unknown' | null;
  freshness_display?: string | null;
  last_active_at?: string | null;
  explanation?: string | null;
}

export interface MeaningfulActivity {
  description: string;
  timestamp: string;
  event_type: string;
  item_id?: string | null;
  item_title?: string | null;
  provenance: string;
  confidence: number;
}

export interface ReconstructionMeaningfulActivitySection {
  status: ReconstructionSectionStatus;
  activity?: MeaningfulActivity | null;
  description?: string | null;
  timestamp?: string | null;
  provenance?: string | null;
  confidence?: number | null;
}

export interface ReconstructionWorksetItemRef {
  id: string;
  item_type: string;
  title?: string | null;
  content_snippet?: string | null;
  app_name?: string | null;
  domain?: string | null;
  relevance: 'primary' | 'supporting' | 'peripheral';
  relevance_score: number;
  provenance: string;
  is_pinned: boolean;
  reasons: string[];
  last_active_at?: string | null;
}

export interface ReconstructionWorksetSection {
  status: ReconstructionSectionStatus;
  items: ReconstructionWorksetItemRef[];
  total_count: number;
  primary_count: number;
  supporting_count: number;
  weak_count: number;
  quality_score: number;
  summary: string;
}

export interface ReconstructionImportantMemoryRef {
  id: string;
  source_id: string;
  title: string;
  memory_type: string;
  importance_band: string;
  importance_score: number;
  provenance: string;
  pinned: boolean;
  explanation: string;
  app_name?: string | null;
  domain?: string | null;
  created_at: string;
}

export interface ReconstructionImportantMemoriesSection {
  status: ReconstructionSectionStatus;
  memories: ReconstructionImportantMemoryRef[];
  total_count: number;
  pinned_count: number;
  summary: string;
}

export interface ReconstructionSnapshotRef {
  id: string;
  context_id: string;
  created_at: string;
  quality_rating: string;
  quality_score: number;
  task_title?: string | null;
  workset_count: number;
  snapshot_statement: string;
  can_confidently_resume: boolean;
}

export interface ReconstructionSnapshotSection {
  status: ReconstructionSectionStatus;
  snapshot?: ReconstructionSnapshotRef | null;
}

export interface ReconstructionActivityRef {
  event_type: string;
  description: string;
  timestamp: string;
  item_id?: string | null;
}

export interface ReconstructionRecentActivitySection {
  status: ReconstructionSectionStatus;
  milestones: ReconstructionActivityRef[];
  total_recent_events: number;
}

export interface ReconstructionResumeReadinessSection {
  status: ReconstructionSectionStatus;
  readiness: 'ready' | 'mostly_ready' | 'needs_alignment' | 'insufficient' | 'unavailable';
  reasons: string[];
  blockers: string[];
  suggested_action: string;
}

export interface ReconstructionConfidence {
  score: number;
  band: string;
  reasons: string[];
}

export interface ReconstructionQualityFactors {
  context_confidence: string;
  task_confidence: string;
  state_confidence: string;
  workset_quality: string;
  memory_quality: string;
  snapshot_freshness: string;
  evidence_consistency: string;
  contradiction_count: number;
  data_freshness: string;
  contradictions: string[];
  gaps: string[];
}

export interface ContextReconstructionQuality {
  rating: 'coherent' | 'mostly_coherent' | 'partial' | 'uncertain' | 'insufficient';
  display_label: string;
  explanation: string;
  factors: ReconstructionQualityFactors;
  can_confidently_resume: boolean;
}

export interface ContextReconstruction {
  context: ReconstructionContextSection;
  current_task: ReconstructionTaskSection;
  current_state: ReconstructionStateSection;
  last_meaningful_activity: ReconstructionMeaningfulActivitySection;
  workset: ReconstructionWorksetSection;
  important_memories: ReconstructionImportantMemoriesSection;
  latest_snapshot: ReconstructionSnapshotSection;
  recent_activity: ReconstructionRecentActivitySection;
  resume_readiness: ReconstructionResumeReadinessSection;
  confidence: ReconstructionConfidence;
  quality: ContextReconstructionQuality;
  disclaimer: string;
  reconstructed_at: string;
}

export const contextReconstructionService = {
  /**
   * Fetches the unified contextual reconstruction for a workspace/context (cached).
   */
  getContextReconstruction: async (workspaceId: string): Promise<ContextReconstruction> => {
    return invoke<ContextReconstruction>('get_context_reconstruction', { workspaceId });
  },

  /**
   * Performs a cold contextual reconstruction bypassing all in-memory caches.
   */
  reconstructCold: async (workspaceId: string): Promise<ContextReconstruction> => {
    return invoke<ContextReconstruction>('reconstruct_context_cold', { workspaceId });
  },
};
