import { invoke } from '@tauri-apps/api/core';
import { ContextState, ContextResumeState } from './contextState';
import { ContextWorkset } from './contextWorkset';

/**
 * Context Interaction API
 * 
 * This service acts as the definitive UI contract for all interaction surfaces
 * (Main UI, Launcher, Notch, Edge layers) to interact with the Context Engine.
 * It strictly proxies requests to the backend Tauri commands without duplicating
 * business logic or maintaining its own state.
 */
export const contextInteractionService = {
  /**
   * Gets the currently active context (if any).
   */
  getActiveContext: async (): Promise<any> => {
    return await invoke<any>('get_last_active_context');
  },

  /**
   * Retrieves a list of recently active contexts.
   * The engine's `get_recent_work_sequence` command expects a `limitDays` window.
   */
  getRecentContexts: async (limitDays: number = 7): Promise<any[]> => {
    const response = await invoke<any>('get_recent_work_sequence', { limitDays });
    // The engine returns a RecentWorkSequenceResponse envelope; unwrap the sequence for UI surfaces.
    if (response && Array.isArray(response.entries)) return response.entries;
    if (response && Array.isArray(response.recent_contexts)) return response.recent_contexts;
    if (response && Array.isArray(response.all_entries)) return response.all_entries;
    return Array.isArray(response) ? response : [];
  },

  /**
   * Gets a high-level overview of a specific context.
   */
  getContextSummary: async (contextId: string): Promise<any> => {
    return await invoke<any>('get_context_overview', { contextId });
  },

  /**
   * Retrieves the current working set (memories) for a context.
   */
  getContextWorkset: async (contextId: string): Promise<ContextWorkset> => {
    return await invoke<ContextWorkset>('get_context_workset', { contextId });
  },

  /**
   * Gets the current state and freshness of a context.
   */
  getContextState: async (contextId: string): Promise<ContextState> => {
    return await invoke<ContextState>('get_context_state', { contextId });
  },

  /**
   * Evaluates if a context is ready to be resumed.
   */
  getResumeReadiness: async (contextId: string): Promise<ContextResumeState> => {
    return await invoke<ContextResumeState>('get_resume_state', { contextId });
  },

  /**
   * Resumes a specific context (sets it as active).
   * Delegates to the Context Engine's signal operations — no local state is mutated.
   * Backend signature: create_context_signal(workspace_id, item_id, signal_type, session_id, payload)
   */
  resumeContext: async (contextId: string): Promise<unknown> => {
    return await invoke<unknown>('create_context_signal', {
      workspaceId: contextId,
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'resume', source: 'InteractionAPI' })
    });
  },

  /**
   * Pauses the currently active context.
   * Delegates to the Context Engine's signal operations — no local state is mutated.
   */
  pauseContext: async (contextId: string): Promise<unknown> => {
    return await invoke<unknown>('create_context_signal', {
      workspaceId: contextId,
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'pause', source: 'InteractionAPI' })
    });
  },

  /**
   * Captures a raw item and sends it to the engine's CapturePipeline.
   * NOTE (Phase 1 gap): the backend `handle_capture` command currently returns
   * `Result<(), String>` and routes processing asynchronously, so no item ID is
   * returned to the caller. Surfaces needing an ID must resolve it via search.
   */
  captureItem: async (item: any): Promise<any> => {
    // Assumes `item` matches the capture::target::CaptureRequest struct
    return await invoke<any>('handle_capture', { req: item });
  },

  /**
   * Assigns an explicitly captured item to a context.
   */
  assignItemToContext: async (itemId: string, contextId: string): Promise<void> => {
    return await invoke<void>('bulk_assign_inbox_items_to_workspace', {
        itemIds: [itemId],
        workspaceId: contextId
    });
  },

  /**
   * Searches across all contexts (text + optional semantic ranking via the engine).
   * The backend command takes an InboxSearchParams envelope, not a bare query.
   */
  searchContexts: async (query: string): Promise<any[]> => {
    return await invoke<any[]>('search_inbox_items', {
      params: { query, enableSemantic: true }
    });
  },

  /**
   * Gets suggested contexts for an item based on context discovery.
   */
  getContextSuggestions: async (itemId: string): Promise<any[]> => {
    return await invoke<any[]>('list_item_context_memberships', { itemId });
  }
};
