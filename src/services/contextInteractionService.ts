import { invoke } from '@tauri-apps/api/core';
import { ContextState, ContextResumeState, ResumeReadinessAssessment } from './contextState';
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
   */
  getRecentContexts: async (limit: number = 5): Promise<any[]> => {
    return await invoke<any[]>('get_recent_work_sequence', { limit });
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
   */
  resumeContext: async (contextId: string): Promise<void> => {
    // We assume resuming a context involves transitioning an inbox item or creating a transition.
    // Given the Phase 1 architecture, setting a context active is typically handled via context transitions.
    return await invoke<void>('create_context_signal', {
        signalType: 'Resume',
        source: 'InteractionAPI',
        contextId: contextId,
        metadata: null
    });
  },

  /**
   * Pauses the currently active context.
   */
  pauseContext: async (contextId: string): Promise<void> => {
    return await invoke<void>('create_context_signal', {
        signalType: 'Pause',
        source: 'InteractionAPI',
        contextId: contextId,
        metadata: null
    });
  },

  /**
   * Captures a raw item and sends it to the engine.
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
   * Searches across all contexts.
   */
  searchContexts: async (query: string): Promise<any> => {
    return await invoke<any>('search_inbox_items', { query });
  },

  /**
   * Gets suggested contexts for an item based on context discovery.
   */
  getContextSuggestions: async (itemId: string): Promise<any[]> => {
    return await invoke<any[]>('list_item_context_memberships', { itemId });
  }
};
