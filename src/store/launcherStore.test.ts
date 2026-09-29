import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from './launcherStore';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';

vi.mock('../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
    getResumeReadiness: vi.fn(),
    getContextState: vi.fn(),
    getContextWorkset: vi.fn(),
    resumeContext: vi.fn(),
  },
}));

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

describe('useLauncherStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLauncherStore.setState({
      isOpen: false,
      searchQuery: '',
      selectedIndex: 0,
      selectedContextId: null,
      activePreview: null,
      isLoading: false,
      isLoadingPreview: false,
      error: null,
      previewError: null,
      activeContext: null,
      recentContexts: [],
    });
  });

  it('initializes with default closed state', () => {
    const state = useLauncherStore.getState();
    expect(state.isOpen).toBe(false);
    expect(state.searchQuery).toBe('');
    expect(state.selectedIndex).toBe(0);
    expect(state.selectedContextId).toBeNull();
    expect(state.activePreview).toBeNull();
    expect(state.isLoading).toBe(false);
    expect(state.error).toBeNull();
    expect(state.activeContext).toBeNull();
    expect(state.recentContexts).toEqual([]);
  });

  it('openLauncher opens the launcher and triggers data fetch', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    useLauncherStore.getState().openLauncher();
    expect(useLauncherStore.getState().isOpen).toBe(true);
    expect(contextInteractionService.getActiveContext).toHaveBeenCalled();
  });

  it('closeLauncher closes the launcher and resets query and index', () => {
    useLauncherStore.setState({ isOpen: true, searchQuery: 'test', selectedIndex: 3 });
    useLauncherStore.getState().closeLauncher();

    const state = useLauncherStore.getState();
    expect(state.isOpen).toBe(false);
    expect(state.searchQuery).toBe('');
    expect(state.selectedIndex).toBe(0);
    expect(state.selectedContextId).toBeNull();
  });

  it('toggleLauncher toggles open and closed states', () => {
    useLauncherStore.getState().toggleLauncher();
    expect(useLauncherStore.getState().isOpen).toBe(true);

    useLauncherStore.getState().toggleLauncher();
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });

  it('setSearchQuery updates query and resets index', () => {
    useLauncherStore.setState({ selectedIndex: 5 });
    useLauncherStore.getState().setSearchQuery('research');

    expect(useLauncherStore.getState().searchQuery).toBe('research');
    expect(useLauncherStore.getState().selectedIndex).toBe(0);
  });

  it('setSelectedIndex updates selectedIndex', () => {
    useLauncherStore.getState().setSelectedIndex(4);
    expect(useLauncherStore.getState().selectedIndex).toBe(4);
  });

  it('fetchLauncherData populates active context and recent contexts from Context Engine', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
      context_id: 'ctx-active-1',
      context: 'Aurora Website',
      state: 'active',
      task: 'Hero animation',
      context_confidence: 'high',
      recent_items: [{ id: 'item-1' }],
    });

    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([
      {
        context_id: 'ctx-recent-1',
        context_name: 'Tokyo Trip',
        duration_estimate: 'Active for about 30 minutes',
        task_title: 'Flight tickets',
        task_confidence_band: 'high',
        item_count: 5,
      },
    ]);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready with 3 active workset items',
      },
      formatted_resume_summary: 'Continue hero animation',
    } as any);

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.isLoading).toBe(false);
    expect(state.error).toBeNull();
    expect(state.activeContext).toEqual({
      id: 'ctx-active-1',
      name: 'Aurora Website',
      isActive: true,
      state: 'active',
      lastActiveAt: null,
      taskTitle: 'Hero animation',
      confidence: 'high',
      itemCount: 1,
      memoryStatement: null,
      recentActivitySummary: null,
      resumeReadiness: {
        readiness: 'ready',
        formattedExplanation: 'Ready with 3 active workset items',
        summary: 'Continue hero animation',
      },
    });
    expect(state.recentContexts).toHaveLength(1);
    expect(state.recentContexts[0].id).toBe('ctx-recent-1');
    expect(state.recentContexts[0].name).toBe('Tokyo Trip');
  });

  it('fetchLauncherData sets activeContext to null when no context is active', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.activeContext).toBeNull();
  });

  it('fetchLauncherData preserves exact engine recency ordering and prevents duplicate IDs', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce({
      context_id: 'ctx-1',
      context: 'Active Context',
      state: 'active',
    });

    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([
      { context_id: 'ctx-1', context_name: 'Active Context Duplicate' },
      { context_id: 'ctx-2', context_name: 'Alpha Project', duration_estimate: 'Active for about 10m' },
      { context_id: 'ctx-3', context_name: 'Beta Project', session_count: 3 },
      { context_id: 'ctx-2', context_name: 'Alpha Duplicate' },
      { context_id: 'ctx-4', context_name: 'Gamma Project' },
    ]);

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ctx-3',
          name: 'Beta Project',
          created_at: '2026-09-30T00:00:00Z',
          updated_at: '2026-09-30T00:00:00Z',
          lifecycle_state: 'dormant',
        },
      ],
    });

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.activeContext?.id).toBe('ctx-1');
    expect(state.recentContexts).toHaveLength(3);
    expect(state.recentContexts.map((c) => c.id)).toEqual(['ctx-2', 'ctx-3', 'ctx-4']);
    expect(state.recentContexts[1].state).toBe('dormant');
    expect(state.recentContexts[1].sessionCount).toBe(3);
  });

  it('selectContextById loads preview model from state, workset, and readiness APIs', async () => {
    vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
      context_id: 'ctx-alpha',
      context_name: 'Alpha Project',
      state: 'active',
      task_title: 'API Integration',
      last_meaningful_activity: 'Refactored auth endpoint',
      freshness_display: 'Active 5m ago',
    } as any);

    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
      context_id: 'ctx-alpha',
      context_name: 'Alpha Project',
      items: [
        {
          id: 'item-1',
          title: 'api_client.rs',
          item_type: 'code',
          app_name: 'VS Code',
          relevance: 'primary',
          content_snippet: 'pub struct ApiClient',
        } as any,
      ],
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      context_id: 'ctx-alpha',
      context_name: 'Alpha Project',
      readiness: {
        readiness: 'ready',
        formatted_explanation: 'Ready with active API code workset',
        reasons: ['Recent code edits in VS Code'],
      },
      formatted_resume_summary: 'Continue API integration',
    } as any);

    useLauncherStore.getState().selectContextById('ctx-alpha');

    // Wait for 50ms debounce
    await new Promise((resolve) => setTimeout(resolve, 60));

    const state = useLauncherStore.getState();
    expect(state.selectedContextId).toBe('ctx-alpha');
    expect(state.isLoadingPreview).toBe(false);
    expect(state.activePreview).not.toBeNull();
    expect(state.activePreview?.name).toBe('Alpha Project');
    expect(state.activePreview?.taskTitle).toBe('API Integration');
    expect(state.activePreview?.readinessBand).toBe('ready');
    expect(state.activePreview?.worksetItems).toHaveLength(1);
    expect(state.activePreview?.worksetItems[0].title).toBe('api_client.rs');
    expect(state.activePreview?.isAmbiguous).toBe(false);
    expect(state.activePreview?.isStale).toBe(false);
  });

  it('selectContextById handles ambiguous and dormant context states', async () => {
    vi.mocked(contextInteractionService.getContextState).mockResolvedValueOnce({
      context_id: 'ctx-dormant',
      context_name: 'Archived Project',
      state: 'uncertain',
    } as any);

    vi.mocked(contextInteractionService.getContextWorkset).mockResolvedValueOnce({
      context_id: 'ctx-dormant',
      context_name: 'Archived Project',
      items: [],
    } as any);

    vi.mocked(contextInteractionService.getResumeReadiness).mockResolvedValueOnce({
      context_id: 'ctx-dormant',
      context_name: 'Archived Project',
      readiness: {
        readiness: 'insufficient',
        formatted_explanation: 'Insufficient recent evidence',
        reasons: [],
      },
    } as any);

    useLauncherStore.getState().selectContextById('ctx-dormant');
    await new Promise((resolve) => setTimeout(resolve, 60));

    const state = useLauncherStore.getState();
    expect(state.activePreview?.isAmbiguous).toBe(true);
    expect(state.activePreview?.readinessBand).toBe('insufficient');
  });

  it('resumeContextById delegates to contextInteractionService.resumeContext and selectWorkspace', async () => {
    vi.mocked(contextInteractionService.resumeContext).mockResolvedValueOnce({ status: 'ok' });
    const selectSpy = vi.spyOn(useWorkspaceStore.getState(), 'selectWorkspace').mockImplementation(() => {});

    useLauncherStore.setState({ isOpen: true });
    await useLauncherStore.getState().resumeContextById('ctx-target-1');

    expect(contextInteractionService.resumeContext).toHaveBeenCalledWith('ctx-target-1');
    expect(selectSpy).toHaveBeenCalledWith('ctx-target-1');
    expect(useLauncherStore.getState().isOpen).toBe(false);
  });
});
