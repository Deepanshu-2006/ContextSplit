import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { contextInteractionService } from './contextInteractionService';

// Mock the Tauri invoke function
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('contextInteractionService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('getActiveContext calls get_last_active_context', async () => {
    const mockContext = { id: 'ctx1', name: 'Work' };
    vi.mocked(invoke).mockResolvedValueOnce(mockContext);
    
    const result = await contextInteractionService.getActiveContext();
    expect(invoke).toHaveBeenCalledWith('get_last_active_context');
    expect(result).toEqual(mockContext);
  });

  it('getRecentContexts calls get_recent_work_sequence and unwraps the engine envelope', async () => {
    const entries = [{ id: 'ctx1' }];
    vi.mocked(invoke).mockResolvedValueOnce({ entries });

    const result = await contextInteractionService.getRecentContexts(10);
    expect(invoke).toHaveBeenCalledWith('get_recent_work_sequence', { limitDays: 10 });
    expect(result).toEqual(entries);
  });

  it('getContextSummary calls get_context_overview', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ overview: true });
    
    await contextInteractionService.getContextSummary('ctx1');
    expect(invoke).toHaveBeenCalledWith('get_context_overview', { contextId: 'ctx1' });
  });

  it('getContextWorkset calls get_context_workset', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ items: [] });
    
    await contextInteractionService.getContextWorkset('ctx1');
    expect(invoke).toHaveBeenCalledWith('get_context_workset', { contextId: 'ctx1' });
  });

  it('getContextState calls get_context_state', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ state: 'Active' });
    
    await contextInteractionService.getContextState('ctx1');
    expect(invoke).toHaveBeenCalledWith('get_context_state', { contextId: 'ctx1' });
  });

  it('getResumeReadiness calls get_resume_state', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ readiness: 'High' });
    
    await contextInteractionService.getResumeReadiness('ctx1');
    expect(invoke).toHaveBeenCalledWith('get_resume_state', { contextId: 'ctx1' });
  });

  it('resumeContext emits a resume user_action signal via create_context_signal', async () => {
    await contextInteractionService.resumeContext('ctx1');
    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      workspaceId: 'ctx1',
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'resume', source: 'InteractionAPI' })
    });
  });

  it('pauseContext emits a pause user_action signal via create_context_signal', async () => {
    await contextInteractionService.pauseContext('ctx1');
    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      workspaceId: 'ctx1',
      itemId: null,
      signalType: 'user_action',
      sessionId: null,
      payload: JSON.stringify({ action: 'pause', source: 'InteractionAPI' })
    });
  });

  it('captureItem calls handle_capture', async () => {
    const mockItem = { type: 'text', content: 'test' };
    await contextInteractionService.captureItem(mockItem);
    expect(invoke).toHaveBeenCalledWith('handle_capture', { req: mockItem });
  });

  it('assignItemToContext calls bulk_assign_inbox_items_to_workspace', async () => {
    await contextInteractionService.assignItemToContext('item1', 'ctx1');
    expect(invoke).toHaveBeenCalledWith('bulk_assign_inbox_items_to_workspace', {
      itemIds: ['item1'],
      workspaceId: 'ctx1'
    });
  });

  it('searchContexts calls search_inbox_items with a params envelope', async () => {
    await contextInteractionService.searchContexts('query');
    expect(invoke).toHaveBeenCalledWith('search_inbox_items', {
      params: { query: 'query', enableSemantic: true }
    });
  });

  it('getContextSuggestions calls list_item_context_memberships', async () => {
    await contextInteractionService.getContextSuggestions('item1');
    expect(invoke).toHaveBeenCalledWith('list_item_context_memberships', { itemId: 'item1' });
  });
});
