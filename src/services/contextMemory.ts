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

export interface MemoryExplanation {
  summary: string;
  why_reasons: string[];
  supporting_evidence: string[];
  confidence_band: string;
  provenance: InformationProvenance;
  uncertainty_note?: string | null;
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
  is_context_return?: boolean;
  evidence: string[];
  memory_statement: string;
  explanation?: MemoryExplanation | null;
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

export interface WorkSequenceEntry {
  id: string;
  context_id?: string | null;
  context_name: string;
  time_window: string;
  started_at: string;
  ended_at: string;
  duration_seconds: number;
  duration_estimate: string;
  task_title?: string | null;
  task_confidence_band?: string | null;
  task_provenance: InformationProvenance;
  transition_type?: string | null;
  is_context_return: boolean;
  item_count: number;
  recent_items: MemoryItemReference[];
}

export interface WorkSequenceDayGroup {
  date_label: string;
  date_key: string;
  entries: WorkSequenceEntry[];
}

export interface RecentContextUsageRef {
  context_id: string;
  context_name: string;
  session_count: number;
  total_duration_seconds: number;
  duration_estimate: string;
  last_active_at: string;
}

export interface RecentTaskUsageRef {
  title: string;
  context_id?: string | null;
  context_name: string;
  confidence_band: string;
  provenance: InformationProvenance;
  last_active_at: string;
}

export interface RecentWorkSequenceResponse {
  days: WorkSequenceDayGroup[];
  all_entries: WorkSequenceEntry[];
  recent_contexts: RecentContextUsageRef[];
  recent_tasks: RecentTaskUsageRef[];
  context_switches: number;
  context_returns: number;
  last_active_context?: RecentContextUsageRef | null;
  last_active_task?: RecentTaskUsageRef | null;
  sequence_statement: string;
}

export interface HistoricalContextUsage {
  context_id: string;
  context_name: string;
  session_count: number;
  total_duration_seconds: number;
  duration_estimate: string;
  primary_task?: string | null;
  tasks: string[];
  last_active_at: string;
}

export interface HistoricalTaskSummary {
  title: string;
  context_id?: string | null;
  context_name: string;
  confidence_band: string;
  provenance: InformationProvenance;
  last_active_at: string;
}

export interface HistoricalContextMemoryResponse {
  date: string;
  date_label: string;
  contexts_used: HistoricalContextUsage[];
  primary_tasks: HistoricalTaskSummary[];
  context_switches: number;
  context_returns: number;
  most_recent_context?: HistoricalContextUsage | null;
  most_recent_task?: HistoricalTaskSummary | null;
  important_captured_items: MemoryItemReference[];
  meaningful_activity_segments: WorkSequenceEntry[];
  has_sufficient_evidence: boolean;
  summary_statement: string;
  explanation?: MemoryExplanation | null;
}

/**
 * Dedicated ContextMemoryQuery and WorkSequence service independent from React.
 * Answers "What was I working on?" and reconstructs cross-context work sequences.
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

  /**
   * Reconstructs the user's chronological cross-context recent work sequence.
   */
  async getRecentWorkSequence(limitDays?: number): Promise<RecentWorkSequenceResponse | null> {
    try {
      return await invoke<RecentWorkSequenceResponse>("get_recent_work_sequence", {
        limitDays: limitDays ?? null,
      });
    } catch (err) {
      console.error("Failed to fetch recent work sequence:", err);
      return null;
    }
  },

  /**
   * Reconstructs historical contextual working memory for "Today".
   */
  async getTodayMemory(): Promise<HistoricalContextMemoryResponse | null> {
    try {
      return await invoke<HistoricalContextMemoryResponse>("get_today_memory");
    } catch (err) {
      console.error("Failed to fetch today's memory:", err);
      return null;
    }
  },

  /**
   * Reconstructs historical contextual working memory for "Yesterday".
   */
  async getYesterdayMemory(): Promise<HistoricalContextMemoryResponse | null> {
    try {
      return await invoke<HistoricalContextMemoryResponse>("get_yesterday_memory");
    } catch (err) {
      console.error("Failed to fetch yesterday's memory:", err);
      return null;
    }
  },

  /**
   * Reconstructs historical contextual working memory for a date string ("today", "yesterday", "YYYY-MM-DD").
   */
  async getHistoricalMemoryForDate(date: string): Promise<HistoricalContextMemoryResponse | null> {
    try {
      return await invoke<HistoricalContextMemoryResponse>("get_historical_memory_for_date", {
        date,
      });
    } catch (err) {
      console.error(`Failed to fetch historical memory for '${date}':`, err);
      return null;
    }
  },

  /**
   * Queries historical memories for the past N active days.
   */
  async getRecentHistoricalMemories(limitDays?: number): Promise<HistoricalContextMemoryResponse[]> {
    try {
      return await invoke<HistoricalContextMemoryResponse[]>("get_recent_historical_memories", {
        limitDays: limitDays ?? null,
      });
    } catch (err) {
      console.error("Failed to fetch recent historical memories:", err);
      return [];
    }
  },

