import { invoke } from "@tauri-apps/api/core";
import { ContextWorkset } from "./contextWorkset";

/**
 * Conservative classification of the user's working state inside a context.
 */
export type ContextStateValue =
  | "active"
  | "recently_active"
  | "paused"
  | "dormant"
  | "uncertain"
  | "insufficient_evidence";

/**
 * Internal classification of how ready ContextSplit is to help the user resume a context.
 *
 * Invariant: Do NOT expose a numerical score to the user.
 */
export type ResumeReadiness =
  | "ready"
  | "mostly_ready"
  | "partial"
  | "insufficient";

/**
 * Detailed evaluation of resume readiness with human-readable rationale.
 */
export interface ResumeReadinessAssessment {
  readiness: ResumeReadiness;
  reasons: string[];
  formatted_explanation: string;
}

/**
 * Activity type classification for a meaningful contextual event.
 */
export type MeaningfulActivityType =
  | "user_organization"
  | "confirmed_activity"
  | "task_transition"
  | "context_transition"
  | "task_related_item"
  | "strongly_related_item"
  | "activity_segment"
  | "peripheral_capture";

/**
 * Entity reference associated with a meaningful activity.
 */
export interface MeaningfulEntityReference {
  entity_type: string;
  id: string;
  title?: string | null;
}

/**
 * Detailed structured representation of the last meaningful activity.
 */
export interface MeaningfulActivity {
  activity_type: MeaningfulActivityType;
  timestamp: string;
  reference?: MeaningfulEntityReference | null;
  provenance: string;
  confidence: number;
  description: string;
  explanation: string;
}

/**
 * Human-readable resume state representing where the user left off in a context.
 *
 * Derived from ContextMemory + ContextWorkset + activity without launching apps/files.
 */
export interface ContextResumeState {
  context_id: string;
  context_name: string;
  left_off_on?: string | null;
  last_meaningful_activity?: string | null;
  last_activity_type?: MeaningfulActivityType | null;
  recent_work: string[];
  formatted_resume_summary: string;
  readiness: ResumeReadinessAssessment;
  state: ContextStateValue;
  confidence: number;
  reconstructed_at: string;
}

/**
 * Single grounded reason explaining why ContextSplit reconstructed this state.
 */
export interface StateReason {
  text: string;
  provenance: "Observed" | "Inferred" | string;
}

/**
 * Structured progressive explanation for ContextState reasoning.
 */
export interface StateExplanation {
  title: string;
  reasons: StateReason[];
  uncertainty_note?: string | null;
}

/**
 * Derived representation of the user's last known working state inside a context.
 */
export interface ContextState {
  context_id: string;
  context_name?: string | null;
  task_id?: string | null;
  task_title?: string | null;
  state: ContextStateValue;
  last_active_at?: string | null;
  last_activity_segment_id?: string | null;
  last_transition_id?: string | null;
  last_meaningful_item_id?: string | null;
  last_meaningful_item_title?: string | null;
  current_workset?: ContextWorkset | null;
  recent_item_ids: string[];
  recent_activity?: string | null;
  active_application?: string | null;
  active_domain?: string | null;
  recent_applications: string[];
  latest_meaningful_action?: string | null;
  last_meaningful_activity: string;
  meaningful_activity?: MeaningfulActivity | null;
  resume_state: ContextResumeState;
  readiness: ResumeReadinessAssessment;
  explanation: StateExplanation;
  state_confidence: number;
  provenance: string;
  has_user_feedback: boolean;
  reconstructed_at: string;
}

/**
 * Service for querying contextual working state and resume state.
 */
export const contextStateService = {
  /**
   * Reconstructs and returns the human-readable resume state for a context.
   *
   * Example:
   * Aurora Website
   *
   * You left off on:
   * Hero animation
   *
   * Last meaningful activity:
   * Framer Motion animation reference
   *
   * Recent work:
   * Figma hero design
   * React Hero component
   * Framer Motion reference
   */
  async getResumeState(contextId: string): Promise<ContextResumeState | null> {
    try {
      return await invoke<ContextResumeState>("get_resume_state", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch resume state for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Evaluates the contextual resume readiness without numerical scores.
   *
   * Example:
   * Resume readiness:
   * Ready
   *
   * Because:
   * - current task identified
   * - 5 relevant workset items
   * - recent activity available
   */
  async evaluateResumeReadiness(
    contextId: string
  ): Promise<ResumeReadinessAssessment | null> {
    try {
      return await invoke<ResumeReadinessAssessment>(
        "evaluate_resume_readiness",
        {
          contextId,
        }
      );
    } catch (err) {
      console.error(`Failed to evaluate resume readiness for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Reconstructs and returns the full ContextState for a specific workspace context.
   */
  async getContextState(contextId: string): Promise<ContextState | null> {
    try {
      return await invoke<ContextState>("get_context_state", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch context state for '${contextId}':`, err);
      return null;
    }
  },
};
