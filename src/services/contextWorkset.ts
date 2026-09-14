import { invoke } from "@tauri-apps/api/core";
import {
  InformationProvenance,
  MemoryExplanation,
  MemoryTaskReference,
} from "./contextMemory";

/**
 * Functional relevance classification for an item within a contextual workset.
 *
 * Distinct from raw rank score; explicitly signifies how this item supports the active context/task.
 */
export type WorksetItemRelevance =
  | "primary"
  | "supporting"
  | "reference"
  | "recent"
  | "related";

/**
 * Operational state of the Context Workset.
 */
export type WorksetState =
  | "active"
  | "recently_active"
  | "detour_return"
  | "dormant"
  | "insufficient_evidence";

/**
 * Lightweight reference to an item within a contextual workset.
 * Invariant: Does NOT duplicate large item payloads, base64 content, or raw vector embeddings.
 */
export interface WorksetItemReference {
  id: string;
  title?: string | null;
  item_type: string;
  relevance: WorksetItemRelevance;
  relevance_score: number;
  reasons: string[];
  provenance: InformationProvenance;
  created_at: string;
  last_active_at?: string | null;
  app_name?: string | null;
  domain?: string | null;
  content_snippet: string;
  is_suggested_addition: boolean;
}

/**
 * Derived representation of the minimal working set of information
 * most useful for continuing work inside a context.
 */
export interface ContextWorkset {
  context_id: string;
  context_name: string;
  task_id?: string | null;
  task_title?: string | null;
  generated_at: string;
  last_active_at?: string | null;
  workset_state: WorksetState;
  items: WorksetItemReference[];
  recent_activity: string;
  relevant_items: WorksetItemReference[];
  important_references: WorksetItemReference[];
  related_tasks: MemoryTaskReference[];
  confidence: number;
  confidence_band: string;
  provenance: InformationProvenance;
  explanation: MemoryExplanation;
}

/**
 * Service for querying and interacting with Context Worksets.
 */
export const contextWorksetService = {
  /**
   * Reconstructs and returns the ContextWorkset for a specific workspace context.
   */
  async getContextWorkset(contextId: string): Promise<ContextWorkset | null> {
    try {
      return await invoke<ContextWorkset>("get_context_workset", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch context workset for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Reconstructs and returns the active ContextWorkset across the system.
   */
  async getActiveWorkset(): Promise<ContextWorkset | null> {
    try {
      return await invoke<ContextWorkset | null>("get_active_workset");
    } catch (err) {
      console.error("Failed to fetch active context workset:", err);
      return null;
    }
  },

  /**
   * Submits explicit user feedback (keep or remove) on a workset item.
   * Invariant: Does not delete the item from the system or database.
   */
  async submitWorksetFeedback(
    contextId: string,
    itemId: string,
    isPositive: boolean
  ): Promise<boolean> {
    try {
      return await invoke<boolean>("submit_workset_feedback", {
        contextId,
        itemId,
        isPositive,
      });
    } catch (err) {
      console.error("Failed to submit workset feedback:", err);
      return false;
    }
  },

  /**
   * Returns items partitioned by relevance category.
   */
  getItemsByRelevance(
    workset: ContextWorkset,
    relevance: WorksetItemRelevance
  ): WorksetItemReference[] {
    return workset.items.filter((item) => item.relevance === relevance);
  },

  /**
   * Generates a concise human-readable working state summary.
   */
  getWorksetSummary(workset: ContextWorkset): string {
    const primary = this.getItemsByRelevance(workset, "primary");
    const supporting = this.getItemsByRelevance(workset, "supporting");

    if (workset.task_title) {
      return `Focused on '${workset.task_title}' with ${primary.length} primary and ${supporting.length} supporting reference(s).`;
    }

    if (workset.items.length === 0) {
      return `Clean context workspace with no recent items.`;
    }

    return `Context active with ${workset.items.length} working item(s).`;
  },
};

export const getContextWorkset = contextWorksetService.getContextWorkset;
export const getActiveWorkset = contextWorksetService.getActiveWorkset;
export const submitWorksetFeedback = contextWorksetService.submitWorksetFeedback;
