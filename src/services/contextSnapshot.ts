import { invoke } from "@tauri-apps/api/core";
import { ContextStateValue, ResumeReadiness, ResumeReadinessAssessment, StateFreshness } from "./contextState";

/**
 * Epistemic provenance of contextual snapshot information.
 */
export type InformationProvenance = "Observed" | "Inferred" | "Confirmed";

/**
 * Quality rating classification for a contextual snapshot.
 */
export type SnapshotQualityRating = "complete" | "useful" | "partial" | "insufficient";

/**
 * Quality control evaluation for a contextual snapshot.
 */
export interface ContextSnapshotQuality {
  rating: SnapshotQualityRating;
  score: number;
  context_confidence: number;
  task_confidence?: number | null;
  workset_quality_score: number;
  strong_workset_count: number;
  total_workset_count: number;
  recent_activity_count: number;
  has_meaningful_activity: boolean;
  state_freshness: StateFreshness;
  evidence_diversity_score: number;
  distinct_apps_count: number;
  has_contradictions: boolean;
  contradictions: string[];
  reasons: string[];
  explanation: string;
  can_confidently_resume: boolean;
}

/**
 * Lightweight reference to a workset item in a snapshot.
 */
export interface SnapshotWorksetItemReference {
  id: string;
  title?: string | null;
  item_type: string;
  relevance: string;
  relevance_score: number;
  provenance: InformationProvenance;
  app_name?: string | null;
  domain?: string | null;
  content_snippet?: string | null;
  reasons: string[];
  created_at: string;
  last_active_at?: string | null;
}

/**
 * Lightweight reference to a chronological activity in a snapshot.
 */
export interface SnapshotActivityReference {
  id?: string | null;
  entry_type: string;
  description: string;
  timestamp: string;
  provenance: InformationProvenance;
}

/**
 * Lightweight reference to a task in a snapshot.
 */
export interface SnapshotTaskReference {
  id?: string | null;
  title: string;
  action_verb?: string | null;
  target_subject?: string | null;
  confidence: number;
  confidence_band: string;
  provenance: InformationProvenance;
  first_seen_at?: string | null;
  last_active_at?: string | null;
  status?: string | null;
}

/**
 * Point-in-time contextual snapshot representing what a user's work looked like inside a context.
 */
export interface ContextSnapshot {
  id: string;
  context_id: string;
  context_name: string;
  task_id?: string | null;
  task_title?: string | null;
  task_provenance?: InformationProvenance | null;
  task_confidence?: number | null;
  created_at: string;
  last_activity_at?: string | null;
  state: ContextStateValue;
  state_freshness: StateFreshness;
  state_freshness_display: string;
  resume_readiness: ResumeReadinessAssessment;
  workset_item_references: SnapshotWorksetItemReference[];
  recent_activity_references: SnapshotActivityReference[];
  recent_task_references: SnapshotTaskReference[];
  snapshot_confidence: number;
  snapshot_statement: string;
  quality: ContextSnapshotQuality;
  provenance: InformationProvenance;
  epistemic_disclaimer: string;
}

/**
 * Structured factual difference between two snapshots of the same context.
 */
export interface ContextSnapshotDiff {
  context_id: string;
  context_name: string;
  from_snapshot_id?: string | null;
  to_snapshot_id: string;
  from_timestamp?: string | null;
  to_timestamp: string;
  task_changed: boolean;
  previous_task?: string | null;
  current_task?: string | null;
  task_change_summary?: string | null;
  state_changed: boolean;
  previous_state?: ContextStateValue | null;
  current_state: ContextStateValue;
  added_items: SnapshotWorksetItemReference[];
  removed_items: SnapshotWorksetItemReference[];
  retained_items: SnapshotWorksetItemReference[];
  readiness_changed: boolean;
  previous_readiness?: ResumeReadiness | null;
  current_readiness: ResumeReadiness;
  confidence_delta: number;
  formatted_diff_summary: string;
  has_meaningful_changes: boolean;
}

/**
 * Chronological history of snapshots for a context.
 */
export interface ContextSnapshotHistory {
  context_id: string;
  context_name: string;
  snapshots: ContextSnapshot[];
  max_capacity: number;
}

/**
 * Frontend client service for querying context snapshots and diffs.
 */
export const contextSnapshotService = {
  /**
   * Retrieves the point-in-time snapshot for a context.
   */
  async getSnapshot(contextId: string): Promise<ContextSnapshot | null> {
    try {
      return await invoke<ContextSnapshot>("get_context_snapshot", { contextId });
    } catch (err) {
      console.error(`Failed to get snapshot for context '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Retrieves the snapshot history for a context.
   */
  async getSnapshotHistory(contextId: string): Promise<ContextSnapshotHistory | null> {
    try {
      return await invoke<ContextSnapshotHistory | null>("get_context_snapshot_history", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to get snapshot history for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Computes the factual diff between the latest snapshot and previous snapshot.
   */
  async getSnapshotDiff(contextId: string): Promise<ContextSnapshotDiff | null> {
    try {
      return await invoke<ContextSnapshotDiff | null>("get_context_snapshot_diff", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to get snapshot diff for '${contextId}':`, err);
      return null;
    }
  },
};
