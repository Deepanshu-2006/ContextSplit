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

  it('getRecentContexts calls get_recent_work_sequence', async () => {
    const mockContexts = [{ id: 'ctx1' }];
    vi.mocked(invoke).mockResolvedValueOnce(mockContexts);
    
    const result = await contextInteractionService.getRecentContexts(10);
    expect(invoke).toHaveBeenCalledWith('get_recent_work_sequence', { limit: 10 });
    expect(result).toEqual(mockContexts);
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

  it('resumeContext calls create_context_signal', async () => {
    await contextInteractionService.resumeContext('ctx1');
    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      signalType: 'Resume',
      source: 'InteractionAPI',
      contextId: 'ctx1',
      metadata: null
    });
  });

  it('pauseContext calls create_context_signal', async () => {
    await contextInteractionService.pauseContext('ctx1');
    expect(invoke).toHaveBeenCalledWith('create_context_signal', {
      signalType: 'Pause',
      source: 'InteractionAPI',
      contextId: 'ctx1',
      metadata: null
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

  it('searchContexts calls search_inbox_items', async () => {
    await contextInteractionService.searchContexts('query');
    expect(invoke).toHaveBeenCalledWith('search_inbox_items', { query: 'query' });
  });

  it('getContextSuggestions calls list_item_context_memberships', async () => {
    await contextInteractionService.getContextSuggestions('item1');
    expect(invoke).toHaveBeenCalledWith('list_item_context_memberships', { itemId: 'item1' });
  });
});
