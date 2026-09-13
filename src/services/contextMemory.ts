import { invoke } from "@tauri-apps/api/core";

export type InformationProvenance = "observed" | "inferred" | "confirmed";

export type ContextMemoryActivityState =
  | "active"
  | "recently_active"
  | "dormant"
  | "insufficient_evidence";

export interface MemoryItemReference {
  id: string;
  title?: string | null;
  item_type: string;
  timestamp: string;
  provenance: InformationProvenance;
  app_name?: string | null;
  domain?: string | null;
}

export interface MemoryTaskReference {
  id?: string | null;
  title: string;
  action_verb?: string | null;
  target_subject?: string | null;
  confidence: number;
  confidence_band: string;
  provenance: InformationProvenance;
  first_seen_at?: string | null;
  last_active_at?: string | null;
  streak_count: number;
  status?: string | null;
}

export interface ContextMemoryResponse {
  context_id: string;
  context: string;
  context_confidence: "high" | "medium" | "low" | "none" | string;
  task?: string | null;
  task_confidence?: "high" | "medium" | "low" | "none" | string | null;
  last_active_at?: string | null;
  state: ContextMemoryActivityState | string;
  recent_activity: string;
  recent_items: MemoryItemReference[];
  recent_task?: MemoryTaskReference | null;
  provenance: InformationProvenance;
  evidence: string[];
  memory_statement: string;
}

export interface LastActiveTaskResponse {
  context_id: string;
  task_id?: string | null;
  title: string;
  action_verb?: string | null;
  target_subject?: string | null;
  confidence: number;
  confidence_band: string;
  provenance: InformationProvenance;
  last_active_at?: string | null;
  is_current: boolean;
}

/**
 * Dedicated ContextMemoryQuery service independent from React.
 * Answers "What was I working on?" from derived contextual data.
 */
export const contextMemoryService = {
  /**
   * Reconstructs and returns the active context memory across the system.
   */
  async getCurrentMemory(): Promise<ContextMemoryResponse | null> {
    try {
      return await invoke<ContextMemoryResponse | null>("get_current_memory");
    } catch (err) {
      console.error("Failed to fetch current memory:", err);
      return null;
    }
  },

  /**
   * Reconstructs and returns contextual working memory for a specific context ID.
   */
  async getContextMemory(contextId: string): Promise<ContextMemoryResponse | null> {
    try {
      return await invoke<ContextMemoryResponse | null>("get_context_memory", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch context memory for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Reconstructs and returns recent context memories up to `limit` count.
   */
  async getRecentContextMemories(limit: number = 5): Promise<ContextMemoryResponse[]> {
    try {
      return await invoke<ContextMemoryResponse[]>("get_recent_context_memories", {
        limit,
      });
    } catch (err) {
      console.error("Failed to fetch recent context memories:", err);
      return [];
    }
  },

  /**
   * Returns the single most recently active context memory snapshot.
   */
  async getLastActiveContext(): Promise<ContextMemoryResponse | null> {
    try {
      return await invoke<ContextMemoryResponse | null>("get_last_active_context");
    } catch (err) {
      console.error("Failed to fetch last active context:", err);
      return null;
    }
  },

  /**
   * Returns the last active task inside a specific context.
   */
  async getLastActiveTask(contextId: string): Promise<LastActiveTaskResponse | null> {
    try {
      return await invoke<LastActiveTaskResponse | null>("get_last_active_task", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch last active task for '${contextId}':`, err);
      return null;
    }
  },
};

// Standalone function exports matching the required method names
export const getCurrentMemory = contextMemoryService.getCurrentMemory;
export const getContextMemory = contextMemoryService.getContextMemory;
export const getRecentContextMemories = contextMemoryService.getRecentContextMemories;
export const getLastActiveContext = contextMemoryService.getLastActiveContext;
export const getLastActiveTask = contextMemoryService.getLastActiveTask;