  /**
   * Submits explicit user feedback on contextual memory reconstruction.
   * Positive = "Correct" (confirms candidate), Negative = "Not quite" (dismisses candidate).
   */
  async submitMemoryFeedback(
    contextId: string,
    taskTitle?: string | null,
    taskId?: string | null,
    isPositive: boolean = true
  ): Promise<boolean> {
    try {
      return await invoke<boolean>("submit_memory_feedback", {
        contextId,
        taskTitle: taskTitle || null,
        taskId: taskId || null,
        isPositive,
      });
    } catch (err) {
      console.error("Failed to submit memory feedback:", err);
      return false;
    }
  },

  /**
   * Reconstructs derived important contextual memories for a context.
   */
  async getContextImportantMemories(contextId: string): Promise<ContextImportantMemorySet | null> {
    try {
      return await invoke<ContextImportantMemorySet>("get_context_important_memories", {
        contextId,
      });
    } catch (err) {
      console.error(`Failed to fetch important memories for '${contextId}':`, err);
      return null;
    }
  },

  /**
   * Searches contextual memories and items within a context.
   */
  async searchContextMemory(
    contextId: string,
    query: string,
    limit?: number
  ): Promise<ContextImportantMemory[]> {
    try {
      return await invoke<ContextImportantMemory[]>("search_context_memory", {
        contextId,
        query,
        limit: limit ?? null,
      });
    } catch (err) {
      console.error(`Failed to search context memory for '${contextId}':`, err);
      return [];
    }
  },

  /**
   * Finds matching important memories within a context.
   */
  async findImportantMemory(
    contextId: string,
    query: string,
    limit?: number
  ): Promise<ContextImportantMemory[]> {
    try {
      return await invoke<ContextImportantMemory[]>("find_important_memory", {
        contextId,
        query,
        limit: limit ?? null,
      });
    } catch (err) {
      console.error(`Failed to find important memory for '${contextId}':`, err);
      return [];
    }
  },

  /**
   * Retrieves important memories for a context ordered by importance.
   */
  async getImportantMemories(
    contextId: string,
    limit?: number
  ): Promise<ContextImportantMemory[]> {
    try {
      return await invoke<ContextImportantMemory[]>("get_important_memories", {
        contextId,
        limit: limit ?? null,
      });
    } catch (err) {
      console.error(`Failed to get important memories for '${contextId}':`, err);
      return [];
    }
  },

  /**
   * Retrieves recent memories for a context ordered by recency.
   */
  async getRecentMemories(
    contextId: string,
    limit?: number
  ): Promise<ContextImportantMemory[]> {
    try {
      return await invoke<ContextImportantMemory[]>("get_recent_memories", {
        contextId,
        limit: limit ?? null,
      });
    } catch (err) {
      console.error(`Failed to get recent memories for '${contextId}':`, err);
      return [];
    }
  },
};

export type ImportantMemoryType =
  | 'important_item'
  | 'important_reference'
  | 'recurring_reference'
  | 'confirmed_context_fact'
  | 'active_task'
  | 'recent_task'
  | 'meaningful_state';

export type MemoryImportanceBand = 'critical' | 'high' | 'medium' | 'low';

export interface ImportantMemoryMetadata {
  item_type?: string | null;
  app_name?: string | null;
  domain?: string | null;
  action_verb?: string | null;
  target_subject?: string | null;
  session_count?: number | null;
  streak_count?: number | null;
  state_value?: string | null;
  tags?: string[];
}

export interface ContextImportantMemory {
  id: string;
  context_id: string;
  source_id: string;
  memory_type: ImportantMemoryType;
  importance: number;
  importance_band: MemoryImportanceBand;
  confidence: number;
  first_seen_at?: string | null;
  last_seen_at?: string | null;
  provenance: InformationProvenance;
  title: string;
  detail?: string | null;
  explanation: {
    summary: string;
    why_reasons: string[];
    supporting_evidence: string[];
    confidence_band: string;
    provenance: InformationProvenance;
    uncertainty_note?: string | null;
  };
  metadata: ImportantMemoryMetadata;
}

export interface ContextImportantMemorySet {
  context_id: string;
  context_name: string;
  memories: ContextImportantMemory[];
  summary_statement: string;
  generated_at: string;
}

// Standalone function exports matching the required method names
export const getCurrentMemory = contextMemoryService.getCurrentMemory;
export const getContextMemory = contextMemoryService.getContextMemory;
export const getRecentContextMemories = contextMemoryService.getRecentContextMemories;
export const getLastActiveContext = contextMemoryService.getLastActiveContext;
export const getLastActiveTask = contextMemoryService.getLastActiveTask;
export const getRecentWorkSequence = contextMemoryService.getRecentWorkSequence;
export const getTodayMemory = contextMemoryService.getTodayMemory;
export const getYesterdayMemory = contextMemoryService.getYesterdayMemory;
export const getHistoricalMemoryForDate = contextMemoryService.getHistoricalMemoryForDate;
export const getRecentHistoricalMemories = contextMemoryService.getRecentHistoricalMemories;
export const submitMemoryFeedback = contextMemoryService.submitMemoryFeedback;
export const getContextImportantMemories = contextMemoryService.getContextImportantMemories;
export const searchContextMemory = contextMemoryService.searchContextMemory;
export const findImportantMemory = contextMemoryService.findImportantMemory;
export const getImportantMemories = contextMemoryService.getImportantMemories;
export const getRecentMemories = contextMemoryService.getRecentMemories;


