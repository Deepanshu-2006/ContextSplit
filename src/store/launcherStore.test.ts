import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useLauncherStore } from './launcherStore';
import { contextInteractionService } from '../services/contextInteractionService';
import { useWorkspaceStore } from './workspaceStore';

vi.mock('../services/contextInteractionService', () => ({
  contextInteractionService: {
    getActiveContext: vi.fn(),
    getRecentContexts: vi.fn(),
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
      isLoading: false,
      error: null,
      activeContext: null,
      recentContexts: [],
    });
  });

  it('initializes with default closed state', () => {
    const state = useLauncherStore.getState();
    expect(state.isOpen).toBe(false);
    expect(state.searchQuery).toBe('');
    expect(state.selectedIndex).toBe(0);
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
    });
    expect(state.recentContexts).toHaveLength(1);
    expect(state.recentContexts[0].id).toBe('ctx-recent-1');
    expect(state.recentContexts[0].name).toBe('Tokyo Trip');
  });

  it('fetchLauncherData falls back to workspaceStore if engine recents are empty', async () => {
    vi.mocked(contextInteractionService.getActiveContext).mockResolvedValueOnce(null);
    vi.mocked(contextInteractionService.getRecentContexts).mockResolvedValueOnce([]);

    useWorkspaceStore.setState({
      workspaces: [
        {
          id: 'ws-1',
          name: 'Personal Project',
          created_at: '2026-09-30T00:00:00Z',
          updated_at: '2026-09-30T00:00:00Z',
          lifecycle_state: 'active',
        },
      ],
    });

    await useLauncherStore.getState().fetchLauncherData();

    const state = useLauncherStore.getState();
    expect(state.recentContexts).toHaveLength(1);
    expect(state.recentContexts[0].id).toBe('ws-1');
    expect(state.recentContexts[0].name).toBe('Personal Project');
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
